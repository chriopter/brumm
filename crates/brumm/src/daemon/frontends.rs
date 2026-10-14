//! What the players ask of the daemon beyond playing: the options they
//! share, signing in, and each other. A player needs nothing but the
//! socket; the files and programs behind these stay the daemon's.

use std::sync::{Arc, Mutex};

use serde_json::{Map, Value};

use super::Daemon;
use crate::config::Options;
use crate::error::Result;
use crate::ipc::Message;
use crate::provider::session::AuthEvent;
use crate::{login, update};

/// One change of the options at a time.
static OPTIONS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Where a player last was; it lives only as long as the daemon.
static PLACE: Mutex<Option<Value>> = Mutex::new(None);

/// One provider selection at a time.
static PROVIDER: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// One sign-in page at a time.
static LOGIN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Keeps where a player is, and answers where the last one was: a player
/// switched to (g) opens where the other left off. The daemon never looks
/// inside.
pub(super) fn place(set: Option<&Value>, reply: &mut Message) {
    let mut p = PLACE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(v) = set
        && serde_json::to_vec(v).map(|b| b.len()).unwrap_or(usize::MAX) < 64 << 10
    {
        *p = Some(v.clone());
    }
    reply.place.clone_from(&p);
}

impl Daemon {
    /// Persist only a provider whose adapter is actually available.
    pub(super) async fn select_provider(
        self: &Arc<Self>,
        id: &str,
        reply: &mut Message,
    ) -> Result<()> {
        let _one = PROVIDER.lock().await;
        let mut cfg = crate::config::Config::load()?;
        if !id.is_empty() {
            crate::provider::registry::validate(id)?;
            cfg.provider = id.into();
            cfg.save()?;
            self.spawn_boot();
        }
        reply.provider = Some(crate::provider::registry::selection(&cfg));
        Ok(())
    }

    /// The saved options, after changing those named in set. A change is
    /// announced to every subscribed player.
    pub(super) async fn options(
        &self,
        set: Option<&Map<String, Value>>,
        reply: &mut Message,
    ) -> Result<()> {
        let _one = OPTIONS.lock().await;
        let mut o = Options::load();
        if let Some(set) = set.filter(|s| !s.is_empty()) {
            let was = o.clone();
            o = o.with(set)?;
            if set.contains_key("bar") && o.bar != was.bar {
                update::set_plugin(o.bar == "on")?;
            }
            if set.contains_key("music_keys") && o.music_keys != was.music_keys {
                update::set_music_keys(o.music_keys)?;
            }
            o.save()?;
            o = Options::load();
            if o.no_autoplay != was.no_autoplay
                && let Some(eng) = self.engine()
            {
                let _ = eng.set_autoplay(!o.no_autoplay).await;
            }
            self.broadcast(Message {
                options: Some(o.clone()),
                ..Default::default()
            })
            .await;
        }
        reply.options = Some(o);
        reply.omarchy = update::has_omarchy() && update::can_music_keys();
        reply.agent = update::has_agent();
        Ok(())
    }

    /// Opens the sign-in page, once at a time, and reloads the player with
    /// the new session.
    pub(super) fn login(self: &Arc<Self>) {
        let Ok(guard) = LOGIN.try_lock() else { return }; // the page is open already
        let d = self.clone();
        let report = {
            let d = d.clone();
            Arc::new(move |event| {
                let state = {
                    let mut i = d.lock();
                    match event {
                        AuthEvent::Progress(message) => i.state.message = message,
                        AuthEvent::Prompt(prompt) => i.state.auth = Some(prompt),
                    }
                    i.state.clone()
                };
                let d = d.clone();
                tokio::spawn(async move {
                    d.broadcast(Message {
                        state: Some(state),
                        ..Default::default()
                    })
                    .await;
                });
            })
        };
        let task = tokio::spawn(async move {
            let _guard = guard;
            let result = login::run_with_report(report).await;
            {
                let mut i = d.lock();
                i.state.auth = None;
                i.login_task = None;
            }
            match result {
                Ok(()) => d.reload().await,
                Err(e) => {
                    eprintln!("login: {e}");
                    // The players say what went wrong: the report carries it.
                    let st = {
                        let mut i = d.lock();
                        i.state.player.err = format!("sign-in: {e}");
                        i.state.clone()
                    };
                    d.broadcast(Message {
                        state: Some(st),
                        ..Default::default()
                    })
                    .await;
                }
            }
        });
        self.lock().login_task = Some(task.abort_handle());
    }

    pub(super) async fn cancel_login(&self) {
        let st = {
            let mut i = self.lock();
            if let Some(task) = i.login_task.take() {
                task.abort();
            }
            i.state.auth = None;
            i.state.message = "sign-in cancelled".into();
            i.state.clone()
        };
        self.broadcast(Message {
            state: Some(st),
            ..Default::default()
        })
        .await;
    }

    /// Asks the active provider what a share link names.
    pub(super) async fn resolve(&self, link: &str, reply: &mut Message) -> Result<()> {
        let api = self.client()?;
        let (it, song) = self.auth_check(api.resolve(link).await).await?;
        reply.items = vec![it];
        if !song.is_empty() {
            reply.ids = vec![song];
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc;

    /// A player leaves its place; the next one asks and finds it; a null
    /// clears it.
    #[test]
    fn place_round_trip() {
        let mut reply = Message::default();
        place(Some(&serde_json::json!({"section": 2})), &mut reply);
        let mut reply = Message::default();
        place(None, &mut reply);
        assert_eq!(reply.place, Some(serde_json::json!({"section": 2})));
        let huge = Value::String("x".repeat(70_000));
        place(Some(&huge), &mut Message::default());
        let mut reply = Message::default();
        place(None, &mut reply);
        assert_eq!(
            reply.place,
            Some(serde_json::json!({"section": 2})),
            "an oversized place was kept"
        );
        place(Some(&Value::Null), &mut Message::default());
        let mut reply = Message::default();
        place(None, &mut reply);
        assert_eq!(
            ipc::encode(&reply),
            b"{\"place\":null}\n".to_vec(),
            "cleared, as the Go daemon answered"
        );
    }
}
