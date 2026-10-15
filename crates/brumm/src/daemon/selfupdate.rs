//! Updates: offered to the players, installed on request (or alone when
//! this build's Apple Music access runs out), and restarted into at a
//! moment nobody hears.

use std::os::unix::process::CommandExt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::Daemon;
use crate::config;
use crate::error::{Result, err};
use crate::ipc::Message;
use crate::{launch, update};

/// The exit status that tells systemd to start the daemon again
/// (RestartForceExitStatus in the unit): the program was replaced.
const EXIT_UPDATED: i32 = 75;

impl Daemon {
    /// Looks for a newer release right after start, daily, and whenever a
    /// player opens (check_now, at most every 10 minutes), and offers it
    /// to the players. It installs on its own only when this build's
    /// Apple Music access is about to run out, so playback never simply
    /// stops. Development builds never update.
    pub(super) async fn check_updates(self: Arc<Self>) {
        if !update::managed(&self.version) {
            return;
        }
        let mut wait = Duration::from_secs(5);
        let mut last: Option<Instant> = None;
        let mut restored = false;
        loop {
            tokio::select! {
                _ = self.quitted() => return,
                _ = tokio::time::sleep(wait) => {}
                _ = self.check_now.notified() => {
                    // Opening brumm asks each time; GitHub allows 60
                    // anonymous requests an hour. An offer found earlier is
                    // still shown.
                    if last.is_some_and(|l| l.elapsed() < Duration::from_secs(600)) {
                        continue;
                    }
                }
            }
            wait = Duration::from_secs(24 * 3600);
            last = Some(Instant::now());
            if !restored {
                // Updated by a brumm from before the window: fetch it now.
                match update::restore_window(&self.version).await {
                    Err(e) => eprintln!("restore window: {e}"),
                    Ok(ok) => {
                        restored = true;
                        if ok {
                            eprintln!("restored the window (brumm-gui) of {}", self.version);
                        }
                    }
                }
            }
            let tag = match update::latest().await {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("update check: {e}");
                    continue;
                }
            };
            if !update::newer(&tag, &self.current()) {
                continue;
            }
            self.offer_update(&tag).await;
            let urgent = self
                .lock()
                .expires
                .is_some_and(|e| e - chrono::Utc::now().timestamp() < 30 * 86400);
            if urgent && let Err(e) = self.install_update().await {
                eprintln!("update: {e}");
            }
        }
    }

    /// The version running, or the one installed and waiting to restart.
    fn current(&self) -> String {
        let i = self.lock();
        if i.installed.is_empty() {
            self.version.clone()
        } else {
            i.installed.clone()
        }
    }

    /// Installs the newest release; the daemon then restarts into it at
    /// the next quiet moment (watch_self).
    pub(super) async fn install_update(&self) -> Result<()> {
        if !update::managed(&self.version) {
            return Err(err(format!(
                "this brumm ({}) was built from source; update it with packaging/update",
                self.version
            )));
        }
        let (tag, installed) = update::update(&self.version).await?;
        if installed {
            eprintln!("installed {tag}; restarting when idle");
            self.lock().installed = tag;
            self.offer_update("").await;
        }
        Ok(())
    }

    /// Publishes an available update (or clears it) to the players at
    /// once — also while the player is not running, when a fix may be
    /// needed most.
    async fn offer_update(&self, tag: &str) {
        let st = {
            let mut i = self.lock();
            i.update = tag.to_string();
            i.state.update = tag.to_string();
            i.state.clone()
        };
        self.broadcast(Message {
            state: Some(st),
            ..Default::default()
        })
        .await;
    }

    /// Restarts the daemon into a replaced program — by an update or by
    /// dev/setup — at a moment nobody hears: while paused, or right as a
    /// song ends, in which case the new daemon carries on playing.
    pub(super) async fn watch_self(self: Arc<Self>) {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = self.quitted() => return,
                _ = tick.tick() => {}
            }
            if !launch::binary_updated() {
                continue;
            }
            // Replaced. Wait for a quiet moment.
            let last_id = self.current_id();
            loop {
                tokio::select! {
                    _ = self.quitted() => return,
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                }
                let (playing, id) = (self.playing(), self.current_id());
                if !playing || id != last_id {
                    self.restart_for_update(playing).await;
                    return;
                }
            }
        }
    }

    fn current_id(&self) -> String {
        self.lock().state.player.id.clone()
    }

    /// Saves where playback is — marked to continue if it was playing —
    /// and becomes the new program.
    pub(super) async fn restart_for_update(&self, playing: bool) {
        let me = match launch::executable() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("restart: {e}");
                return;
            }
        };
        let eng = {
            let mut i = self.lock();
            if let Some(r) = i.resume.as_mut() {
                r.autoplay = playing;
                r.save();
            }
            i.eng.take()
        };
        if let Some(e) = eng {
            e.close().await;
        }
        if let Some(m) = self.mpris.get() {
            m.close().await;
        }
        let _ = std::fs::remove_file(config::socket());
        eprintln!("restarting into the updated program");
        // Exec also works when launch started a detached daemon without
        // systemd.
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        let e = std::process::Command::new(&me).args(args).exec();
        eprintln!("restart: {e}");
        std::process::exit(EXIT_UPDATED);
    }

    /// Answers "is there an update?" right away: the version running, the
    /// newest release, and whether it is newer. A newer one is offered as
    /// the daily check would.
    pub(super) async fn check_now_for(&self, reply: &mut Message) -> Result<()> {
        let current = self.current();
        reply.version.clone_from(&current);
        if !update::managed(&self.version) {
            return Err(err(format!(
                "this brumm ({}) was built from source; update it with packaging/update",
                self.version
            )));
        }
        let (tag, notes) = update::notes_since(&current).await?;
        reply.link.clone_from(&tag);
        if update::newer(&tag, &current) {
            reply.pos = 1;
            reply.notes = notes;
            self.offer_update(&tag).await;
        }
        Ok(())
    }
}
