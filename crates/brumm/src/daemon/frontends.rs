//! What the players ask of the daemon beyond playing: the options they
//! share, signing in, and each other. A player needs nothing but the
//! socket; the files and programs behind these stay the daemon's.

use std::sync::{Arc, Mutex};

use serde_json::{Map, Value};

use super::Daemon;
use crate::apple;
use crate::config::Options;
use crate::error::{Result, err};
use crate::ipc::Message;
use crate::{login, update};

/// One change of the options at a time.
static OPTIONS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Where a player last was; it lives only as long as the daemon.
static PLACE: Mutex<Option<Value>> = Mutex::new(None);

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
        tokio::spawn(async move {
            let _guard = guard;
            match login::run(|s| eprintln!("login: {s}")).await {
                Ok(()) => d.reload().await,
                Err(e) => {
                    eprintln!("login: {e}");
                    // The players say what went wrong: the report carries it.
                    let mut st = d.lock().state.clone();
                    st.player.err = format!("sign-in: {e}");
                    d.broadcast(Message {
                        state: Some(st),
                        ..Default::default()
                    })
                    .await;
                }
            }
        });
    }

    /// What a music.apple.com link names; a song's link names its album,
    /// with the song picked out.
    pub(super) async fn resolve(&self, link: &str, reply: &mut Message) -> Result<()> {
        let (mut it, song) =
            apple::parse_link(link).ok_or_else(|| err("not a music.apple.com link"))?;
        if it.id.is_empty() {
            let api = self.client()?;
            it = self.auth_check(api.album_of(&song).await).await?;
        }
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
