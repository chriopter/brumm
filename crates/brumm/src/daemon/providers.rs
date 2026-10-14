//! Constructs the selected provider through its shared definition.
use crate::{
    config::Config,
    error::Result,
    provider::{Api, registry},
};
use std::sync::Arc;
pub(super) fn new_api(cfg: &Config) -> Result<Arc<dyn Api>> {
    registry::definition(cfg.effective_provider())?.api(cfg)
}
