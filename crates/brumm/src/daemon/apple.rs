//! The composition point for the currently selected music API.
use crate::config::Config;
use crate::error::{Error, Result};
use crate::provider::{Api, apple};
use std::sync::Arc;
pub(super) fn new_api(cfg: &Config) -> Result<Arc<dyn Api>> {
    match cfg.effective_provider() {
        "apple" => Ok(Arc::new(apple::Client::new(
            cfg.developer(),
            &cfg.user_token,
        ))),
        _ => Err(Error::Unsupported),
    }
}
