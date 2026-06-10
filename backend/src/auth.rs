//! Session auth + the login flows (ported house pattern — see scribe/chat).
//!
//! Credential precedence in the [`AuthUser`] extractor:
//!   1. Signed session cookie (`represent_session`, payload `sub|email`) —
//!      minted by the OIDC callback or the DEV_AUTH login.
//!   2. oauth2-proxy forward-auth headers (`X-Auth-Request-User`/`-Email`) —
//!      the gated-host deploy mode, no own login needed.
//!   3. `DEV_AUTH=1` → synthetic `dev` identity.
//!
//! Every identity resolves to a `profile` row (sub match → email match with
//! sub backfill → create), and all project/document queries key off that
//! profile id — this is what makes the app multi-user.
//!
//! OIDC handshake values (csrf state, nonce, PKCE verifier, post-login next
//! URL) round-trip in a separate short-lived signed cookie `represent_oidc`,
//! removed the moment the callback consumes it. The browser never sees
//! provider tokens; the session payload is only `sub|email`.

use axum::extract::{Query, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::extract::FromRequestParts;
use axum::Json;
use axum_extra::extract::cookie::{Cookie, Key, SameSite, SignedCookieJar};
use openidconnect::{Nonce, PkceCodeVerifier};
use serde::Deserialize;
use serde_json::json;

use crate::error::AppError;
use crate::state::AppState;
use crate::store;

const COOKIE_NAME: &str = "represent_session";
const OIDC_COOKIE: &str = "represent_oidc";

const HDR_USER: &str = "x-auth-request-user";
const HDR_EMAIL: &str = "x-auth-request-email";

/// Build the signing key from the SESSION_KEY hex. `config::resolve_session_key`
/// has already validated this is ≥64 bytes of real hex (or a random key in
/// dev), so decoding here is infallible.
pub fn cookie_key(hex: &str) -> Key {
    let bytes = hex::decode(hex).expect("SESSION_KEY validated at config load");
    Key::from(&bytes[..64])
}

/// The authenticated principal. `profile_id` keys every store query; the
/// email is surfaced via `/api/me` and never logged (PII).
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub profile_id: i64,
    pub email: String,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // 1. Session cookie.
        let jar = SignedCookieJar::from_headers(&parts.headers, state.cookie_key.clone());
        if let Some((sub, email)) = jar
            .get(COOKIE_NAME)
            .and_then(|c| parse_cookie(c.value()).map(|(s, e)| (s.to_string(), e.to_string())))
        {
            let profile_id = store::resolve_profile(&state.db, &sub, &email).await?;
            return Ok(AuthUser { profile_id, email });
        }

        // 2. Forward-auth headers (oauth2-proxy in front).
        let user = header(parts, HDR_USER);
        let email = header(parts, HDR_EMAIL);
        if let (Some(user), Some(email)) = (user, email) {
            if !user.is_empty() && !email.is_empty() {
                let profile_id = store::resolve_profile(&state.db, &user, &email).await?;
                return Ok(AuthUser { profile_id, email });
            }
        }

        // 3. DEV_AUTH synthetic identity.
        if state.cfg.dev_auth {
            let email = "dev@localhost".to_string();
            let profile_id = store::resolve_profile(&state.db, "dev", &email).await?;
            return Ok(AuthUser { profile_id, email });
        }

        Err(AppError::Unauthorized)
    }
}

