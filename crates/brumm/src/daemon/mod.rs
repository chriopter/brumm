//! brumm's background player: one headless Chrome doing the playback, an
//! MPRIS server for media keys and the bar, and a unix socket the players
//! talk to. Music keeps playing when the players are closed.

mod feedback;
mod frontends;
mod library;
mod playback;
mod providers;
mod resume;
mod selfupdate;
mod session;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::unix::OwnedWriteHalf;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Notify, Semaphore};

use self::providers::new_api;
use crate::config::{self, Config, Options};
use crate::engine::{self, CdpError, Engine};
use crate::error::{Error, Result, err};
use crate::ipc::{self, Message, Player, Request, State};
use crate::music::{
    ExplorePage, Item, KIND_ALBUM, KIND_ARTIST, KIND_PLAYLIST, KIND_SHELF, KIND_STATION, KIND_TERM,
    Shelf,
};
use crate::provider::Api;
use crate::{chrome, launch, mpris};

use library::Library;
use resume::Resume;

/// Spectrum frames per second, for players that do not say.
const DEFAULT_FPS: i64 = 25;
// The page answers as soon as anything changes; these are only how long it
// may stay silent before it is asked again — the playhead is extrapolated
// in between, so playing needs a check now and then and paused hardly at
// all.
const HEART_PLAY: Duration = Duration::from_secs(3);
const HEART_PAUSE: Duration = Duration::from_secs(30);
/// Failing this long while the page is up: restart Chrome.
const STALE_AFTER: Duration = Duration::from_secs(10);
/// After a failed player start, doubling.
const RETRY_FIRST: Duration = Duration::from_secs(2);
const RETRY_MAX: Duration = Duration::from_secs(60);
/// Paused this long: close Chrome until asked to play.
const SLEEP_AFTER: Duration = Duration::from_secs(10 * 60);
/// The loop's wait when there is nothing to do.
const IDLE_WAKE: Duration = Duration::from_secs(3600);
/// A player that takes longer to read is dropped.
const SEND_WITHIN: Duration = Duration::from_millis(200);

/// One player's end of its connection. Writes carry a deadline so a
/// stalled player (say, a suspended terminal) is dropped instead of
/// blocking every other one.
struct ConnOut {
    writer: tokio::sync::Mutex<OwnedWriteHalf>,
    dead: Notify,
    is_dead: AtomicBool,
}

impl ConnOut {
    async fn send(&self, line: &[u8]) -> bool {
        if self.is_dead.load(Ordering::Relaxed) {
            return false;
        }
        let ok = tokio::time::timeout(SEND_WITHIN, async {
            self.writer.lock().await.write_all(line).await
        })
        .await
        .is_ok_and(|r| r.is_ok());
        if !ok {
            self.kill();
        }
        ok
    }

    fn kill(&self) {
        self.is_dead.store(true, Ordering::Relaxed);
        self.dead.notify_one();
    }
}

/// What a connection asked to be pushed.
struct ConnInfo {
    out: Arc<ConnOut>,
    sub: bool,
    /// Spectrum bands wanted; 0 = none.
    bands: i64,
    /// Waveform samples wanted; 0 = none.
    wave: i64,
    /// Spectrum frames per second wanted.
    fps: i64,
}

struct CachedPage {
    shelves: Vec<Shelf>,
    at: Instant,
}

#[derive(Default)]
struct Inner {
    login_task: Option<tokio::task::AbortHandle>,
    eng: Option<Arc<Engine>>,
    api: Option<Arc<dyn Api>>,
    state: State,
    conns: HashMap<u64, ConnInfo>,
    booting: bool,
    refreshing: bool,
    /// Another refresh was asked for during one.
    refresh_next: bool,
    /// Days until the developer token expires, when under 45.
    expires_in: i64,
    /// When the developer token expires, Unix seconds.
    expires: Option<i64>,
    /// A newer release, when one is available.
    update: String,
    /// A release installed but not yet restarted into.
    installed: String,
    /// Artist pages and charts, briefly.
    pages: HashMap<String, CachedPage>,
    /// Chrome was closed for being idle; a play command wakes it.
    asleep: bool,
    /// When the player last played or was told something.
    active: Option<Instant>,
    /// When to start the player again after a failure.
    retry_at: Option<Instant>,
    /// The wait after the next failure.
    retry: Duration,
    /// The library was loaded once; restarts skip it.
    booted: bool,
    /// Invalidates checks when another transport command arrives.
    play_check: u64,
    /// The last queue and song.
    resume: Option<Resume>,
}

pub struct Daemon {
    version: String,
    mpris: OnceLock<mpris::Server>,
    lib: Library,
    session: session::SessionGate,
    inner: Mutex<Inner>,
    /// True once the daemon is to stop.
    quit: tokio::sync::watch::Sender<bool>,
    /// Asks check_updates to look now.
    check_now: Notify,
    /// The spectrum loop has something new to consider.
    nudge: Notify,
    next_conn: AtomicU64,
}

/// Seconds as Go writes a duration rounded to the second: 2s, 1m0s.
fn go_duration(d: Duration) -> String {
    let s = d.as_secs_f64().round() as u64;
    if s >= 60 {
        format!("{}m{}s", s / 60, s % 60)
    } else {
        format!("{s}s")
    }
}

type Task = std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>;

fn same(a: &Option<Arc<Engine>>, b: &Arc<Engine>) -> bool {
    a.as_ref().is_some_and(|a| Arc::ptr_eq(a, b))
}

/// Serves until SIGTERM, SIGINT or a quit request.
pub async fn run(version: &str) -> Result<()> {
    let ln = listen().await?;
    engine::sweep_profiles(); // left behind by crashes
    config::migrate_session_cache()?;
    let cfg = Config::load()?;
    let authenticated = crate::provider::registry::definition(cfg.effective_provider())
        .is_ok_and(|p| p.signed_in(&cfg));
    let d = Arc::new(Daemon::new(
        version,
        if authenticated {
            Library::load()
        } else {
            Library::default()
        },
        if authenticated { Resume::load() } else { None },
    ));
    match mpris::Server::start(Arc::downgrade(&d)).await {
        Ok(m) => {
            let _ = d.mpris.set(m);
        }
        Err(e) => eprintln!("mpris disabled: {e}"),
    }
    tokio::spawn(accept(d.clone(), ln));
    tokio::spawn(d.clone().boot());
    tokio::spawn(d.clone().spectrum_loop());
    tokio::spawn(d.clone().check_updates());
    tokio::spawn(d.clone().watch_self());

    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
        _ = d.quitted() => {}
    }
    let _ = std::fs::remove_file(config::socket());
    let eng = {
        let mut i = d.inner.lock().unwrap();
        if let Some(r) = i.resume.as_mut() {
            r.save();
        }
        i.eng.clone()
    };
    if let Some(e) = eng {
        e.close().await;
    }
    if let Some(m) = d.mpris.get() {
        m.close().await;
    }
    Ok(())
}

