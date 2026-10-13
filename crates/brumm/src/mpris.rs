//! The daemon as an MPRIS media player, which gives brumm media keys,
//! playerctl, the Omarchy bar and the lock screen for free.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};
use std::time::Instant;

use sha2::{Digest, Sha256};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};
use zbus::{Connection, interface};

use crate::daemon::Daemon;
use crate::ipc::Player as PlayerState;

const BUS_NAME: &str = "org.mpris.MediaPlayer2.brumm";
const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const IFACE_PLAY: &str = "org.mpris.MediaPlayer2.Player";
const NO_TRACK: &str = "/org/brumm/track/none";

/// The last state published, and when its position was sampled.
struct Shared {
    state: PlayerState,
    at: Instant,
    last_id: String,
}

impl Shared {
    /// The playhead, extrapolated from the last sample.
    fn position(&self) -> f64 {
        let mut p = self.state.pos;
        if self.state.playing {
            p += self.at.elapsed().as_secs_f64();
        }
        if self.state.dur > 0.0 {
            p = p.min(self.state.dur);
        }
        p
    }
}

pub struct Server {
    conn: Connection,
    shared: Arc<Mutex<Shared>>,
}

/// Object paths allow only [A-Za-z0-9_]; whatever identifies the track
/// is hashed.
fn track_id(st: &PlayerState) -> String {
    if st.title.is_empty() {
        return NO_TRACK.to_string();
    }
    let sum =
        Sha256::digest(format!("{}\0{}\0{}\0{}", st.id, st.title, st.artist, st.index).as_bytes());
    let hex: String = sum[..8].iter().map(|b| format!("{b:02x}")).collect();
    format!("/org/brumm/track/t{hex}")
}

fn loop_status(repeat: i64) -> &'static str {
    ["None", "Track", "Playlist"][repeat.clamp(0, 2) as usize]
}

fn status(st: &PlayerState) -> &'static str {
    if st.playing {
        "Playing"
    } else if !st.title.is_empty() {
        "Paused"
    } else {
        "Stopped"
    }
}

fn owned(v: Value<'_>) -> OwnedValue {
    v.try_to_owned().unwrap_or_else(|_| OwnedValue::from(0u8))
}

/// Always every key, empty ones included, so no client keeps a stale
/// value from the previous track.
fn metadata(st: &PlayerState, id: &str) -> HashMap<String, OwnedValue> {
    let artists: Vec<String> = if st.artist.is_empty() {
        Vec::new()
    } else {
        vec![st.artist.clone()]
    };
    let path = ObjectPath::try_from(id.to_string())
        .unwrap_or_else(|_| ObjectPath::from_static_str_unchecked(NO_TRACK));
    HashMap::from([
        ("mpris:trackid".to_string(), owned(Value::ObjectPath(path))),
        (
            "mpris:length".to_string(),
            owned(Value::I64((st.dur * 1e6) as i64)),
        ),
        (
            "mpris:artUrl".to_string(),
            owned(Value::from(st.artwork.clone())),
        ),
        (
            "xesam:title".to_string(),
            owned(Value::from(st.title.clone())),
        ),
        ("xesam:artist".to_string(), owned(Value::from(artists))),
        (
            "xesam:album".to_string(),
            owned(Value::from(st.album.clone())),
        ),
    ])
}

struct Root {
    daemon: Weak<Daemon>,
}

#[interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    async fn raise(&self) {
        if let Some(d) = self.daemon.upgrade() {
            d.raise();
        }
    }
    fn quit(&self) {}
    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn can_raise(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn identity(&self) -> String {
        "brumm".into()
    }
    #[zbus(property)]
    fn desktop_entry(&self) -> String {
        "brumm".into()
    }
    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        Vec::new()
    }
    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
    }
}

struct Player {
    daemon: Weak<Daemon>,
    shared: Arc<Mutex<Shared>>,
}

impl Player {
    fn st(&self) -> PlayerState {
        self.shared.lock().unwrap().state.clone()
    }
}