fn header(parts: &Parts, name: &str) -> Option<String> {
    parts
        .headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

fn parse_cookie(raw: &str) -> Option<(&str, &str)> {
    let (sub, email) = raw.split_once('|')?;
    if sub.is_empty() || email.is_empty() {
        return None;
    }
    Some((sub, email))
}

fn write_cookie(jar: SignedCookieJar, sub: &str, email: &str, secure: bool) -> SignedCookieJar {
    let value = format!("{sub}|{email}");
    let cookie = Cookie::build((COOKIE_NAME, value))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        // Secure in prod (HTTPS behind Traefik); off only under DEV_AUTH
        // where the dev server is plain-HTTP localhost.
        .secure(secure)
        .build();
    jar.add(cookie)
}

fn sanitize_next(next: Option<&str>) -> String {
    match next {
        Some(n) if n.starts_with('/') && !n.starts_with("//") => n.to_string(),
        _ => "/".to_string(),
    }
}

#[derive(Debug, Deserialize)]
pub struct LoginQuery {
    /// DEV_AUTH only: mint a session for an arbitrary user (multi-user dev).
    username: Option<String>,
    email: Option<String>,
    next: Option<String>,
}

/// `GET /auth/login`
pub async fn login(
    State(state): State<AppState>,
    jar: SignedCookieJar,
    Query(q): Query<LoginQuery>,
) -> Result<Response, AppError> {
    let dest = sanitize_next(q.next.as_deref());

    // 1. OIDC if configured. Discovery is lazy + retried (OidcLazy), so an
    // issuer that was down at boot recovers here without a restart.
    if state.oidc.is_configured() {
        match state.oidc.ctx().await {
            Some(oidc) => {
                let auth = oidc.authorize();
                // Handshake values round-trip in a short-lived signed cookie:
                // csrf|nonce|pkce|next.
                let payload = format!(
                    "{}|{}|{}|{}",
                    auth.csrf.secret(),
                    auth.nonce.secret(),
                    auth.pkce_verifier.secret(),
                    dest
                );
                let cookie = Cookie::build((OIDC_COOKIE, payload))
                    .path("/")
                    .http_only(true)
                    .same_site(SameSite::Lax)
                    .secure(!state.cfg.dev_auth)
                    .max_age(time::Duration::minutes(10))
                    .build();
                return Ok((jar.add(cookie), Redirect::to(auth.url.as_str())).into_response());
            }
            // Configured but unreachable: retryable 503 in prod rather than a
            // silent downgrade; in dev fall through to DEV_AUTH below.
            None if !state.cfg.dev_auth => {
                return Err(AppError::ServiceUnavailable(
                    "auth provider not reachable; retry shortly".into(),
                ));
            }
            None => {}
        }
    }

    // 2. DEV_AUTH: mint a session for ?username=… (defaults to dev).
    if state.cfg.dev_auth {
        let user = q.username.unwrap_or_else(|| "dev".to_string());
        let email = q.email.unwrap_or_else(|| format!("{user}@local"));
        store::resolve_profile(&state.db, &user, &email).await?;
        return Ok((write_cookie(jar, &user, &email, false), Redirect::to(&dest)).into_response());
    }

    // 3. Nothing configured (forward-auth deploys never hit /auth/login).
    Err(AppError::BadRequest(
        "auth not configured. set DEV_AUTH=1 or all four OIDC_* env vars".into(),
    ))
}

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

/// `GET /auth/callback`
pub async fn callback(
    State(state): State<AppState>,
    jar: SignedCookieJar,
    Query(q): Query<CallbackQuery>,
) -> Result<Response, AppError> {
    let oidc = state.oidc.ctx().await.ok_or_else(|| {
        AppError::ServiceUnavailable("auth provider not reachable; retry shortly".into())
    })?;

    if let Some(err) = &q.error {
        tracing::warn!(
            "oidc provider returned error: {err} ({:?})",
            q.error_description
        );
        return Err(AppError::BadRequest(format!("provider error: {err}")));
    }

    let code = q
        .code
        .clone()
        .ok_or_else(|| AppError::BadRequest("missing code".into()))?;
    let returned_state = q
        .state
        .clone()
        .ok_or_else(|| AppError::BadRequest("missing state".into()))?;

    let handshake = jar
        .get(OIDC_COOKIE)
        .map(|c| c.value().to_string())
        .ok_or_else(|| AppError::BadRequest("session missing oidc handshake values".into()))?;

    // Drop the handshake cookie regardless of outcome — replay is never
    // useful and partial state on retry is worse than a restart.
    let cleared = jar.remove(Cookie::build((OIDC_COOKIE, "")).path("/").build());

    let parts: Vec<&str> = handshake.splitn(4, '|').collect();
    if parts.len() != 4 {
        return Err(AppError::BadRequest("malformed handshake cookie".into()));
    }
    let (csrf, nonce, pkce, dest) = (parts[0], parts[1], parts[2], parts[3]);

    if csrf != returned_state {
        tracing::warn!("oidc state mismatch — possible csrf");
        return Err(AppError::BadRequest("state mismatch".into()));
    }

    let claims = oidc
        .exchange(
            &code,
            PkceCodeVerifier::new(pkce.to_string()),
            Nonce::new(nonce.to_string()),
        )
        .await
        .map_err(|e| AppError::Upstream(e.to_string()))?;

    store::resolve_profile(&state.db, &claims.sub, &claims.email).await?;

    let dest = sanitize_next(Some(dest));
    Ok((
        write_cookie(cleared, &claims.sub, &claims.email, !state.cfg.dev_auth),
        Redirect::to(&dest),
    )
        .into_response())
}

/// `POST /auth/logout`
pub async fn logout(jar: SignedCookieJar) -> Response {
    let cookie = Cookie::build((COOKIE_NAME, ""))
        .path("/")
        .http_only(true)
        .build();
    (jar.remove(cookie), StatusCode::NO_CONTENT).into_response()
}

/// `GET /api/me` — who am I (drives the header chip + logout affordance).
pub async fn me(user: AuthUser) -> Json<serde_json::Value> {
    Json(json!({ "email": user.email }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    fn state(dev_auth: bool) -> AppState {
        // SESSION_KEY env may leak between parallel tests — build the config
        // by hand instead of from_env.
        let cfg = crate::config::Config {
            bind: String::new(),
            dev_auth,
            db_path: std::path::PathBuf::new(),
            session_key_hex: "ab".repeat(64),
            oidc: None,
            import_dir: None,
            import_email: None,
            static_dir: std::path::PathBuf::new(),
        };
        let db = crate::db::Db::open_in_memory().unwrap();
        AppState::new(cfg, db)
    }

    async fn extract(req: Request<()>, dev_auth: bool) -> Result<AuthUser, AppError> {
        let (mut parts, _) = req.into_parts();
        AuthUser::from_request_parts(&mut parts, &state(dev_auth)).await
    }

    #[tokio::test]
    async fn rejects_when_no_credentials_in_prod() {
        let req = Request::builder().body(()).unwrap();
        assert!(matches!(
            extract(req, false).await,
            Err(AppError::Unauthorized)
        ));
    }

    #[tokio::test]
    async fn accepts_forward_auth_headers() {
        let req = Request::builder()
            .header(HDR_USER, "alice")
            .header(HDR_EMAIL, "alice@example.com")
            .body(())
            .unwrap();
        let u = extract(req, false).await.unwrap();
        assert_eq!(u.email, "alice@example.com");
        assert!(u.profile_id > 0);
    }

    #[tokio::test]
    async fn dev_auth_bypasses() {
        let req = Request::builder().body(()).unwrap();
        let u = extract(req, true).await.unwrap();
        assert_eq!(u.email, "dev@localhost");
    }

    #[test]
    fn sanitize_next_blocks_open_redirect() {
        assert_eq!(sanitize_next(Some("/p/demo")), "/p/demo");
        assert_eq!(sanitize_next(Some("//evil.com")), "/");
        assert_eq!(sanitize_next(Some("https://evil.com")), "/");
        assert_eq!(sanitize_next(None), "/");
    }

    #[test]
    fn parse_cookie_rejects_malformed() {
        assert_eq!(parse_cookie("abc|a@b"), Some(("abc", "a@b")));
        assert_eq!(parse_cookie("noseparator"), None);
        assert_eq!(parse_cookie("|a@b"), None);
        assert_eq!(parse_cookie("abc|"), None);
    }
}
