pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod oidc;
pub mod routes;
pub mod state;
pub mod store;

use tower_http::set_header::SetResponseHeaderLayer;
use tracing_subscriber::EnvFilter;

use config::Config;
use state::AppState;

/// Build the Content-Security-Policy. Everything stays same-origin except the
/// Google Fonts hosts halo-design uses and `img-src https:` — demo scripts are
/// markdown authored elsewhere and may embed remote screenshots. HSTS /
/// X-Frame-Options / X-Content-Type-Options are Traefik's job, not the
/// binary's.
///
/// SvelteKit inlines its bootstrap `<script>` in index.html, and its content
/// changes every build (chunk hashes), so we can't use `'self'` alone or a
/// fixed hash. Instead we hash whatever inline scripts the built index.html
/// contains at boot and allow exactly those via `'sha256-…'` — no
/// `'unsafe-inline'`. `style-src` keeps `'unsafe-inline'` for the
/// `style="display:contents"` attribute SvelteKit emits + scoped-style attrs.
fn build_csp(script_hashes: &[String]) -> String {
    let mut script_src = String::from("'self'");
    for h in script_hashes {
        script_src.push(' ');
        script_src.push_str(h);
    }
    format!(
        "default-src 'self'; \
         script-src {script_src}; \
         style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; \
         font-src 'self' data: https://fonts.gstatic.com; \
         img-src 'self' data: blob: https:; \
         connect-src 'self'; \
         frame-ancestors 'none'; \
         base-uri 'self'; \
         object-src 'none'; \
         form-action 'self'"
    )
}

/// CSP `'sha256-…'` source for every inline `<script>` (no `src=`) in `html`,
/// hashing the exact element text the browser hashes.
fn inline_script_hashes(html: &str) -> Vec<String> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use sha2::{Digest, Sha256};

    let mut out = Vec::new();
    let mut idx = 0;
    while let Some(rel) = html[idx..].find("<script") {
        let tag = idx + rel;
        let Some(gt) = html[tag..].find('>') else { break };
        let open = &html[tag..tag + gt + 1];
        let body_start = tag + gt + 1;
        let Some(close) = html[body_start..].find("</script>") else {
            break;
        };
        let body = &html[body_start..body_start + close];
        if !open.contains("src=") {
            let digest = Sha256::digest(body.as_bytes());
            out.push(format!("'sha256-{}'", STANDARD.encode(digest)));
        }
        idx = body_start + close + "</script>".len();
    }
    out
}

pub async fn run_server() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,represent_backend=debug")),
        )
        .init();

    let cfg = Config::from_env()?;
    if cfg.dev_auth {
        tracing::warn!("DEV_AUTH=1 — auth gate bypassed; do not use in prod");
    }

    // All durable state is the SQLite file; refuse to boot without it.
    let db = db::Db::open(&cfg.db_path)
        .map_err(|e| anyhow::anyhow!("db {} unusable: {e}", cfg.db_path.display()))?;

    // One-shot import of a legacy flat-file data dir (existing projects are
    // skipped, so leaving this configured is harmless).
    if let (Some(dir), Some(email)) = (&cfg.import_dir, &cfg.import_email) {
        match store::import_legacy(&db, dir, email).await {
            Ok(n) if n > 0 => tracing::info!(count = n, dir = %dir.display(), "imported legacy projects"),
            Ok(_) => {}
            Err(e) => tracing::warn!(dir = %dir.display(), error = %e, "legacy import failed; continuing"),
        }
    }

    let state = AppState::new(cfg, db);
    let bind = state.cfg.bind.clone();

    // Hash the SPA's inline bootstrap script(s) so the CSP can allow exactly
    // them. Read once at boot; the built index.html is immutable for the
    // process lifetime.
    let index_path = state.cfg.static_dir.join("index.html");
    let hashes = std::fs::read_to_string(&index_path)
        .map(|h| inline_script_hashes(&h))
        .unwrap_or_default();
    if hashes.is_empty() {
        tracing::warn!(
            path = %index_path.display(),
            "no inline-script hashes (index.html missing or no inline scripts); \
             CSP script-src is 'self' only"
        );
    }
    let csp_value = axum::http::HeaderValue::from_str(&build_csp(&hashes))
        .map_err(|e| anyhow::anyhow!("invalid CSP header: {e}"))?;
    let app = routes::router(state).layer(SetResponseHeaderLayer::if_not_present(
        axum::http::header::CONTENT_SECURITY_POLICY,
        csp_value,
    ));

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(%bind, "represent listening");
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_inline_scripts_skips_external() {
        let html = r#"<script src="/app.js"></script><script>abc</script>"#;
        // Only the inline one; sha256("abc") base64.
        assert_eq!(
            inline_script_hashes(html),
            vec!["'sha256-ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0='"]
        );
    }

    #[test]
    fn csp_includes_hashes_and_no_unsafe_inline_scripts() {
        let csp = build_csp(&["'sha256-X'".into()]);
        assert!(csp.contains("script-src 'self' 'sha256-X'"));
        assert!(!csp.contains("script-src 'self' 'unsafe-inline'"));
    }
}
