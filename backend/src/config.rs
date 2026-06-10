use std::env;
use std::path::PathBuf;

/// All durable state is the SQLite file at `db_path` (profiles, projects,
/// documents) — one restic-backed file in prod. Auth is either an own OIDC
/// session (kanidm), trusted oauth2-proxy forward-auth headers, or DEV_AUTH;
/// see [`crate::auth`].
#[derive(Debug, Clone)]
pub struct Config {
    pub bind: String,
    /// When set, requests without any other credential get a synthetic dev
    /// identity, and `/auth/login?username=…` mints arbitrary dev sessions
    /// (multi-user testing). Never enable in prod.
    pub dev_auth: bool,

    pub db_path: PathBuf,

    /// Signed-cookie key (hex, ≥64 bytes). The session cookie is the entire
    /// auth credential, so prod refuses to boot without a strong key.
    pub session_key_hex: String,

    /// Own OIDC login (kanidm). Unset → rely on forward-auth / DEV_AUTH.
    pub oidc: Option<OidcSettings>,

    /// One-shot import of a legacy flat-file data dir
    /// (`<dir>/<project>/*.md` plus optional `.order`) into `import_email`'s
    /// profile at boot. Both must be set; existing projects are skipped.
    pub import_dir: Option<PathBuf>,
    pub import_email: Option<String>,

    /// Directory of the built SPA to serve (Vite `dist/`).
    pub static_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct OidcSettings {
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_url: String,
}

impl OidcSettings {
    fn from_env() -> Option<Self> {
        let issuer = env::var("OIDC_ISSUER").ok().filter(|s| !s.is_empty())?;
        let client_id = env::var("OIDC_CLIENT_ID").ok().filter(|s| !s.is_empty())?;
        let client_secret = env::var("OIDC_CLIENT_SECRET").ok().filter(|s| !s.is_empty())?;
        let redirect_url = env::var("OIDC_REDIRECT_URL").ok().filter(|s| !s.is_empty())?;
        Some(Self {
            issuer,
            client_id,
            client_secret,
            redirect_url,
        })
    }
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let dev_auth = env::var("DEV_AUTH").as_deref() == Ok("1");
        let session_key_hex = resolve_session_key(dev_auth)?;
        Ok(Self {
            dev_auth,
            bind: env::var("REPRESENT_BIND").unwrap_or_else(|_| "0.0.0.0:3008".into()),
            db_path: PathBuf::from(
                env::var("REPRESENT_DB_PATH").unwrap_or_else(|_| "represent.db".into()),
            ),
            session_key_hex,
            oidc: OidcSettings::from_env(),
            import_dir: env::var("REPRESENT_IMPORT_DIR")
                .ok()
                .filter(|s| !s.is_empty())
                .map(PathBuf::from),
            import_email: env::var("REPRESENT_IMPORT_EMAIL")
                .ok()
                .filter(|s| !s.is_empty()),
            static_dir: PathBuf::from(env::var("STATIC_DIR").unwrap_or_else(|_| "./dist".into())),
        })
    }
}

/// Resolve the signed-cookie key (house pattern — see scribe). The cookie is
/// the *entire* auth credential (no server-side sessions), so:
///   - `SESSION_KEY` set → require ≥64 bytes of valid hex, else hard error.
///   - unset + `DEV_AUTH` → random per-boot key (sessions drop on restart).
///   - unset + prod → fail closed: a predictable key would let anyone forge
///     a session cookie and authenticate as any user.
fn resolve_session_key(dev_auth: bool) -> anyhow::Result<String> {
    match env::var("SESSION_KEY") {
        Ok(k) if !k.trim().is_empty() => {
            let k = k.trim().to_string();
            let decoded = hex::decode(&k)
                .map_err(|_| anyhow::anyhow!("SESSION_KEY must be hex (128 chars = 64 bytes)"))?;
            if decoded.len() < 64 {
                anyhow::bail!(
                    "SESSION_KEY too short: {} bytes decoded, need ≥64 (128 hex chars). \
                     Generate one with `openssl rand -hex 64`",
                    decoded.len()
                );
            }
            Ok(k)
        }
        _ => {
            if dev_auth {
                tracing::warn!(
                    "SESSION_KEY unset; using a random ephemeral key (DEV_AUTH only). \
                     Sessions drop on restart."
                );
                Ok(random_session_key())
            } else {
                anyhow::bail!(
                    "SESSION_KEY is required when DEV_AUTH is off. \
                     Generate one with `openssl rand -hex 64`"
                )
            }
        }
    }
}

fn random_session_key() -> String {
    use rand::Rng;
    let mut bytes = [0u8; 64];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
