use std::sync::Arc;

use axum_extra::extract::cookie::Key;

use crate::config::Config;
use crate::db::Db;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub db: Db,
    pub cookie_key: Key,
    /// Lazily-discovered OIDC provider with on-demand retry — the issuer may
    /// boot after us; discovery self-heals on the next login or /status call.
    pub oidc: Arc<crate::oidc::OidcLazy>,
}

impl AppState {
    pub fn new(cfg: Config, db: Db) -> Self {
        let cookie_key = crate::auth::cookie_key(&cfg.session_key_hex);
        let oidc = Arc::new(crate::oidc::OidcLazy::new(cfg.oidc.clone()));
        Self {
            cfg: Arc::new(cfg),
            db,
            cookie_key,
            oidc,
        }
    }
}

impl axum::extract::FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Self {
        state.cookie_key.clone()
    }
}