#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    async fn play(&self) {
        if let Some(d) = self.daemon.upgrade() {
            d.play().await;
        }
    }
    async fn pause(&self) {
        if let Some(d) = self.daemon.upgrade() {
            d.pause().await;
        }
    }
    async fn play_pause(&self) {
        if let Some(d) = self.daemon.upgrade() {
            d.toggle().await;
        }
    }
    /// brumm has no unloaded state: pause at the start.
    async fn stop(&self) {
        if let Some(d) = self.daemon.upgrade() {
            d.pause().await;
            d.seek(0.0).await;
        }
    }
    async fn next(&self) {
        if let Some(d) = self.daemon.upgrade() {
            d.next().await;
        }
    }
    async fn previous(&self) {
        if let Some(d) = self.daemon.upgrade() {
            d.prev().await;
        }
    }
    async fn seek(&self, offset: i64) {
        let pos = self.shared.lock().unwrap().position();
        if let Some(d) = self.daemon.upgrade() {
            d.seek(pos + offset as f64 / 1e6).await;
        }
    }
    async fn set_position(&self, track_id: ObjectPath<'_>, position: i64) {
        let (current, dur) = {
            let s = self.shared.lock().unwrap();
            (s.last_id.clone(), s.state.dur)
        };
        if track_id.as_str() == current
            && position >= 0
            && position as f64 / 1e6 <= dur
            && let Some(d) = self.daemon.upgrade()
        {
            d.seek(position as f64 / 1e6).await;
        }
    }
    fn open_uri(&self, _uri: &str) {}

    #[zbus(signal)]
    async fn seeked(emitter: &SignalEmitter<'_>, position: i64) -> zbus::Result<()>;

    #[zbus(property)]
    fn playback_status(&self) -> String {
        status(&self.st()).into()
    }
    #[zbus(property)]
    fn loop_status(&self) -> String {
        loop_status(self.st().repeat).into()
    }
    #[zbus(property)]
    async fn set_loop_status(&self, v: String) {
        let mode = match v.as_str() {
            "Track" => 1,
            "Playlist" => 2,
            _ => 0,
        };
        if let Some(d) = self.daemon.upgrade() {
            d.set_repeat(mode).await;
        }
    }
    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn shuffle(&self) -> bool {
        self.st().shuffle
    }
    #[zbus(property)]
    async fn set_shuffle(&self, on: bool) {
        if let Some(d) = self.daemon.upgrade() {
            d.set_shuffle(on).await;
        }
    }
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        let s = self.shared.lock().unwrap();
        metadata(&s.state, &s.last_id)
    }
    #[zbus(property)]
    fn volume(&self) -> f64 {
        self.st().volume
    }
    #[zbus(property)]
    async fn set_volume(&self, v: f64) {
        if let Some(d) = self.daemon.upgrade() {
            d.set_volume(v.clamp(0.0, 1.0)).await;
        }
    }
    #[zbus(property(emits_changed_signal = "false"))]
    fn position(&self) -> i64 {
        (self.shared.lock().unwrap().position() * 1e6) as i64
    }
    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        let st = self.st();
        !st.title.is_empty() && st.index < st.length - 1
    }
    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        !self.st().title.is_empty()
    }
    #[zbus(property)]
    fn can_play(&self) -> bool {
        self.st().ready
    }
    #[zbus(property)]
    fn can_pause(&self) -> bool {
        !self.st().title.is_empty()
    }
    #[zbus(property)]
    fn can_seek(&self) -> bool {
        !self.st().title.is_empty()
    }
    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
}

impl Server {
    /// Claims the MPRIS name on the session bus for daemon.
    pub async fn start(daemon: Weak<Daemon>) -> zbus::Result<Server> {
        let shared = Arc::new(Mutex::new(Shared {
            state: PlayerState::default(),
            at: Instant::now(),
            last_id: NO_TRACK.into(),
        }));
        let conn = zbus::connection::Builder::session()?
            .name(BUS_NAME)?
            .serve_at(
                OBJECT_PATH,
                Root {
                    daemon: daemon.clone(),
                },
            )?
            .serve_at(
                OBJECT_PATH,
                Player {
                    daemon,
                    shared: shared.clone(),
                },
            )?
            .build()
            .await?;
        Ok(Server { conn, shared })
    }

    /// Publishes a new player state, signalling only what changed. A jump
    /// of more than two seconds against the extrapolated playhead is a
    /// seek.
    pub async fn update(&self, st: &PlayerState) {
        let (prev, expected, id, track_changed) = {
            let mut s = self.shared.lock().unwrap();
            let prev = s.state.clone();
            let expected = s.position();
            let id = track_id(st);
            let changed = id != s.last_id;
            s.state = st.clone();
            s.at = Instant::now();
            s.last_id = id.clone();
            (prev, expected, id, changed)
        };
        let mut changed: HashMap<&str, Value<'_>> = HashMap::new();
        let has = !st.title.is_empty();
        if track_changed || prev.dur != st.dur || prev.artwork != st.artwork {
            let md: HashMap<String, Value<'_>> = metadata(st, &id)
                .into_iter()
                .map(|(k, v)| (k, Value::from(v)))
                .collect();
            changed.insert("Metadata", Value::from(md));
            changed.insert("CanGoNext", Value::from(has && st.index < st.length - 1));
            changed.insert("CanGoPrevious", Value::from(has));
            changed.insert("CanPause", Value::from(has));
            changed.insert("CanSeek", Value::from(has));
        }
        if status(&prev) != status(st) {
            changed.insert("PlaybackStatus", Value::from(status(st)));
        }
        if prev.volume != st.volume {
            changed.insert("Volume", Value::from(st.volume));
        }
        if prev.shuffle != st.shuffle {
            changed.insert("Shuffle", Value::from(st.shuffle));
        }
        if prev.repeat != st.repeat {
            changed.insert("LoopStatus", Value::from(loop_status(st.repeat)));
        }
        if prev.ready != st.ready {
            changed.insert("CanPlay", Value::from(st.ready));
        }
        let Ok(emitter) = SignalEmitter::new(&self.conn, OBJECT_PATH) else {
            return;
        };
        if !changed.is_empty() {
            let iface = zbus::names::InterfaceName::from_static_str_unchecked(IFACE_PLAY);
            let _ = zbus::fdo::Properties::properties_changed(
                &emitter,
                iface,
                changed,
                Cow::Borrowed(&[]),
            )
            .await;
        }
        if !track_changed && has && (st.pos - expected).abs() > 2.0 {
            let _ = Player::seeked(&emitter, (st.pos * 1e6) as i64).await;
        }
    }

    pub async fn close(&self) {
        let _ = self.conn.release_name(BUS_NAME).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_ids_are_valid_object_paths() {
        let st = PlayerState {
            title: "Song / with ünïcode".into(),
            id: "i.x".into(),
            ..Default::default()
        };
        let id = track_id(&st);
        assert!(ObjectPath::try_from(id.as_str()).is_ok(), "{id}");
        assert_eq!(track_id(&PlayerState::default()), NO_TRACK);
        let md = metadata(&st, &id);
        assert_eq!(md.len(), 6, "every key, every time");
        assert_eq!(loop_status(7), "Playlist");
        assert_eq!(status(&st), "Paused");
    }
}
