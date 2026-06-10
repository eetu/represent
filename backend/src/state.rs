use std::sync::Arc;

use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
}

impl AppState {
    pub fn new(cfg: Config) -> Self {
        Self { cfg: Arc::new(cfg) }
    }
}
