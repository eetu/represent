use std::env;
use std::path::PathBuf;

/// Flat-file config. No DB, no session key — the app sits behind oauth2-proxy
/// (it trusts `X-Auth-Request-*`, see [`crate::auth`]) so it holds no auth
/// secret of its own. All durable state is markdown files under `data_dir`
/// (one subdirectory per project), which is the restic-backed bind mount in
/// prod.
#[derive(Debug, Clone)]
pub struct Config {
    pub bind: String,
    /// When set, the forward-auth gate is bypassed with a synthetic user so the
    /// app is usable on localhost without oauth2-proxy in front. Never enable in
    /// prod — it removes the only request-origin check the binary makes.
    pub dev_auth: bool,

    /// Root of the project store: `data_dir/<project>/<file>.md`.
    pub data_dir: PathBuf,

    /// Directory of the built SPA to serve (Vite `dist/`).
    pub static_dir: PathBuf,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            dev_auth: env::var("DEV_AUTH").as_deref() == Ok("1"),
            bind: env::var("REPRESENT_BIND").unwrap_or_else(|_| "0.0.0.0:3008".into()),
            data_dir: PathBuf::from(
                env::var("REPRESENT_DATA_DIR").unwrap_or_else(|_| "./data".into()),
            ),
            static_dir: PathBuf::from(env::var("STATIC_DIR").unwrap_or_else(|_| "./dist".into())),
        })
    }
}
