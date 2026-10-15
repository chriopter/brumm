//! Selects the provider's authentication flow. Provider modules own credentials.
use crate::{
    config::Config,
    error::{Result, err},
    provider::{
        registry,
        session::{AuthEvent, AuthReporter},
    },
};
use std::sync::Arc;
pub async fn run(status: impl Fn(&str) + Send + Sync + 'static) -> Result<()> {
    run_with_report(Arc::new(move |event| {
        if let AuthEvent::Progress(message) = event {
            status(&message);
        }
    }))
    .await
}
pub async fn run_with_report(report: AuthReporter) -> Result<()> {
    let cfg = Config::load()?;
    if cfg.needs_provider_choice() {
        return Err(err(
            "open Brumm and choose your music service before signing in",
        ));
    }
    registry::definition(cfg.effective_provider())?
        .authenticate(report)
        .await
}
