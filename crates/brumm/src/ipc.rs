//! The protocol between the brumm daemon and its players: newline-delimited
//! JSON over a unix socket. Players send Requests; the daemon answers each
//! with a Message carrying the same id, and pushes state and spectrum
//! Messages to connections that subscribed. The window (gui/src/daemon.cpp)
//! and the stand-in daemon (tools/fakedaemon) speak it too.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::net::unix::OwnedWriteHalf;
use tokio::sync::{mpsc, oneshot};

use crate::apple::{ExplorePage, Item, Ref, Shelf, Track};
use crate::config::{self, Options};
use crate::error::{Error, Result, err};

// Commands.
/// Add ids (of kind `list`: songs, albums, playlists) to the library.
pub const CMD_ADD: &str = "add";
/// Push state (and spectrum when bands > 0).
pub const CMD_SUBSCRIBE: &str = "subscribe";
/// `list`: one of the lists below → items or tracks.
pub const CMD_LIST: &str = "list";
/// `item` → tracks (playlist, album) or shelves (artist, charts).
pub const CMD_OPEN: &str = "open";
/// `query` → shelves.
pub const CMD_SEARCH: &str = "search";
/// → shelves: recently played, radio, recommendations…
pub const CMD_HOME: &str = "home";
/// `refs`: submit typed resources to Apple Music favorites.
pub const CMD_FAVORITE: &str = "favorite";
/// `list`: folder or playlist; `query`: name; `start`: parent folder.
pub const CMD_CREATE: &str = "create";
/// `query`: relative public API route → explorer page.
pub const CMD_EXPLORE: &str = "explore";
/// `item` (a station, or an artist) or `start` (a song): play its station.
pub const CMD_STATION: &str = "station";
/// `ids` to `start` (a playlist id), or to a new playlist named `query` → items.
pub const CMD_PLAYLIST: &str = "playlist";
/// `value` 1: similar music plays on when the queue ends.
pub const CMD_AUTOPLAY: &str = "autoplay";
/// `ids`: songs likely to be played next; the player looks them up.
pub const CMD_WARM: &str = "warm";
/// `ids`, `start`, `source`.
pub const CMD_PLAY: &str = "play";
/// `start`: a song id → items: its album.
pub const CMD_ALBUM: &str = "album";
/// `start`: a song id; `value` 0 stops the preview.
pub const CMD_PREVIEW: &str = "preview";
/// `value`: how many → tracks from the current song, pos.
pub const CMD_QUEUE: &str = "queue";
/// `value`: queue index to play.
pub const CMD_JUMP: &str = "jump";
/// `ids`; `value` 1 plays them next, 0 at the end.
pub const CMD_ENQUEUE: &str = "enqueue";
/// `refs` → ratings: those loved (1) or disliked (-1).
pub const CMD_RATINGS: &str = "ratings";
/// `refs[0]`; `value` 1 love, -1 dislike, 0 neither.
pub const CMD_RATE: &str = "rate";
/// `start`: a song id → items: its artist.
pub const CMD_ARTIST: &str = "artist";
/// `start`: a song id → link: its music.apple.com address.
pub const CMD_LINK: &str = "link";
/// `value` 1 installs the available update; 0 checks for one; 2 checks now
/// and answers; 3 restarts into an installed one.
pub const CMD_UPDATE: &str = "update";
pub const CMD_TOGGLE: &str = "toggle";
pub const CMD_NEXT: &str = "next";
pub const CMD_PREV: &str = "prev";
/// `value`: absolute seconds.
pub const CMD_SEEK: &str = "seek";
/// `value`: 0–1.
pub const CMD_VOLUME: &str = "volume";
/// `value`: 0 or 1.
pub const CMD_SHUFFLE: &str = "shuffle";
/// `value`: 0 off, 1 one, 2 all.
pub const CMD_REPEAT: &str = "repeat";
/// Re-read credentials after a login.
pub const CMD_RELOAD: &str = "reload";
/// `options` set (a partial set of Options fields) → options; pushed to
/// subscribers when they change.
pub const CMD_OPTIONS: &str = "options";
/// Open the Apple Music sign-in in the browser; the player reloads once
/// signed in.
pub const CMD_LOGIN: &str = "login";
/// `query` "tui", "gui" or "" (as the options say): open that player, or
/// bring it to the front.
pub const CMD_SHOW: &str = "show";
/// `query`: a music.apple.com link → items: what it names, ids: the song it
/// points at.
pub const CMD_RESOLVE: &str = "resolve";
/// `query`: what the user wrote, `source`: the face (tui, gui), `value` 1:
/// hand it to Omarchy's agent → link: "agent" or "browser", how it went.
pub const CMD_FEEDBACK: &str = "feedback";
/// `place` set: where a player is (section, open views, selection); none →
/// place: where the last one was.
pub const CMD_PLACE: &str = "place";
/// Stop the daemon.
pub const CMD_QUIT: &str = "quit";