/// Claims the socket, clearing a stale one left by a crash but refusing
/// to start next to a live daemon.
async fn listen() -> Result<UnixListener> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    // The app's cache stays private even on the first start, before a
    // library or browser profile creates anything in it.
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(config::cache_dir())?;
    std::fs::set_permissions(config::cache_dir(), std::fs::Permissions::from_mode(0o700))?;
    let path = config::socket();
    if let Some(dir) = path.parent() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
        // Only the cache fallback belongs to brumm. Existing runtime
        // directories are managed by the caller or desktop session.
        if dir == config::cache_dir() {
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    if tokio::time::timeout(Duration::from_millis(300), UnixStream::connect(&path))
        .await
        .is_ok_and(|c| c.is_ok())
    {
        return Err(err("brumm daemon is already running"));
    }
    let _ = std::fs::remove_file(&path);
    let ln = UnixListener::bind(&path)?;
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    Ok(ln)
}

async fn accept(d: Arc<Daemon>, ln: UnixListener) {
    loop {
        match ln.accept().await {
            Ok((s, _)) => {
                tokio::spawn(d.clone().serve(s));
            }
            Err(_) => {
                if *d.quit.borrow() {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
}

impl Daemon {
    fn new(version: &str, lib: Library, resume: Option<Resume>) -> Daemon {
        Daemon {
            version: version.to_string(),
            mpris: OnceLock::new(),
            lib,
            session: session::SessionGate::default(),
            inner: Mutex::new(Inner {
                state: State {
                    status: ipc::STATUS_STARTING.into(),
                    message: "starting".into(),
                    ..Default::default()
                },
                resume,
                active: Some(Instant::now()),
                ..Default::default()
            }),
            quit: tokio::sync::watch::Sender::new(false),
            check_now: Notify::new(),
            nudge: Notify::new(),
            next_conn: AtomicU64::new(0),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn stop(&self) {
        self.quit.send_replace(true);
    }

    /// Returns once the daemon is to stop.
    async fn quitted(&self) {
        let mut rx = self.quit.subscribe();
        let _ = rx.wait_for(|q| *q).await;
    }

    async fn set_status(&self, status: &str, msg: &str) {
        let st = {
            let mut i = self.lock();
            i.state.status = status.to_string();
            i.state.message = msg.to_string();
            i.state.clone()
        };
        self.broadcast(Message {
            state: Some(st),
            ..Default::default()
        })
        .await;
    }

    /// Connects to Apple Music first — so the library refreshes while
    /// Chrome is still being installed — then starts playback. Only one
    /// boot runs at a time.
    async fn boot(self: Arc<Self>) {
        let generation = self.session.current();
        self.boot_for(generation).await;
    }

    async fn boot_for(self: Arc<Self>, generation: i64) {
        let mut changed = self.session.subscribe();
        let Ok(_lease) = self.session.read(generation).await else {
            return;
        };
        {
            let mut i = self.lock();
            if i.booting {
                return;
            }
            i.booting = true;
        }
        tokio::select! { _ = self.clone().boot_once() => {}, _ = changed.changed() => {} }
        self.lock().booting = false;
    }

    async fn boot_once(self: Arc<Self>) {
        let cfg = match Config::load() {
            Ok(c) => c,
            Err(e) => return self.set_status(ipc::STATUS_ERROR, &e.to_string()).await,
        };
        if cfg.needs_provider_choice() {
            return self
                .set_status(ipc::STATUS_CHOOSE_PROVIDER, "Choose your music provider")
                .await;
        }
        let api = match new_api(&cfg) {
            Ok(api) => api,
            Err(e) => return self.set_status(ipc::STATUS_ERROR, &e.to_string()).await,
        };
        let provider = match crate::provider::registry::definition(cfg.effective_provider()) {
            Ok(p) => p,
            Err(e) => return self.set_status(ipc::STATUS_ERROR, &e.to_string()).await,
        };
        if let Err(e) = provider.validate(&cfg) {
            return self.set_status(ipc::STATUS_ERROR, &e.to_string()).await;
        }
        if !provider.signed_in(&cfg) {
            return self
                .set_status(ipc::STATUS_LOGGED_OUT, "not signed in")
                .await;
        }
        let first = {
            let mut i = self.lock();
            i.api = Some(api);
            if let Some(exp) = provider.expiry(&cfg) {
                i.expires = Some(exp);
                let days = (exp - chrono::Utc::now().timestamp()) / 86400;
                if days < 45 {
                    i.expires_in = days.max(0);
                }
            }
            let first = !i.booted;
            i.booted = true;
            first
        };
        if first {
            self.spawn_refresh();
        }
        let progress = {
            let generation = self.session.current();
            let d = self.clone();
            move |m: &str| {
                let (d, m) = (d.clone(), m.to_string());
                tokio::spawn(async move {
                    if let Ok(_lease) = d.session.read(generation).await {
                        d.set_status(ipc::STATUS_STARTING, &m).await;
                    }
                });
            }
        };
        if let Err(e) = chrome::ensure(progress).await {
            return self.failed(e).await;
        }
        self.set_status(ipc::STATUS_STARTING, "starting player")
            .await;
        let eng = match match provider.playback(&cfg, &self.version) {
            Ok(page) => Engine::start(page).await,
            Err(e) => Err(e),
        } {
            Ok(e) => e,
            Err(e) => return self.failed(e).await,
        };
        let old = {
            let mut i = self.lock();
            let old = i.eng.replace(eng.clone());
            i.asleep = false;
            i.active = Some(Instant::now());
            old
        };
        if let Some(old) = old {
            tokio::spawn(async move { old.close().await });
        }
        let _ = eng.set_autoplay(!Options::load().no_autoplay).await;
        self.spawn_watch(eng);
    }

    /// Shows why the player did not start and schedules another try,
    /// waiting longer each time so a broken Chrome is not restarted in a
    /// loop.
    async fn failed(&self, e: Error) {
        let wait = {
            let mut i = self.lock();
            i.retry = (i.retry * 2).clamp(RETRY_FIRST, RETRY_MAX);
            i.retry_at = Some(Instant::now() + i.retry);
            i.retry
        };
        self.poke();
        self.set_status(
            ipc::STATUS_ERROR,
            &format!("{e} (trying again in {})", go_duration(wait)),
        )
        .await;
    }

    /// Wakes the spectrum loop to look at what it should do now.
    fn poke(&self) {
        self.nudge.notify_one();
    }

    fn engine(&self) -> Option<Arc<Engine>> {
        self.lock().eng.clone()
    }

    // Background work that starts more of itself (boot → watch → restart
    // → boot, refresh → refresh) is spawned through plain functions: an
    // async function spawning itself would make its future's type depend
    // on itself.

    pub(super) fn spawn_boot(self: &Arc<Self>) {
        let d = self.clone();
        let generation = self.session.current();
        let task: Task = Box::pin(async move { d.boot_for(generation).await });
        tokio::spawn(task);
    }

    fn spawn_refresh(self: &Arc<Self>) {
        let d = self.clone();
        let generation = self.session.current();
        let task: Task = Box::pin(async move { d.refresh_for(generation).await });
        tokio::spawn(task);
    }

    fn spawn_watch(self: &Arc<Self>, eng: Arc<Engine>) {
        let d = self.clone();
        let task: Task = Box::pin(async move { d.watch(eng).await });
        tokio::spawn(task);
    }

    /// Follows one player: the page answers the moment its state changes
    /// (and at least every heartbeat), so nothing polls it. A dead or hung
    /// page is restarted, with growing waits if it keeps failing. It ends
    /// when the engine is replaced or closed.
    async fn watch(self: Arc<Self>, eng: Arc<Engine>) {
        let mut rev = 0;
        let mut last_ok = Instant::now();
        loop {
            let heart = if self.playing() {
                HEART_PLAY
            } else {
                HEART_PAUSE
            };
            let generation = self.session.current();
            let mut changed = self.session.subscribe();
            let Ok(_lease) = self.session.read(generation).await else {
                return;
            };
            let r =
                tokio::select! { r = eng.watch(rev, heart) => r, _ = changed.changed() => return };
            if !same(&self.lock().eng, &eng) {
                return;
            }
            match r {
                Err(e) => {
                    if !eng.alive()
                        || matches!(e, CdpError::Timeout(_))
                        || last_ok.elapsed() > STALE_AFTER
                    {
                        self.restart(&eng).await;
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(500)).await; // a hiccup while the page loads
                }
                Ok((es, r)) => {
                    rev = r;
                    last_ok = Instant::now();
                    self.lock().retry = Duration::ZERO; // the player works: the next failure waits briefly again
                    self.apply(es.clone()).await;
                    if self.idle(&eng, &es) {
                        return;
                    }
                }
            }
        }
    }

    /// Drops a dead player so this runs once; boot starts a new one.
    async fn restart(self: &Arc<Self>, eng: &Arc<Engine>) {
        {
            let mut i = self.lock();
            if !same(&i.eng, eng) {
                return;
            }
            i.eng = None;
        }
        let e = eng.clone();
        tokio::spawn(async move { e.close().await }); // a hung page may take a while to close
        self.set_status(ipc::STATUS_STARTING, "restarting player")
            .await;
        self.spawn_boot();
    }

    /// Closes Chrome once nothing has played for SLEEP_AFTER, saving its
    /// memory and every wakeup; the next play command starts it again
    /// where the song stopped. Says whether it did.
    fn idle(&self, eng: &Arc<Engine>, es: &Player) -> bool {
        let sleep = {
            let mut i = self.lock();
            if es.playing || es.preview.is_some() || !es.ready {
                i.active = Some(Instant::now());
            }
            let quiet = i.active.is_none_or(|a| a.elapsed() > SLEEP_AFTER);
            let sleep = quiet && same(&i.eng, eng) && !i.booting;
            if sleep {
                if let Some(r) = i.resume.as_mut() {
                    r.save();
                }
                i.eng = None;
                i.asleep = true;
            }
            sleep
        };
        if sleep {
            let e = eng.clone();
            tokio::spawn(async move { e.close().await });
        }
        sleep
    }

    /// Starts Chrome again after idle; with play, the saved song plays as
    /// soon as the player is up. Says whether the player was asleep.
    async fn wake(self: &Arc<Self>, play: bool) -> bool {
        {
            let mut i = self.lock();
            if !i.asleep {
                return false;
            }
            i.asleep = false;
            i.active = Some(Instant::now());
            if play && let Some(r) = i.resume.as_mut() {
                r.autoplay = true;
            }
        }
        self.set_status(ipc::STATUS_STARTING, "waking the player")
            .await;
        self.spawn_boot();
        true
    }

    /// Feeds the spectrum: one frame per tick while music plays and some
    /// player watches it. Otherwise it sleeps until poked, or until a
    /// failed player start is due for another try.
    async fn spectrum_loop(self: Arc<Self>) {
        let mut wait = IDLE_WAKE;
        loop {
            tokio::select! {
                _ = self.quitted() => return,
                _ = self.nudge.notified() => {}
                _ = tokio::time::sleep(wait) => {}
            }
            let generation = self.session.current();
            let mut changed = self.session.subscribe();
            if let Ok(_lease) = self.session.read(generation).await {
                wait = tokio::select! { wait = self.step() => wait, _ = changed.changed() => IDLE_WAKE };
            }
        }
    }

    /// Does one round of the loop and says when to run the next.
    async fn step(self: &Arc<Self>) -> Duration {
        let Some(eng) = self.engine() else {
            let at = self.lock().retry_at;
            let Some(at) = at else { return IDLE_WAKE };
            let now = Instant::now();
            if at > now {
                return at - now;
            }
            self.lock().retry_at = None;
            self.spawn_boot();
            return IDLE_WAKE;
        };
        let (bands, wave, fps) = self.wants();
        if !self.playing() || (bands == 0 && wave == 0) {
            return IDLE_WAKE;
        }
        if bands > 0
            && let Ok(spec) = eng.spectrum(bands).await
            && !spec.is_empty()
        {
            self.broadcast(Message {
                spectrum: spec,
                ..Default::default()
            })
            .await;
        }
        if wave > 0
            && let Ok(w) = eng.wave(wave).await
            && !w.is_empty()
        {
            self.broadcast(Message {
                wave: w,
                ..Default::default()
            })
            .await;
        }
        Duration::from_secs(1) / fps.max(1) as u32
    }

    fn playing(&self) -> bool {
        self.lock().state.player.playing
    }

    async fn apply(self: &Arc<Self>, mut es: Player) {
        let (status, msg) = if es.needs_auth {
            (ipc::STATUS_LOGGED_OUT, "music session expired".to_string())
        } else if !es.err.is_empty() {
            (ipc::STATUS_ERROR, es.err.clone())
        } else if es.ready {
            (ipc::STATUS_READY, String::new())
        } else {
            (ipc::STATUS_STARTING, "loading music".to_string())
        };
        let (next, changed, started) = {
            let mut i = self.lock();
            if let Some(r) = i.resume.as_mut()
                && r.autoplay
                && es.ready
            {
                r.autoplay = false;
                let r = r.clone();
                let d = self.clone();
                tokio::spawn(async move {
                    if let Some(eng) = d.engine() {
                        let _ = r.play(&eng).await;
                    }
                });
            }
            if let Some(r) = i.resume.as_mut() {
                if es.title.is_empty() && es.preview.is_none() {
                    es = r.overlay(es); // idle: show the saved song, paused
                } else {
                    r.track(&es);
                }
            }
            let next = State {
                auth: i.state.auth.clone(),
                status: status.into(),
                message: msg,
                expires_in: i.expires_in,
                update: i.update.clone(),
                player: es.clone(),
            };
            let changed = next != i.state;
            let started = es.playing && !i.state.player.playing;
            i.state = next.clone();
            (next, changed, started)
        };
        if started {
            self.poke(); // the spectrum loop starts
        }
        if let Some(m) = self.mpris.get() {
            m.update(&es).await;
        }
        if changed {
            self.broadcast(Message {
                state: Some(next),
                ..Default::default()
            })
            .await;
        }
    }

    /// The most spectrum bands and waveform samples any player asks for,
    /// and the fastest frame rate among those that ask.
    fn wants(&self) -> (i64, i64, i64) {
        let i = self.lock();
        let (mut bands, mut wave, mut fps) = (0, 0, 0);
        for c in i.conns.values() {
            bands = bands.max(c.bands);
            wave = wave.max(c.wave);
            if c.bands > 0 || c.wave > 0 {
                fps = fps.max(c.fps);
            }
        }
        (bands, wave, fps.max(1))
    }

    /// Pushes to subscribers; spectrum frames only to those that want
    /// them. A player that cannot keep up is disconnected.
    async fn broadcast(&self, mut m: Message) {
        if m.generation == 0
            && (m.state.is_some() || m.library || !m.spectrum.is_empty() || !m.wave.is_empty())
        {
            m.generation = self.session.current();
        }
        let targets: Vec<Arc<ConnOut>> = {
            let i = self.lock();
            i.conns
                .values()
                .filter(|c| {
                    c.sub
                        && (m.spectrum.is_empty() || c.bands > 0)
                        && (m.wave.is_empty() || c.wave > 0)
                })
                .map(|c| c.out.clone())
                .collect()
        };
        if targets.is_empty() {
            return;
        }
        let line = ipc::encode(&m);
        for c in targets {
            c.send(&line).await;
        }
    }

    async fn serve(self: Arc<Self>, s: UnixStream) {
        let (r, w) = s.into_split();
        let out = Arc::new(ConnOut {
            writer: tokio::sync::Mutex::new(w),
            dead: Notify::new(),
            is_dead: AtomicBool::new(false),
        });
        let id = self.next_conn.fetch_add(1, Ordering::Relaxed);
        self.lock().conns.insert(
            id,
            ConnInfo {
                out: out.clone(),
                sub: false,
                bands: 0,
                wave: 0,
                fps: 0,
            },
        );
        // At most 8 requests in flight per player.
        let busy = Arc::new(Semaphore::new(8));
        let mut r = BufReader::with_capacity(64 << 10, r);
        let mut line = Vec::new();
        loop {
            let got = tokio::select! {
                got = ipc::read_line(&mut r, &mut line, 4 << 20) => got,
                _ = out.dead.notified() => break,
            };
            if !matches!(got, Ok(true)) {
                break;
            }
            let Ok(mut req) = serde_json::from_slice::<Request>(&line) else {
                continue;
            };
            if req.generation == 0 {
                req.generation = self.session.current();
            }
            // Requests may be slow (network); answer them side by side so a
            // search does not hold up a pause.
            let Ok(permit) = busy.clone().acquire_owned().await else {
                break;
            };
            let (d, out) = (self.clone(), out.clone());
            tokio::spawn(async move {
                let mut reply = Message {
                    id: req.id,
                    ..Default::default()
                };
                if let Err(e) = d.handle(id, &req, &mut reply).await {
                    reply.error = e.to_string();
                }
                out.send(&ipc::encode(&reply)).await;
                drop(permit);
            });
        }
        self.lock().conns.remove(&id);
        out.kill();
        let _ = out.writer.lock().await.shutdown().await;
    }

    async fn handle(self: &Arc<Self>, conn: u64, r: &Request, reply: &mut Message) -> Result<()> {
        let generation = if r.generation == 0 {
            self.session.current()
        } else {
            r.generation
        };
        let _lease = if ipc::session_free(&r.cmd) {
            None
        } else {
            Some(self.session.read(generation).await?)
        };
        if !ipc::session_free(&r.cmd) {
            reply.generation = generation;
        }
        self.dispatch(conn, r, reply).await
    }

    fn provider_selection(&self, cfg: &Config) -> crate::provider::registry::Selection {
        let mut selection = crate::provider::registry::selection(cfg);
        selection.generation = self.session.current();
        selection
    }

    async fn dispatch(self: &Arc<Self>, conn: u64, r: &Request, reply: &mut Message) -> Result<()> {
        if r.protocol_version != 0 && r.protocol_version != ipc::PROTOCOL_VERSION {
            return Err(err(
                "unsupported socket protocol; update Brumm's daemon and frontends together",
            ));
        }
        if matches!(
            r.cmd.as_str(),
            ipc::CMD_PLAY
                | ipc::CMD_STATION
                | ipc::CMD_NEXT
                | ipc::CMD_PREV
                | ipc::CMD_JUMP
                | ipc::CMD_PREVIEW
        ) {
            self.cancel_play_check();
        }
        match r.cmd.as_str() {
            ipc::CMD_SUBSCRIBE => {
                let _snapshot = self.session.snapshot().await;
                let st = {
                    let mut i = self.lock();
                    if let Some(c) = i.conns.get_mut(&conn) {
                        c.sub = true;
                        c.bands = r.bands.min(256);
                        c.wave = r.wave.min(1024);
                        c.fps = if r.fps > 0 {
                            r.fps.min(60)
                        } else {
                            DEFAULT_FPS
                        };
                    }
                    i.state.clone()
                };
                self.poke();
                reply.state = Some(st);
                reply.provider = Some(self.provider_selection(&Config::load()?));
                return Ok(());
            }
            ipc::CMD_CANCEL_LOGIN => {
                self.cancel_login().await;
                return Ok(());
            }
            ipc::CMD_PROVIDER => return self.select_provider(&r.query, reply).await,
            ipc::CMD_LIST => return self.list(&r.list, reply).await,
            ipc::CMD_OPEN => {
                let it = r.item.as_ref().ok_or_else(|| err("open: no item"))?;
                return self.open(it, reply).await;
            }
            ipc::CMD_SEARCH => {
                let api = self.client()?;
                reply.shelves = self.auth_check(api.search(&r.query).await).await?;
                return Ok(());
            }
            ipc::CMD_FAVORITE => {
                let api = self.client()?;
                self.auth_check(api.favorite(&r.refs).await).await?;
                self.spawn_refresh();
                return Ok(());
            }
            ipc::CMD_CREATE => {
                let api = self.client()?;
                let it = self
                    .auth_check(api.create_container(&r.list, &r.query, &r.start).await)
                    .await?;
                reply.items = vec![it];
                self.spawn_refresh();
                return Ok(());
            }
            ipc::CMD_EXPLORE => return self.explore(&r.query, reply).await,
            ipc::CMD_HOME => {
                let api = self.client()?;
                reply.shelves = self.auth_check(api.home().await).await?;
                return Ok(());
            }
            ipc::CMD_STATION => return self.station(r, reply).await,
            ipc::CMD_PLAYLIST => return self.add_to_playlist(r, reply).await,
            ipc::CMD_ALBUM => {
                let api = self.client()?;
                reply.items = vec![self.auth_check(api.album_of(&r.start).await).await?];
                return Ok(());
            }
            ipc::CMD_RATINGS => {
                let api = self.client()?;
                reply.ratings = self.auth_check(api.ratings(&r.refs).await).await?;
                return Ok(());
            }
            ipc::CMD_RATE => {
                let api = self.client()?;
                let first = r.refs.first().ok_or_else(|| err("rate: nothing named"))?;
                return self.auth_check(api.rate(first, r.value as i64).await).await;
            }
            ipc::CMD_ADD => {
                let api = self.client()?;
                self.auth_check(api.add_to_library(&r.list, &r.ids).await)
                    .await?;
                let d = self.clone();
                let generation = self.session.current();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(5)).await; // Apple adds in the background
                    d.refresh_for(generation).await;
                });
                return Ok(());
            }
            ipc::CMD_ARTIST => {
                let api = self.client()?;
                reply.items = vec![self.auth_check(api.artist_of(&r.start).await).await?];
                return Ok(());
            }
            ipc::CMD_LINK => {
                let api = self.client()?;
                reply.link = self.auth_check(api.link(&r.start).await).await?;
                return Ok(());
            }
            ipc::CMD_UPDATE => {
                match r.value as i64 {
                    // Look now and say: link is the newest release, pos 1 if it is newer.
                    2 => return self.check_now_for(reply).await,
                    // Installed: restart into it now, playback continues.
                    3 => {
                        let (d, playing) = (self.clone(), self.playing());
                        tokio::spawn(async move {
                            tokio::time::sleep(Duration::from_millis(200)).await; // the reply goes out first
                            d.restart_for_update(playing).await;
                        });
                        return Ok(());
                    }
                    0 => {
                        self.check_now.notify_one(); // just look
                        return Ok(());
                    }
                    _ => return self.install_update().await,
                }
            }
            ipc::CMD_RELOAD => {
                tokio::spawn(self.clone().reload());
                return Ok(());
            }
            ipc::CMD_QUIT => {
                self.stop();
                return Ok(());
            }
            ipc::CMD_OPTIONS => return self.options(r.options.as_ref(), reply).await,
            ipc::CMD_LOGIN => {
                self.login();
                return Ok(());
            }
            ipc::CMD_SHOW => return launch::open(&r.query),
            ipc::CMD_RESOLVE => return self.resolve(&r.query, reply).await,
            ipc::CMD_FEEDBACK => {
                reply.link = feedback::send(&r.query, &self.version, &r.source, r.value == 1.0)?;
                return Ok(());
            }
            ipc::CMD_PLACE => {
                frontends::place(r.place.as_ref(), reply);
                return Ok(());
            }
            _ => {}
        }

        let Some(eng) = self.engine() else {
            return self.asleep_cmd(r, reply).await;
        };
        if r.cmd != ipc::CMD_WARM {
            // Browsing alone does not keep a paused player up.
            self.lock().active = Some(Instant::now());
        }
        match r.cmd.as_str() {
            ipc::CMD_PLAY => {
                {
                    let mut i = self.lock();
                    let res = i.resume.get_or_insert_with(Resume::default);
                    res.ids = r.ids.clone();
                    res.source = r.source.clone();
                    res.station.clear();
                }
                eng.play_ids(&r.ids, &r.start, &r.source, 0.0).await
            }
            ipc::CMD_TOGGLE => self.toggle_playback(&eng).await,
            ipc::CMD_NEXT => eng.next().await,
            ipc::CMD_PREV => eng.prev().await,
            ipc::CMD_SEEK => eng.seek(r.value).await,
            ipc::CMD_VOLUME => eng.set_volume(r.value).await,
            ipc::CMD_PREVIEW => {
                if r.value == 0.0 {
                    eng.stop_preview().await
                } else {
                    eng.preview(&r.start).await
                }
            }
            ipc::CMD_QUEUE => {
                let q = eng.queue((r.value as i64).max(1)).await?;
                reply.tracks = q.items;
                reply.pos = q.pos;
                Ok(())
            }
            ipc::CMD_JUMP => eng.jump(r.value as i64).await,
            ipc::CMD_ENQUEUE => eng.enqueue(&r.ids, r.value != 0.0).await,
            ipc::CMD_SHUFFLE => eng.set_shuffle(r.value != 0.0).await,
            ipc::CMD_REPEAT => eng.set_repeat(r.value as i64).await,
            ipc::CMD_AUTOPLAY => eng.set_autoplay(r.value != 0.0).await,
            ipc::CMD_WARM => eng.warm(&r.ids).await,
            other => Err(err(format!("unknown command {other:?}"))),
        }
    }

    /// Answers a player command while there is no Chrome: playing wakes
    /// the player, which then plays; the rest waits for that.
    async fn asleep_cmd(self: &Arc<Self>, r: &Request, reply: &mut Message) -> Result<()> {
        match r.cmd.as_str() {
            ipc::CMD_PLAY => {
                {
                    let mut i = self.lock();
                    if i.asleep {
                        i.resume = Some(Resume {
                            ids: r.ids.clone(),
                            source: r.source.clone(),
                            id: r.start.clone(),
                            ..Default::default()
                        });
                    }
                }
                if self.wake(true).await {
                    return Ok(());
                }
            }
            ipc::CMD_TOGGLE | ipc::CMD_NEXT | ipc::CMD_PREV => {
                if self.wake(true).await {
                    return Ok(());
                }
            }
            // A new player reads autoplay from the options; warming waits for one.
            ipc::CMD_AUTOPLAY | ipc::CMD_WARM => return Ok(()),
            ipc::CMD_QUEUE => {
                if self.lock().asleep {
                    reply.pos = -1; // nothing is queued while the player sleeps
                    return Ok(());
                }
            }
            _ => {
                if self.wake(false).await {
                    return Err(err("the player was asleep and is waking; try again"));
                }
            }
        }
        Err(err("the player is still starting"))
    }

    /// A library list from the cache, fetched on a miss.
    async fn list(self: &Arc<Self>, name: &str, reply: &mut Message) -> Result<()> {
        if name == ipc::LIST_SONGS {
            if let Some(v) = self.lib.tracks(name) {
                reply.tracks = v;
                return Ok(());
            }
        } else if let Some(v) = self.lib.items(name) {
            reply.items = v;
            return Ok(());
        }
        let api = self.client()?;
        let r = self.fetch_list(api.as_ref(), name, reply).await;
        self.auth_check(r).await
    }

    async fn fetch_list(&self, api: &dyn Api, name: &str, reply: &mut Message) -> Result<()> {
        let e = match name {
            ipc::LIST_PLAYLISTS => {
                let (v, e) = api.playlists().await;
                reply.items = v;
                e
            }
            ipc::LIST_ALBUMS => {
                let (v, e) = api.albums().await;
                reply.items = v;
                e
            }
            ipc::LIST_ARTISTS => {
                let (v, e) = api.artists().await;
                reply.items = v;
                e
            }
            ipc::LIST_SONGS => {
                let (v, e) = api.songs().await;
                reply.tracks = v;
                e
            }
            other => return Err(err(format!("unknown list {other:?}"))),
        };
        match e {
            // Show what arrived, but do not cache an incomplete list.
            Some(Error::Partial) if reply.items.len() + reply.tracks.len() > 0 => return Ok(()),
            Some(e) => return Err(e),
            None => {}
        }
        if name == ipc::LIST_SONGS {
            self.lib.set_tracks(name, reply.tracks.clone());
        } else {
            self.lib.set_items(name, reply.items.clone());
        }
        Ok(())
    }

    /// What is inside an item: tracks, or an artist's page.
    async fn open(self: &Arc<Self>, it: &Item, reply: &mut Message) -> Result<()> {
        if !it.route.is_empty() && it.kind != KIND_ALBUM && it.kind != KIND_PLAYLIST {
            return self.explore(&it.route, reply).await;
        }
        let key = it.key();
        match it.kind.as_str() {
            KIND_ARTIST | KIND_SHELF => return self.page(it, reply).await,
            KIND_STATION | KIND_TERM => return Err(err("this plays; it does not open")),
            _ => {}
        }
        if let Some(v) = self.lib.tracks(&key) {
            reply.tracks = v;
            return Ok(());
        }
        let api = self.client()?;
        let (tracks, e) = api.tracks(it).await;
        reply.tracks = tracks;
        match e {
            Some(Error::Partial) => return Ok(()), // show, don't cache
            Some(e) => return self.auth_check(Err(e)).await,
            None => {}
        }
        self.lib.set_tracks(&key, reply.tracks.clone());
        Ok(())
    }

    /// An artist page or the charts, from memory when asked for in the
    /// last half hour: they change slowly and cost several requests.
    async fn page(self: &Arc<Self>, it: &Item, reply: &mut Message) -> Result<()> {
        let key = it.key();
        if let Some(p) = self
            .lock()
            .pages
            .get(&key)
            .filter(|p| p.at.elapsed() < Duration::from_secs(30 * 60))
        {
            reply.shelves = p.shelves.clone();
            return Ok(());
        }
        let api = self.client()?;
        let r = if it.kind == KIND_ARTIST {
            api.artist(it).await
        } else {
            api.open(it).await
        };
        reply.shelves = self.auth_check(r).await?;
        let mut i = self.lock();
        if i.pages.len() > 50 {
            i.pages.clear();
        }
        i.pages.insert(
            key,
            CachedPage {
                shelves: reply.shelves.clone(),
                at: Instant::now(),
            },
        );
        Ok(())
    }

    /// Plays a station: the one named, or the one Apple makes from an
    /// artist or a song. A sleeping player wakes up to play it.
    async fn station(self: &Arc<Self>, r: &Request, reply: &mut Message) -> Result<()> {
        let st = match &r.item {
            Some(it) if it.kind == KIND_STATION => it.clone(),
            _ => {
                let api = self.client()?;
                let it = r.item.clone().unwrap_or_default();
                self.auth_check(api.station(&it, &r.start).await).await?
            }
        };
        reply.items = vec![st.clone()];
        let source = format!("station:{}", st.id);
        {
            let mut i = self.lock();
            let res = i.resume.get_or_insert_with(Resume::default);
            res.station = st.id.clone();
            res.ids.clear();
            res.source = source.clone();
            res.title = st.name.clone();
            res.artwork = st.artwork.clone();
            i.active = Some(Instant::now());
        }
        if let Some(eng) = self.engine() {
            return eng.play_station(&st.id, &source).await;
        }
        if self.wake(true).await {
            return Ok(());
        }
        Err(err("the player is still starting"))
    }

    /// Adds songs to a playlist, or to a new one named r.query.
    async fn add_to_playlist(self: &Arc<Self>, r: &Request, reply: &mut Message) -> Result<()> {
        let api = self.client()?;
        if r.ids.is_empty() {
            return Err(err("no songs to add"));
        }
        if r.start.is_empty() {
            reply.items = vec![
                self.auth_check(api.create_playlist(&r.query, &r.ids).await)
                    .await?,
            ];
        } else {
            self.auth_check(api.add_to_playlist(&r.start, &r.ids).await)
                .await?;
        }
        let d = self.clone();
        let generation = self.session.current();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(3)).await; // Apple saves in the background
            d.refresh_for(generation).await;
        });
        Ok(())
    }

    fn client(&self) -> Result<Arc<dyn Api>> {
        self.lock().api.clone().ok_or_else(|| err("still starting"))
    }

    /// Says the session expired, when Apple says so.
    async fn auth_check<T>(&self, r: Result<T>) -> Result<T> {
        if let Err(Error::Unauthorized) = &r {
            self.set_status(ipc::STATUS_LOGGED_OUT, "music session expired")
                .await;
        }
        r
    }

    /// Picks up a fresh login: hot-swaps the token into a running page, or
    /// starts the player if it never ran.
    async fn reload(self: Arc<Self>) {
        let generation = self.session.current();
        self.reload_for(generation).await;
    }

    async fn reload_for(self: Arc<Self>, generation: i64) {
        let _one = frontends::PROVIDER.lock().await;
        if self.session.current() != generation {
            return;
        }
        let _exclusive = self.session.advance().await;
        // The same account can keep its playback queue when only credentials
        // changed. Watchers were cancelled by the transition and are restarted.
        if let Ok(cfg) = Config::load()
            && config::session_cache_dir_for(&cfg) == self.lib.namespace()
            && let Some(engine) = self.engine()
            && let Ok(provider) = crate::provider::registry::definition(cfg.effective_provider())
            && provider.signed_in(&cfg)
            && let (Ok(api), Ok(page)) =
                (provider.api(&cfg), provider.playback(&cfg, &self.version))
            && engine.refresh_session(page).await.is_ok()
        {
            self.lock().api = Some(api);
            let state = self.lock().state.clone();
            self.broadcast(Message {
                provider: Some(self.provider_selection(&cfg)),
                state: Some(state),
                ..Default::default()
            })
            .await;
            self.spawn_watch(engine);
            self.spawn_refresh();
            return;
        }
        if let Err(e) = self.install_session().await {
            self.set_status(ipc::STATUS_ERROR, &e.to_string()).await;
        }
    }

    /// Re-reads the library from Apple Music — the lists first, then every
    /// playlist's tracks — saving and telling players after each step.
    async fn refresh_for(self: Arc<Self>, generation: i64) {
        let mut changed = self.session.subscribe();
        let Ok(_lease) = self.session.read(generation).await else {
            return;
        };
        let api = {
            let mut i = self.lock();
            if i.refreshing {
                i.refresh_next = true; // e.g. a new login: refresh again with its client
                return;
            }
            let Some(api) = i.api.clone() else { return };
            i.refreshing = true;
            api
        };
        tokio::select! { _ = self.refresh_with(api.as_ref()) => {}, _ = changed.changed() => {} }
        let again = {
            let mut i = self.lock();
            let again = i.refresh_next;
            i.refreshing = false;
            i.refresh_next = false;
            again
        };
        if again {
            self.spawn_refresh();
        }
    }

    async fn refresh_with(self: &Arc<Self>, api: &dyn Api) {
        for name in [
            ipc::LIST_PLAYLISTS,
            ipc::LIST_ALBUMS,
            ipc::LIST_ARTISTS,
            ipc::LIST_SONGS,
        ] {
            let mut reply = Message::default();
            if let Err(Error::Unauthorized) = self.fetch_list(api, name, &mut reply).await {
                self.auth_check::<()>(Err(Error::Unauthorized)).await.ok();
                return;
            }
        }
        self.library_changed().await;
        for p in self.lib.items(ipc::LIST_PLAYLISTS).unwrap_or_default() {
            match api.tracks(&p).await {
                (_, Some(Error::Partial)) => continue, // keep the cached full list rather than a cut one
                (_, Some(Error::Unauthorized)) => {
                    self.auth_check::<()>(Err(Error::Unauthorized)).await.ok();
                    return;
                }
                (t, None) => self.lib.set_tracks(&p.key(), t),
                _ => {}
            }
        }
        self.library_changed().await;
    }

    async fn library_changed(&self) {
        if let Err(e) = self.lib.save() {
            eprintln!("library cache: {e}");
        }
        self.broadcast(Message {
            library: true,
            ..Default::default()
        })
        .await;
    }

    /// Starts the saved queue where it stopped when nothing is loaded yet,
    /// as after a restart. Says whether it did.
    async fn resume_if_idle(&self, eng: &Engine) -> bool {
        let r = {
            let i = self.lock();
            let idle = i.state.status == ipc::STATUS_READY && !i.state.player.playing;
            match &i.resume {
                Some(r) if idle && r.playable() => r.clone(),
                _ => return false,
            }
        };
        match eng.state().await {
            Ok(es) if es.title.is_empty() => r.play(eng).await.is_ok(),
            _ => false, // something is loaded: a plain toggle resumes it
        }
    }

    // What MPRIS asks of the player.

    pub async fn play(self: &Arc<Self>) {
        let seq = self.cancel_play_check();
        let Some(eng) = self.engine() else {
            self.wake(true).await;
            return;
        };
        self.mark_active();
        if !self.resume_if_idle(&eng).await && eng.play().await.is_err() {
            return;
        }
        tokio::spawn(self.clone().verify_playback(eng, seq));
    }

    pub async fn pause(self: &Arc<Self>) {
        self.cancel_play_check();
        if let Some(e) = self.engine() {
            self.mark_active();
            let _ = e.pause().await;
        }
    }

    pub async fn toggle(self: &Arc<Self>) {
        let Some(eng) = self.engine() else {
            self.wake(true).await;
            return;
        };
        self.mark_active();
        let _ = self.toggle_playback(&eng).await;
    }

    pub async fn next(self: &Arc<Self>) {
        self.cancel_play_check();
        match self.engine() {
            None => {
                self.wake(true).await;
            }
            Some(e) => {
                self.mark_active();
                let _ = e.next().await;
            }
        }
    }

    pub async fn prev(self: &Arc<Self>) {
        self.cancel_play_check();
        match self.engine() {
            None => {
                self.wake(true).await;
            }
            Some(e) => {
                self.mark_active();
                let _ = e.prev().await;
            }
        }
    }

    pub async fn seek(&self, sec: f64) {
        if let Some(e) = self.engine() {
            self.mark_active();
            let _ = e.seek(sec).await;
        }
    }

    pub async fn set_volume(&self, v: f64) {
        if let Some(e) = self.engine() {
            self.mark_active();
            let _ = e.set_volume(v).await;
        }
    }

    pub async fn set_shuffle(&self, on: bool) {
        if let Some(e) = self.engine() {
            self.mark_active();
            let _ = e.set_shuffle(on).await;
        }
    }

    pub async fn set_repeat(&self, mode: i64) {
        if let Some(e) = self.engine() {
            self.mark_active();
            let _ = e.set_repeat(mode).await;
        }
    }

    /// Brings the player to the front — the window or the terminal player,
    /// as the options pick — opening it if none is running.
    pub fn raise(&self) {
        if let Err(e) = launch::open("") {
            eprintln!("raise: {e}");
        }
    }

    fn mark_active(&self) {
        self.lock().active = Some(Instant::now());
    }

    /// Favorited library resources were fetched in the background: that
    /// snapshot is reused so opening Favorites does not rescan the library.
    async fn explore(self: &Arc<Self>, route: &str, reply: &mut Message) -> Result<()> {
        if let Some(typ) = route.strip_prefix("app:favorites:")
            && let Some(page) = self.favorites_from_cache(typ)
        {
            reply.explorer = Some(page);
            return Ok(());
        }
        let api = self.client()?;
        reply.explorer = Some(self.auth_check(api.feature(route).await).await?);
        Ok(())
    }

    fn favorites_from_cache(&self, typ: &str) -> Option<ExplorePage> {
        let mut page = ExplorePage {
            title: format!("Favorite {typ}"),
            ..Default::default()
        };
        let mut known = false;
        if typ == ipc::LIST_SONGS {
            let ts = self.lib.tracks(typ)?;
            known |= ts.is_empty();
            for t in ts {
                known |= t.details.iter().any(|d| d.label == "Favorite");
                if t.favorite {
                    page.tracks.push(t);
                }
            }
        } else if matches!(
            typ,
            ipc::LIST_ALBUMS | ipc::LIST_ARTISTS | ipc::LIST_PLAYLISTS
        ) {
            let items = self.lib.items(typ)?;
            known |= items.is_empty();
            for it in items {
                known |= it.details.iter().any(|d| d.label == "Favorite");
                if it.favorite {
                    page.items.push(it);
                }
            }
        }
        known.then_some(page)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::testenv;

    fn daemon() -> Arc<Daemon> {
        Arc::new(Daemon::new("dev", Library::default(), None))
    }

    async fn connect(d: &Arc<Daemon>) -> (ipc::Client, tokio::sync::mpsc::Receiver<Message>) {
        let (a, b) = UnixStream::pair().unwrap();
        tokio::spawn(d.clone().serve(b));
        ipc::Client::new(a)
    }

    #[tokio::test]
    async fn wants_the_most_any_player_asks_for() {
        let d = daemon();
        assert_eq!(d.wants(), (0, 0, 1));
        let (c1, _e1) = connect(&d).await;
        let (c2, _e2) = connect(&d).await;
        c1.request(Request {
            cmd: ipc::CMD_SUBSCRIBE.into(),
            bands: 48,
            fps: 15,
            ..Default::default()
        })
        .await
        .unwrap();
        // Asks for nothing: its rate must not count.
        c2.request(Request {
            cmd: ipc::CMD_SUBSCRIBE.into(),
            fps: 25,
            ..Default::default()
        })
        .await
        .unwrap();
        assert_eq!(d.wants(), (48, 0, 15));
        let (c3, _e3) = connect(&d).await;
        c3.request(Request {
            cmd: ipc::CMD_SUBSCRIBE.into(),
            bands: 48,
            wave: 128,
            fps: 999,
            ..Default::default()
        })
        .await
        .unwrap();
        assert_eq!(d.wants(), (48, 128, 60), "the rate is capped");
    }

    #[tokio::test]
    async fn broadcasts_reach_subscribers_and_slow_ones_are_dropped() {
        let d = daemon();
        let (c, mut events) = connect(&d).await;
        let (quiet, mut quiet_events) = connect(&d).await;
        c.request(Request {
            cmd: ipc::CMD_SUBSCRIBE.into(),
            ..Default::default()
        })
        .await
        .unwrap();
        quiet.request(Request::new(ipc::CMD_OPTIONS)).await.ok();
        d.broadcast(Message {
            library: true,
            ..Default::default()
        })
        .await;
        assert!(
            tokio::time::timeout(Duration::from_secs(1), events.recv())
                .await
                .unwrap()
                .unwrap()
                .library
        );
        // The spectrum goes only to those that asked for it.
        d.broadcast(Message {
            spectrum: vec![1, 2],
            ..Default::default()
        })
        .await;
        assert!(
            tokio::time::timeout(Duration::from_millis(100), events.recv())
                .await
                .is_err()
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(50), quiet_events.recv())
                .await
                .is_err(),
            "not subscribed"
        );
        // A player that stops reading is dropped, not waited for.
        let (mut stalled, theirs) = UnixStream::pair().unwrap();
        tokio::spawn(d.clone().serve(theirs));
        stalled
            .write_all(b"{\"id\":1,\"cmd\":\"subscribe\"}\n")
            .await
            .unwrap();
        while d.lock().conns.values().filter(|c| c.sub).count() < 2 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let big = Message {
            notes: vec!["x".repeat(1 << 20)],
            ..Default::default()
        };
        let start = Instant::now();
        for _ in 0..8 {
            d.broadcast(big.clone()).await;
            let _ = events.try_recv();
        }
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "a stalled player held the daemon"
        );
        for _ in 0..100 {
            if d.lock().conns.len() == 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            d.lock().conns.len(),
            2,
            "the stalled player was not dropped"
        );
        drop(stalled);
    }

    #[tokio::test]
    async fn requests_are_answered_side_by_side_with_errors() {
        let d = daemon();
        let (c, _e) = connect(&d).await;
        let r = c.request(Request::new("nonsense")).await;
        assert_eq!(r.unwrap_err().to_string(), "the player is still starting");
        let r = c
            .request(Request {
                cmd: ipc::CMD_LIST.into(),
                list: ipc::LIST_ALBUMS.into(),
                ..Default::default()
            })
            .await;
        assert_eq!(r.unwrap_err().to_string(), "still starting");
        let r = c.request(Request::new(ipc::CMD_OPEN)).await;
        assert_eq!(r.unwrap_err().to_string(), "open: no item");
        let q = c
            .request(Request {
                cmd: ipc::CMD_QUEUE.into(),
                value: 10.0,
                ..Default::default()
            })
            .await;
        assert_eq!(q.unwrap_err().to_string(), "the player is still starting");
        d.lock().asleep = true;
        let q = c
            .request(Request {
                cmd: ipc::CMD_QUEUE.into(),
                value: 10.0,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(q.pos, -1, "nothing is queued while the player sleeps");
    }

    /// Explore and Replay cards carry a route, but bulk song actions take
    /// the same complete track list as albums opened from the library.
    #[tokio::test]
    async fn routed_containers_open_as_tracks() {
        for kind in [KIND_ALBUM, KIND_PLAYLIST] {
            let it = Item {
                kind: kind.into(),
                id: "123".into(),
                catalog: true,
                route: format!("/v1/catalog/de/{kind}s/123"),
                ..Default::default()
            };
            let d = daemon();
            d.lib.set_tracks(
                &it.key(),
                vec![crate::music::Track {
                    id: "song".into(),
                    title: "Song".into(),
                    ..Default::default()
                }],
            );
            let mut reply = Message::default();
            d.open(&it, &mut reply).await.unwrap();
            assert!(reply.explorer.is_none());
            assert_eq!(reply.tracks.len(), 1);
            assert_eq!(reply.tracks[0].id, "song");
        }
    }

    #[test]
    fn favorites_come_from_the_cache_when_it_knows() {
        let d = Daemon::new("dev", Library::default(), None);
        assert!(
            d.favorites_from_cache("songs").is_none(),
            "nothing cached: ask Apple"
        );
        let fav = crate::music::Detail {
            label: "Favorite".into(),
            value: "true".into(),
        };
        d.lib.set_tracks(
            "songs",
            vec![
                crate::music::Track {
                    id: "a".into(),
                    favorite: true,
                    details: vec![fav.clone()],
                    ..Default::default()
                },
                crate::music::Track {
                    id: "b".into(),
                    details: vec![fav],
                    ..Default::default()
                },
            ],
        );
        let page = d.favorites_from_cache("songs").unwrap();
        assert_eq!(page.tracks.len(), 1);
        assert_eq!(page.title, "Favorite songs");
    }

    #[test]
    fn go_durations() {
        assert_eq!(go_duration(Duration::from_secs(2)), "2s");
        assert_eq!(go_duration(Duration::from_secs(60)), "1m0s");
    }

    #[test]
    fn options_change_and_announce() {
        let Some(_env) = testenv::isolated() else {
            return;
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            Options {
                cover: "smooth".into(),
                no_autoplay: true,
                ..Default::default()
            }
            .save()
            .unwrap();
            let d = daemon();
            let (c, mut events) = connect(&d).await;
            c.request(Request::new(ipc::CMD_SUBSCRIBE)).await.unwrap();
            let mut set = serde_json::Map::new();
            set.insert("start".into(), "gui".into());
            set.insert("no_autoplay".into(), false.into());
            let reply = c
                .request(Request {
                    cmd: ipc::CMD_OPTIONS.into(),
                    options: Some(set),
                    ..Default::default()
                })
                .await
                .unwrap();
            let got = Options::load();
            assert!(
                got.start == "gui" && !got.no_autoplay && got.cover == "smooth",
                "{got:?}"
            );
            assert_eq!(reply.options.as_ref(), Some(&got));
            let pushed = tokio::time::timeout(Duration::from_secs(1), events.recv())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(pushed.options, Some(got), "every player hears of it");
            let mut bad = serde_json::Map::new();
            bad.insert("viz_fps".into(), "fast".into());
            assert!(
                c.request(Request {
                    cmd: ipc::CMD_OPTIONS.into(),
                    options: Some(bad),
                    ..Default::default()
                })
                .await
                .is_err()
            );
            let r = c
                .request(Request {
                    cmd: ipc::CMD_RESOLVE.into(),
                    query: "https://example.com/x".into(),
                    ..Default::default()
                })
                .await;
            assert!(r.is_err(), "resolved a link elsewhere");
        });
    }
}

#[cfg(test)]
mod provider_tests;
