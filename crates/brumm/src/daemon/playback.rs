//! Confirming that playback really started: a command succeeding only
//! means JavaScript accepted it.

use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{Daemon, same};
use crate::engine::Engine;
use crate::error::Result;
use crate::ipc::{self, Player};

const PLAY_CONFIRM_WITHIN: Duration = Duration::from_secs(15);

impl Daemon {
    /// Invalidates running checks; the new one's number.
    pub(super) fn cancel_play_check(&self) -> u64 {
        let mut i = self.lock();
        i.play_check += 1;
        i.play_check
    }

    fn checking_playback(&self, eng: &Arc<Engine>, seq: u64) -> bool {
        let i = self.lock();
        same(&i.eng, eng) && i.play_check == seq
    }

    /// Checks the actual player before toggling: the daemon's last report
    /// can be stale. Only a request to resume needs a confirmation.
    pub(super) async fn toggle_playback(self: &Arc<Self>, eng: &Arc<Engine>) -> Result<()> {
        let seq = self.cancel_play_check();
        let es = eng.state().await?;
        self.apply(es.clone()).await;
        if self.resume_if_idle(eng).await {
            tokio::spawn(self.clone().verify_playback(eng.clone(), seq));
            return Ok(());
        }
        eng.toggle().await?;
        if !es.playing && es.preview.is_none() {
            tokio::spawn(self.clone().verify_playback(eng.clone(), seq));
        }
        Ok(())
    }

    /// Confirms that playback started, even if the event stream stopped
    /// reporting changes. One failed attempt gets one player restart; a
    /// deliberate pause cancels it.
    pub(super) async fn verify_playback(self: Arc<Self>, eng: Arc<Engine>, seq: u64) {
        let (d, e, old) = (self.clone(), eng.clone(), eng.clone());
        self.confirm_playback(
            &eng,
            seq,
            PLAY_CONFIRM_WITHIN,
            Duration::from_secs(1),
            || e.state_owned(),
            move || {
                eprintln!(
                    "playback did not start within {}s; restarting player and restoring the queue",
                    PLAY_CONFIRM_WITHIN.as_secs()
                );
                tokio::spawn(async move { old.close().await });
                let d = d.clone();
                tokio::spawn(async move {
                    d.set_status(
                        ipc::STATUS_STARTING,
                        "playback did not start; restarting player",
                    )
                    .await;
                    d.spawn_boot();
                });
            },
        )
        .await;
    }

    pub(super) async fn confirm_playback<F, Fut>(
        self: &Arc<Self>,
        eng: &Arc<Engine>,
        seq: u64,
        timeout: Duration,
        interval: Duration,
        state: F,
        recover: impl FnOnce(),
    ) where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<Player>>,
    {
        let deadline = Instant::now() + timeout;
        let mut tick = tokio::time::interval_at(tokio::time::Instant::now() + interval, interval);
        loop {
            tokio::select! {
                _ = self.quitted() => return,
                _ = tick.tick() => {}
            }
            if !self.checking_playback(eng, seq) {
                return;
            }
            let es = state().await;
            if !self.checking_playback(eng, seq) {
                return;
            }
            if let Ok(es) = es {
                let done = es.playing || es.needs_auth || !es.err.is_empty();
                self.apply(es).await;
                if done {
                    return;
                }
            }
            if Instant::now() < deadline {
                continue;
            }
            if self.claim_playback_recovery(eng, seq) {
                recover();
            }
            return;
        }
    }