// Library lists.
pub const LIST_PLAYLISTS: &str = "playlists";
pub const LIST_ALBUMS: &str = "albums";
pub const LIST_ARTISTS: &str = "artists";
pub const LIST_SONGS: &str = "songs";

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_zero_i(i: &i64) -> bool {
    *i == 0
}

fn is_zero_f(f: &f64) -> bool {
    *f == 0.0
}

/// A present `place`, null included: null clears the place kept.
fn some_value<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Option<Value>, D::Error> {
    Value::deserialize(d).map(Some)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Request {
    pub id: i64,
    pub cmd: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub list: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<Item>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub query: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ids: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub start: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub source: String,
    #[serde(skip_serializing_if = "is_zero_f")]
    pub value: f64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<Ref>,
    #[serde(skip_serializing_if = "is_zero_i")]
    pub bands: i64,
    /// Waveform samples wanted per frame.
    #[serde(skip_serializing_if = "is_zero_i")]
    pub wave: i64,
    /// Spectrum frames per second wanted; 25 by default.
    #[serde(skip_serializing_if = "is_zero_i")]
    pub fps: i64,
    /// Changes the named options, by their JSON names; the rest stay.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Map<String, Value>>,
    /// Where a player is, for the other one to open there: kept by the
    /// daemon as it comes, never read by it.
    #[serde(
        skip_serializing_if = "Option::is_none",
        deserialize_with = "some_value"
    )]
    pub place: Option<Value>,
}

impl Request {
    pub fn new(cmd: &str) -> Request {
        Request {
            cmd: cmd.to_string(),
            ..Default::default()
        }
    }
}

/// The daemon's lifecycle phase.
pub const STATUS_STARTING: &str = "starting";
pub const STATUS_READY: &str = "ready";
pub const STATUS_LOGGED_OUT: &str = "logged-out";
pub const STATUS_ERROR: &str = "error";

/// A song being previewed.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Clip {
    pub title: String,
    pub artist: String,
    pub artwork: String,
}

/// The player's state, as the playback page reports it (brumm.state() in
/// player.html).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Player {
    pub ready: bool,
    pub err: String,
    #[serde(rename = "needsAuth")]
    pub needs_auth: bool,
    pub playing: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub artwork: String,
    pub pos: f64,
    pub dur: f64,
    pub index: i64,
    pub length: i64,
    /// The playing song.
    pub id: String,
    /// Key of the list it was started from.
    pub source: String,
    pub volume: f64,
    pub shuffle: bool,
    /// 0 off, 1 one, 2 all.
    pub repeat: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<Clip>,
}

/// What the daemon tells players about itself and the player.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct State {
    pub status: String,
    /// Progress or error text.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub message: String,
    /// Set when this build's Apple Music access runs out soon (days left);
    /// an update brings a fresh one.
    #[serde(rename = "expiresIn", skip_serializing_if = "is_zero_i")]
    pub expires_in: i64,
    /// Names a newer release when one is available.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub update: String,
    #[serde(flatten)]
    pub player: Player,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Message {
    #[serde(skip_serializing_if = "is_zero_i")]
    pub id: i64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<Item>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tracks: Vec<Track>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub shelves: Vec<Shelf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explorer: Option<ExplorePage>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub ratings: BTreeMap<String, i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<State>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub spectrum: Vec<i64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub wave: Vec<i64>,
    /// The cached library changed.
    #[serde(skip_serializing_if = "is_false")]
    pub library: bool,
    #[serde(skip_serializing_if = "is_zero_i")]
    pub pos: i64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ids: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub link: String,
    /// The daemon's own, with an update check.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub version: String,
    /// With an update check: what the newest release says is new.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// The saved options, answering or announcing a change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Options>,
    /// With the options: omarchy is here, the bar widget can be switched.
    #[serde(skip_serializing_if = "is_false")]
    pub omarchy: bool,
    /// With the options: Omarchy has a coding agent to hand feedback to.
    #[serde(skip_serializing_if = "is_false")]
    pub agent: bool,
    /// Where a player last was (CMD_PLACE).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place: Option<Value>,
}

/// Encodes one message as a line.
pub fn encode<T: Serialize>(m: &T) -> Vec<u8> {
    let mut b = serde_json::to_vec(m).unwrap_or_else(|_| b"{}".to_vec());
    b.push(b'\n');
    b
}

/// Lines longer than this are dropped, not buffered without end.
pub const MAX_LINE: usize = 16 << 20;

/// Reads the next line of at most `max` bytes into buf (without the
/// newline). Longer lines are skipped whole. False at the end.
pub async fn read_line<R: tokio::io::AsyncBufRead + Unpin>(
    r: &mut R,
    buf: &mut Vec<u8>,
    max: usize,
) -> std::io::Result<bool> {
    buf.clear();
    let mut too_long = false;
    loop {
        let chunk = r.fill_buf().await?;
        if chunk.is_empty() {
            return Ok(!buf.is_empty() && !too_long);
        }
        match chunk.iter().position(|&b| b == b'\n') {
            Some(i) => {
                let fits = !too_long && buf.len() + i <= max;
                if fits {
                    buf.extend_from_slice(&chunk[..i]);
                }
                r.consume(i + 1);
                if fits {
                    return Ok(true);
                }
                buf.clear();
                too_long = false;
            }
            None => {
                let n = chunk.len();
                if !too_long && buf.len() + n <= max {
                    buf.extend_from_slice(chunk);
                } else {
                    too_long = true;
                    buf.clear();
                }
                r.consume(n);
            }
        }
    }
}

type Pending = Arc<Mutex<Option<HashMap<i64, oneshot::Sender<Message>>>>>;

/// One connection to the daemon. Clones share it.
#[derive(Clone)]
pub struct Client {
    writer: Arc<tokio::sync::Mutex<OwnedWriteHalf>>,
    next: Arc<AtomicI64>,
    pending: Pending,
}

impl Client {
    /// Connects to a running daemon. The receiver delivers pushed state and
    /// spectrum; it ends when the daemon goes away.
    pub async fn dial() -> Result<(Client, mpsc::Receiver<Message>)> {
        let conn = tokio::time::timeout(
            Duration::from_secs(1),
            UnixStream::connect(config::socket()),
        )
        .await
        .map_err(|_| err("connecting to the brumm daemon timed out"))??;
        Ok(Client::new(conn))
    }

    /// A client over conn.
    pub fn new(conn: UnixStream) -> (Client, mpsc::Receiver<Message>) {
        let (r, w) = conn.into_split();
        let pending: Pending = Arc::new(Mutex::new(Some(HashMap::new())));
        let (tx, rx) = mpsc::channel(256);
        let p = pending.clone();
        tokio::spawn(async move {
            let mut r = BufReader::with_capacity(64 << 10, r);
            let mut line = Vec::new();
            while let Ok(true) = read_line(&mut r, &mut line, MAX_LINE).await {
                let Ok(m) = serde_json::from_slice::<Message>(&line) else {
                    continue;
                };
                if m.id != 0 {
                    let ch = p.lock().unwrap().as_mut().and_then(|w| w.remove(&m.id));
                    if let Some(ch) = ch {
                        let _ = ch.send(m);
                    }
                    continue;
                }
                // Never wait here: replies share this reader. A client that
                // falls this far behind loses frames, and state catches up
                // with the next.
                let _ = tx.try_send(m);
            }
            // Whoever still waits hears the daemon is gone.
            p.lock().unwrap().take();
        });
        (
            Client {
                writer: Arc::new(tokio::sync::Mutex::new(w)),
                next: Arc::new(AtomicI64::new(0)),
                pending,
            },
            rx,
        )
    }

    /// Sends a request and waits for its answer; an answer carrying an
    /// error is that error.
    pub async fn request(&self, mut r: Request) -> Result<Message> {
        r.id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let (tx, rx) = oneshot::channel();
        match self.pending.lock().unwrap().as_mut() {
            Some(w) => w.insert(r.id, tx),
            None => return Err(err("daemon disconnected")),
        };
        let line = encode(&r);
        if let Err(e) = self.writer.lock().await.write_all(&line).await {
            if let Some(w) = self.pending.lock().unwrap().as_mut() {
                w.remove(&r.id);
            }
            return Err(e.into());
        }
        let m = rx.await.map_err(|_| err("daemon disconnected"))?;
        if !m.error.is_empty() {
            return Err(Error::Msg(m.error));
        }
        Ok(m)
    }