    fn claim_playback_recovery(&self, eng: &Arc<Engine>, seq: u64) -> bool {
        let mut i = self.lock();
        let playable = i.resume.as_ref().is_some_and(|r| r.playable());
        if !same(&i.eng, eng) || i.play_check != seq || i.state.player.playing || !playable {
            return false;
        }
        if let Some(r) = i.resume.as_mut() {
            r.autoplay = true;
        }
        i.eng = None; // the running watch must not restart the same player again
        true
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::super::library::Library;
    use super::super::resume::Resume;
    use super::*;

    fn daemon() -> (Arc<Daemon>, Arc<Engine>) {
        let eng = Engine::stand_in();
        let resume = Resume {
            id: "song".into(),
            ids: vec!["song".into()],
            pos: 59.0,
            ..Default::default()
        };
        let d = Arc::new(Daemon::new("dev", Library::default(), Some(resume)));
        d.lock().eng = Some(eng.clone());
        (d, eng)
    }

    #[tokio::test]
    async fn a_failed_resume_restarts_once_and_keeps_the_queue() {
        let (d, eng) = daemon();
        let seq = d.cancel_play_check();
        let restarts = AtomicUsize::new(0);
        let ready = || async {
            Ok(Player {
                ready: true,
                ..Default::default()
            })
        };
        let ms = Duration::from_millis(1);
        d.confirm_playback(&eng, seq, 5 * ms, ms, ready, || {
            restarts.fetch_add(1, Ordering::Relaxed);
        })
        .await;
        {
            let i = d.lock();
            let r = i.resume.as_ref().unwrap();
            assert_eq!(restarts.load(Ordering::Relaxed), 1);
            assert!(i.eng.is_none() && r.autoplay && r.id == "song" && r.pos == 59.0);
        }
        d.confirm_playback(&eng, seq, 5 * ms, ms, ready, || {
            restarts.fetch_add(1, Ordering::Relaxed);
        })
        .await;
        assert_eq!(
            restarts.load(Ordering::Relaxed),
            1,
            "recovery restarted the same player twice"
        );
    }

    #[tokio::test]
    async fn the_check_repairs_a_stale_report() {
        let (d, eng) = daemon();
        d.lock().resume = None;
        let seq = d.cancel_play_check();
        let playing = || async {
            Ok(Player {
                ready: true,
                playing: true,
                title: "Song".into(),
                ..Default::default()
            })
        };
        d.confirm_playback(
            &eng,
            seq,
            Duration::from_secs(1),
            Duration::from_millis(1),
            playing,
            || panic!("healthy playback restarted"),
        )
        .await;
        let i = d.lock();
        assert!(
            i.state.player.playing && i.state.player.title == "Song",
            "stale state not repaired"
        );
    }

    #[tokio::test]
    async fn a_new_transport_action_cancels_the_check() {
        for replace in [false, true] {
            let (d, eng) = daemon();
            let seq = d.cancel_play_check();
            let dd = d.clone();
            let state = move || {
                let dd = dd.clone();
                async move {
                    if replace {
                        dd.lock().eng = Some(Engine::stand_in());
                    } else {
                        dd.cancel_play_check();
                    }
                    Ok(Player {
                        ready: true,
                        ..Default::default()
                    })
                }
            };
            d.confirm_playback(
                &eng,
                seq,
                Duration::from_secs(1),
                Duration::from_millis(1),
                state,
                || panic!("a cancelled check restarted the player"),
            )
            .await;
            assert!(
                !d.lock().resume.as_ref().unwrap().autoplay,
                "a cancelled play request was restored"
            );
        }
    }

    #[tokio::test]
    async fn player_errors_show_instead_of_a_restart() {
        for auth in [false, true] {
            let (d, eng) = daemon();
            let seq = d.cancel_play_check();
            let failing = move || async move {
                Ok(Player {
                    ready: true,
                    err: "play: unavailable".into(),
                    needs_auth: auth,
                    ..Default::default()
                })
            };
            d.confirm_playback(
                &eng,
                seq,
                Duration::from_secs(1),
                Duration::from_millis(1),
                failing,
                || panic!("an explicit error triggered a restart"),
            )
            .await;
            let want = if auth {
                ipc::STATUS_LOGGED_OUT
            } else {
                ipc::STATUS_ERROR
            };
            assert_eq!(d.lock().state.status, want);
        }
    }
}