    /// Ends the connection.
    pub async fn close(&self) {
        let _ = self.writer.lock().await.shutdown().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_format_matches_the_go_daemon() {
        // Empty fields stay out, as Go's omitempty left them out; the
        // window reads "options", "state", "spectrum" by their presence.
        let m = Message {
            id: 3,
            ..Default::default()
        };
        assert_eq!(String::from_utf8(encode(&m)).unwrap(), "{\"id\":3}\n");
        let st = State {
            status: STATUS_READY.into(),
            player: Player {
                title: "Song".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let v: Value = serde_json::to_value(&st).unwrap();
        assert_eq!(v["status"], "ready");
        assert_eq!(v["title"], "Song");
        assert_eq!(v["needsAuth"], false);
        assert!(
            v.get("preview").is_none()
                && v.get("message").is_none()
                && v.get("expiresIn").is_none()
        );
        // A null place is kept: it clears the place.
        let r: Request = serde_json::from_str(r#"{"id":1,"cmd":"place","place":null}"#).unwrap();
        assert_eq!(r.place, Some(Value::Null));
        let r: Request = serde_json::from_str(r#"{"id":1,"cmd":"place"}"#).unwrap();
        assert_eq!(r.place, None);
        // Players send their requests as the Go client did.
        let r = Request {
            cmd: CMD_SUBSCRIBE.into(),
            bands: 48,
            fps: 10,
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"id":0,"cmd":"subscribe","bands":48,"fps":10}"#
        );
    }

    #[tokio::test]
    async fn framing_skips_overlong_and_garbled_lines() {
        let data = format!("{}\n{{bad\n{{\"id\":1}}\n", "x".repeat(100));
        let mut r = BufReader::with_capacity(8, data.as_bytes());
        let mut line = Vec::new();
        assert!(read_line(&mut r, &mut line, 64).await.unwrap());
        assert_eq!(line, b"{bad", "the overlong line was skipped whole");
        assert!(read_line(&mut r, &mut line, 64).await.unwrap());
        assert_eq!(line, b"{\"id\":1}");
        assert!(!read_line(&mut r, &mut line, 64).await.unwrap());
    }

    #[tokio::test]
    async fn concurrent_requests_get_their_own_answers() {
        let (a, b) = UnixStream::pair().unwrap();
        // A server answering out of order, with pushes in between.
        tokio::spawn(async move {
            let (r, mut w) = b.into_split();
            let mut r = BufReader::new(r);
            let mut line = Vec::new();
            let mut got = Vec::new();
            while read_line(&mut r, &mut line, MAX_LINE).await.unwrap() {
                let req: Request = serde_json::from_slice(&line).unwrap();
                got.push(req);
                if got.len() == 3 {
                    for req in got.iter().rev() {
                        w.write_all(&encode(&Message {
                            library: true,
                            ..Default::default()
                        }))
                        .await
                        .unwrap();
                        let reply = Message {
                            id: req.id,
                            link: req.query.clone(),
                            ..Default::default()
                        };
                        w.write_all(&encode(&reply)).await.unwrap();
                    }
                    let fail = Message {
                        id: 99,
                        error: "nope".into(),
                        ..Default::default()
                    };
                    w.write_all(&encode(&fail)).await.unwrap();
                }
            }
        });
        let (c, mut events) = Client::new(a);
        let ask = |q: &str| {
            let c = c.clone();
            let mut r = Request::new(CMD_LINK);
            r.query = q.to_string();
            tokio::spawn(async move { c.request(r).await })
        };
        let (x, y, z) = (ask("x"), ask("y"), ask("z"));
        assert_eq!(x.await.unwrap().unwrap().link, "x");
        assert_eq!(y.await.unwrap().unwrap().link, "y");
        assert_eq!(z.await.unwrap().unwrap().link, "z");
        assert!(events.recv().await.unwrap().library);
        c.close().await;
        // After the server went, requests fail rather than hang.
        let late = tokio::time::timeout(Duration::from_secs(2), c.request(Request::new(CMD_QUEUE)))
            .await
            .unwrap();
        assert!(late.is_err());
    }
}
