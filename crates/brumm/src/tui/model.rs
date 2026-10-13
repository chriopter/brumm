//! The terminal player's state and what every message does to it: a
//! drill-in browser on the left (library sections, then playlists,
//! albums, artists, songs and search results) and a stage on the right
//! with the cover, a live spectrum, the playhead and the transport.

use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use image::RgbaImage;
use serde_json::Value;

use super::accent::Accent;
use super::cover::KittyImage;
use super::features::FeatureForm;
use super::feedback::Feedback;
use super::geometry::Geometry;
use super::options::{Opts, auto_fps};
use super::picker::Picker;
use super::text::L;
use super::update::UpdatePopup;
use super::upnext::{NextKey, NextList, NextSong, NextStrip, NextUp};
use super::viz::{self, Visualizer};
use crate::apple::{self, Item, KIND_STATION, KIND_TERM, KIND_VIDEO, Ref, Shelf, Track};
use crate::art::Size;
use crate::ipc::{self, Client, Message, Request, State};
use crate::launch;

/// How many spectrum bands the player asks the daemon for, and at what
/// rate: the small meters need less than the fullscreen visualizer.
pub const BANDS: i64 = 48;
/// The playing row's equalizer, drawn at most this often (EQ_EVERY).
pub const METER_FPS: i64 = 10;
/// Spectrum frames for the fullscreen visualizer; it draws faster.
pub const VIZ_FPS: i64 = 30;
/// The rate the meters' fall was tuned at.
const FALL_REF: f64 = 25.0;

/// A play queues the chosen song and at most this many in all.
const QUEUE_AFTER: usize = 500;

pub const FRAME_EVERY: Duration = Duration::from_millis(100);

/// Holding space this long previews the selected song instead of pausing.
const HOLD_DELAY: Duration = Duration::from_millis(280);

// Sections.
pub const SEC_HOME: usize = 0;
pub const SEC_EXPLORE: usize = 1;
pub const SEC_PLAYLISTS: usize = 2;
pub const SEC_ALBUMS: usize = 3;
pub const SEC_ARTISTS: usize = 4;
pub const SEC_SONGS: usize = 5;
pub const SEC_RADIO: usize = 6;
pub const SEC_SEARCH: usize = 7;
pub const SEC_QUEUE: usize = 8;
pub const NUM_SECTIONS: usize = 9;

pub const SECTION_NAMES: [&str; NUM_SECTIONS] = [
    "Home",
    "Explore",
    "Playlists",
    "Albums",
    "Artists",
    "Songs",
    "Radio",
    "Search",
    "Queue",
];
const SECTION_LISTS: [&str; 4] = [
    ipc::LIST_PLAYLISTS,
    ipc::LIST_ALBUMS,
    ipc::LIST_ARTISTS,
    ipc::LIST_SONGS,
];

/// One line of a list: a song, or something that opens; or a shelf's
/// title, or a line of text about the view.
#[derive(Clone, Debug, Default)]
pub struct Row {
    pub track: Option<Arc<Track>>,
    pub item: Option<Arc<Item>>,
    pub head: String,
    pub note: String,
}

impl Row {
    /// Songs and things that open or play; headings and notes are only read.
    pub fn selectable(&self) -> bool {
        self.track.is_some() || self.item.is_some()
    }

    pub fn track(t: Track) -> Row {
        Row {
            track: Some(Arc::new(t)),
            ..Default::default()
        }
    }

    pub fn item(it: Item) -> Row {
        Row {
            item: Some(Arc::new(it)),
            ..Default::default()
        }
    }

    pub fn head(s: impl Into<String>) -> Row {
        Row {
            head: s.into(),
            ..Default::default()
        }
    }

    pub fn note(s: impl Into<String>) -> Row {
        Row {
            note: s.into(),
            ..Default::default()
        }
    }
}

/// Explore's first links arrive before the artwork requests; each group is
/// a request of its own, so one slow chart never blocks another.
#[derive(Clone, Debug, Default)]
pub struct ExploreLoad {
    pub id: u64,
    pub page: apple::ExplorePage,
    pub groups: [Vec<Shelf>; 3],
}

/// One level of the browser.
#[derive(Clone, Debug, Default)]
pub struct View {
    pub id: u64,
    pub title: String,
    /// What a play started here is attributed to.
    pub key: String,
    /// The item this view shows the inside of.
    pub item: Option<Item>,
    pub rows: Vec<Row>,
    pub loaded: bool,
    pub loading: bool,
    pub err: Option<String>,
    pub sel: usize,
    pub off: usize,
    /// Frame the selection last changed, for the marquee.
    pub sel_at: i64,
    /// For the queue view: the queue index of the first row.
    pub qpos: i64,
    pub filter: String,
    /// Every row while a filter shows some.
    pub all: Option<Vec<Row>>,
    /// A song to select once the rows come.
    pub want: String,
    pub explore: Option<ExploreLoad>,
}

impl View {
    /// The view's song ids, and each song row's position among them.
    pub fn songs(&self) -> (Vec<String>, HashMap<usize, usize>) {
        let mut ids = Vec::new();
        let mut at = HashMap::new();
        for (i, r) in self.rows.iter().enumerate() {
            if let Some(t) = &r.track {
                at.insert(i, ids.len());
                ids.push(t.id.clone());
            }
        }
        (ids, at)
    }
}

/// A key as the player names it ("enter", "shift+tab", "a") and the text
/// it types.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Key {
    pub name: String,
    pub text: String,
    pub repeat: bool,
    pub release: bool,
}

impl Key {
    pub fn named(name: &str) -> Key {
        let text = if name.chars().count() == 1 {
            name.to_string()
        } else if name == "space" {
            " ".into()
        } else {
            String::new()
        };
        Key {
            name: name.to_string(),
            text,
            ..Default::default()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseKind {
    Down,
    Up,
    /// Moved; with a button down, a drag.
    Moved,
    WheelUp,
    WheelDown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mouse {
    pub kind: MouseKind,
    pub button: Button,
    pub x: i64,
    pub y: i64,
}

pub type Task = Pin<Box<dyn Future<Output = Option<Msg>> + Send>>;

/// Work the loop runs for the model.
pub enum Cmd {
    /// Runs on the runtime; its message comes back.
    Task(Task),
    /// Runs on a thread of its own (drawing a cover).
    Blocking(Box<dyn FnOnce() -> Option<Msg> + Send>),
    /// The message, after a while.
    After(Duration, Box<Msg>),
    /// Bytes for the terminal, before the next frame (kitty images, the
    /// pointer shape).
    Raw(String),
    Quit,
}

pub enum Msg {
    Key(Key),
    Mouse(Mouse),
    Resize(u16, u16),
    Focus(bool),
    Paste(String),
    /// What the terminal said about itself at the start.
    Probe {
        bg: Option<[u8; 3]>,
        releases: bool,
    },
    Event(Message),
    /// The daemon went away.
    Closed,
    Connected(Option<(Client, State)>),
    Loaded {
        view: u64,
        reply: Result<Message, String>,
        select: String,
    },
    Cover {
        url: String,
        img: Option<Arc<RgbaImage>>,
    },
    Thumb {
        key: String,
        lines: Vec<L>,
    },
    Album(Result<(Item, String), String>),
    Login(Result<(), String>),
    Err(String),
    Tick(u64),
    Wheel,
    Refresh(i64),
    Hold(u64),
    Seek(u64),
    Typed(u64),
    Flash(String),
    Open(Item, String),
    Next {
        seq: u64,
        tracks: Vec<Track>,
        pos: i64,
        ok: bool,
    },
    Ratings(BTreeMap<String, i64>),
    PickerLists(Result<Vec<Item>, String>),
    UpdateChecked(Result<Message, String>),
    Installed(Result<(), String>),
    Bar(Option<bool>),
    BarSet {
        on: bool,
        err: Option<String>,
    },
    Feedback(Result<String, String>),
    FeatureSaved,
    ExploreGroup {
        view: u64,
        load: u64,
        group: usize,
        shelves: Vec<Shelf>,
    },
    Place(Option<Value>),
    BinaryCheck,
    Kitty {
        key: String,
        seq: Option<String>,
    },
    Quit,
}

/// A song started and shown, not yet reported playing.
pub struct PendingPlay {
    pub track: Track,
    pub source: String,
    /// Its place in the queue; -1 when not known.
    pub index: i64,
    pub at: Instant,
}

/// A pause or play asked for, shown before the player reports it.
pub struct WantPlaying {
    pub playing: bool,
    pub at: Instant,
}

/// A card the screen wanted but did not have drawn yet.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThumbReq {
    pub key: String,
    pub url: String,
    pub size: Size,
}

/// A cover the screen wanted from the terminal as a kitty image.
#[derive(Clone, Debug, PartialEq)]
pub struct KittyReq {
    pub key: String,
    pub url: String,
    pub size: Size,
}

pub struct Model {
    pub client: Option<Client>,
    pub http: reqwest::Client,
    /// The channel the loop listens on, for a new connection's events.
    pub tx: Option<std::sync::mpsc::Sender<Msg>>,
    pub cmds: Vec<Cmd>,

    pub width: i64,
    pub height: i64,
    /// Cell height ÷ width, measured in pixels.
    pub cell_aspect: f64,
    /// Animation clock in 100 ms steps.
    pub frame: i64,
    pub start: Instant,
    /// When the next frame of the screen's own comes, and its number.
    pub tick_at: Option<Instant>,
    pub tick_seq: u64,
    /// The terminal lost focus: stop what only a viewer would see.
    pub blurred: bool,
    pub player_view: bool,
    /// The last render drew the playing row's equalizer.
    pub eq_shown: bool,
    /// The last render laid a card on the cover.
    pub card_shown: bool,
    /// The last render scrolled a selected row.
    pub marquee: bool,
    /// The rate the spectrum arrives at, and whether it streams.
    pub spec_fps: i64,
    pub spec_on: bool,
    /// Nothing on screen changed with this message: no frame needed.
    pub still: bool,
    /// Whether anything was drawn yet.
    pub drawn: bool,

    pub state: State,
    pub state_at: Instant,
    pub spec: Vec<f64>,

    pub section: usize,
    pub stacks: [Vec<View>; NUM_SECTIONS],
    next_view: u64,

    /// The search box has focus.
    pub searching: bool,
    pub query: String,
    pub typed_seq: u64,

    /// Loves (1) and dislikes (-1) seen so far, by id.
    pub rating: HashMap<String, i64>,

    pub pending: Option<PendingPlay>,
    pub want_play: Option<WantPlaying>,

    /// The size cards were last drawn at, for drawing ahead.
    pub card_size: Size,
    /// The screen's refresh rate: the visualizer's auto rate.
    pub refresh: i64,

    pub upd: Option<UpdatePopup>,
    pub fb: Option<Feedback>,

    pub seek_to: f64,
    pub seek_at: Option<Instant>,
    pub seek_seq: u64,

    pub cover_url: String,
    pub cover: Option<Arc<RgbaImage>>,
    pub cover_dim: Option<Arc<RgbaImage>>,
    pub rendered: HashMap<(Size, bool), Vec<L>>,
    pub covers: HashMap<String, Arc<RgbaImage>>,
    pub cover_lru: Vec<String>,
    pub fetching: HashMap<String, bool>,
    pub thumbs: HashMap<String, Vec<L>>,
    pub thumb_want: ThumbReq,
    pub thumb_busy: bool,

    pub flash: String,
    pub flash_at: Instant,
    pub last_err: String,
    pub help: bool,
    pub resumed: bool,
    /// Asking for a place the window left: no resuming till it answers.
    pub placing: bool,
    pub last_vol: f64,
    pub geo: Geometry,

    /// The terminal reports key releases.
    pub releases: bool,
    pub space_down: bool,
    pub space_seq: u64,
    pub previewing: bool,

    /// The list's share of the width.
    pub split: f64,
    pub dragging: bool,
    pub drag_off: i64,
    /// The binary was updated: restart into it on quit.
    pub reexec: bool,
    /// The mouse pointer's shape as last sent.
    pub pointer: String,

    pub opts: Opts,
    /// The options as last sent to the daemon.
    pub sent_opts: Option<serde_json::Map<String, Value>>,
    pub opt_queue: Option<tokio::sync::mpsc::Sender<serde_json::Map<String, Value>>>,

    /// omarchy is here: the bar widget can be switched.
    pub has_omarchy: bool,
    pub bar_on: bool,
    /// The first start's question about the bar widget is due.
    pub bar_ask: bool,

    /// The terminal's background, once known: accents keep clear of it.
    pub term_bg: Option<[u8; 3]>,
    pub acc: Accent,

    pub next: Vec<NextUp>,
    pub up_tracks: Vec<Track>,
    pub up_pos: i64,
    pub next_asked: NextKey,
    pub next_seq: u64,
    pub next_gen: u64,
    pub next_draw: NextStrip,
    pub songs: Vec<NextSong>,
    pub ahead: i64,
    pub song_draw: NextList,

    pub filtering: bool,
    pub opt_open: bool,
    pub feature_form: Option<FeatureForm>,
    pub pick: Option<Picker>,
    pub opt_lines: Vec<i64>,
    pub opt_sel: usize,

    /// Covers sent to the terminal as kitty images, by address and size.
    pub kitty: HashMap<String, KittyImage>,
    pub kitty_want: Vec<KittyReq>,
    pub kitty_busy: bool,
    pub kitty_next: usize,
    pub kitty_clock: u64,
    /// The terminal shows kitty images (guessed at start).
    pub has_kitty: bool,

    /// The sign-in page opened on its own once.
    pub login_tried: bool,

    // A spinning mouse wheel.
    pub wheel_at: Option<Instant>,
    pub wheel_drawn: Option<Instant>,
    pub wheel_moved: bool,
    pub wheel_tick: bool,

    /// When the equalizer bars last moved, and how they stood.
    pub eq_bars: String,
    pub eq_at: Option<Instant>,

    /// Fullscreen visualizer.
    pub full: bool,
    /// The list of styles shows over it.
    pub viz_list: bool,
    pub viz_sel: usize,
    /// The style before the list opened: esc goes back to it.
    pub viz_from: usize,
    pub viz_style: usize,
    /// The style fading out.
    pub viz_prev: Option<usize>,
    pub viz_at: Instant,
    pub viz: Visualizer,
    pub viz_spec: Vec<f64>,
    pub wave: Vec<f64>,
}

/// Fullscreen opens on the showpiece and moves on every minute, dissolving
/// from one style into the next.
pub const VIZ_OPENING: usize = 0; // milkdrop
pub const VIZ_EVERY: Duration = Duration::from_secs(60);
pub const VIZ_FADE: Duration = Duration::from_millis(1500);

/// A free-spinning wheel sends hundreds of notches a second. A notch only
/// moves the selection; the screen catches up at most once per WHEEL_EVERY,
/// and covers load once the wheel rests for WHEEL_REST.
const WHEEL_EVERY: Duration = Duration::from_millis(25);
const WHEEL_REST: Duration = Duration::from_millis(150);

/// Where the program's own setting lives: the split of the list.
fn split_path() -> std::path::PathBuf {
    crate::config::cache_dir().join("split")
}

/// The list's remembered share of the width, half by default.
pub fn load_split() -> f64 {
    std::fs::read_to_string(split_path())
        .ok()
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|f| (0.1..=0.95).contains(f))
        .unwrap_or(0.5)
}

pub fn save_split(f: f64) {
    let _ = std::fs::create_dir_all(crate::config::cache_dir());
    let _ = crate::config::write_private(&split_path(), format!("{f:.3}").as_bytes());
}

/// Rows sample: up to n ids in random order, leaving out skip.
pub fn sample(ids: &[String], skip: &str, n: usize) -> Vec<String> {
    let mut out: Vec<String> = ids.iter().filter(|id| *id != skip).cloned().collect();
    // Fisher–Yates with the kernel's randomness.
    for i in (1..out.len()).rev() {
        let mut b = [0u8; 8];
        let _ = getrandom::fill(&mut b);
        let j = (u64::from_le_bytes(b) % (i as u64 + 1)) as usize;
        out.swap(i, j);
    }
    out.truncate(n);
    out
}

impl Model {
    pub fn new(client: Option<Client>, initial: State) -> Model {
        let opts = Opts::load();
        let has_omarchy = crate::update::has_omarchy();
        let mut m = Model {
            client,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            tx: None,
            cmds: Vec::new(),
            width: 0,
            height: 0,
            cell_aspect: 2.0,
            frame: 0,
            start: Instant::now(),
            tick_at: None,
            tick_seq: 0,
            blurred: false,
            player_view: false,
            eq_shown: false,
            card_shown: false,
            marquee: false,
            spec_fps: METER_FPS,
            spec_on: true, // as run subscribed
            still: false,
            drawn: false,
            state: initial,
            state_at: Instant::now(),
            spec: Vec::new(),
            section: SEC_HOME,
            stacks: Default::default(),
            next_view: 0,
            searching: false,
            query: String::new(),
            typed_seq: 0,
            rating: HashMap::new(),
            pending: None,
            want_play: None,
            card_size: Size::default(),
            refresh: 60,
            upd: None,
            fb: None,
            seek_to: 0.0,
            seek_at: None,
            seek_seq: 0,
            cover_url: String::new(),
            cover: None,
            cover_dim: None,
            rendered: HashMap::new(),
            covers: HashMap::new(),
            cover_lru: Vec::new(),
            fetching: HashMap::new(),
            thumbs: HashMap::new(),
            thumb_want: ThumbReq::default(),
            thumb_busy: false,
            flash: String::new(),
            flash_at: Instant::now(),
            last_err: String::new(),
            help: false,
            resumed: false,
            placing: false,
            last_vol: 1.0,
            geo: Geometry::default(),
            releases: false,
            space_down: false,
            space_seq: 0,
            previewing: false,
            split: load_split(),
            dragging: false,
            drag_off: 0,
            reexec: false,
            pointer: String::new(),
            bar_on: has_omarchy && opts.bar_known(),
            opts,
            sent_opts: None,
            opt_queue: None,
            has_omarchy,
            bar_ask: false,
            term_bg: None,
            acc: Accent::theme(),
            next: Vec::new(),
            up_tracks: Vec::new(),
            up_pos: 0,
            next_asked: NextKey::default(),
            next_seq: 0,
            next_gen: 0,
            next_draw: NextStrip::default(),
            songs: Vec::new(),
            ahead: 0,
            song_draw: NextList::default(),
            filtering: false,
            opt_open: false,
            feature_form: None,
            pick: None,
            opt_lines: Vec::new(),
            opt_sel: 0,
            kitty: HashMap::new(),
            kitty_want: Vec::new(),
            kitty_busy: false,
            kitty_next: 0,
            kitty_clock: 0,
            has_kitty: crate::art::kitty_supported(),
            login_tried: false,
            wheel_at: None,
            wheel_drawn: None,
            wheel_moved: false,
            wheel_tick: false,
            eq_bars: String::new(),
            eq_at: None,
            full: false,
            viz_list: false,
            viz_sel: 0,
            viz_from: 0,
            viz_style: VIZ_OPENING,
            viz_prev: None,
            viz_at: Instant::now(),
            viz: Visualizer::default(),
            viz_spec: Vec::new(),
            wave: Vec::new(),
        };
        for s in SEC_PLAYLISTS..=SEC_SONGS {
            let v = m.view(
                SECTION_NAMES[s],
                &format!("list:{}", SECTION_LISTS[s - SEC_PLAYLISTS]),
                None,
            );
            m.stacks[s] = vec![v];
        }
        let home = m.view("Home", "home:", None);
        m.stacks[SEC_HOME] = vec![home];
        let explore = Item {
            kind: apple::KIND_SHELF.into(),
            id: "discover".into(),
            name: "Explore".into(),
            route: "app:discover".into(),
            catalog: true,
            ..Default::default()
        };
        let explore = m.view("Explore", "explore:", Some(explore));
        m.stacks[SEC_EXPLORE] = vec![explore];
        let radio = Item {
            kind: apple::KIND_SHELF.into(),
            id: "radio".into(),
            name: "Radio".into(),
            catalog: true,
            ..Default::default()
        };
        let radio = m.view("Radio", "radio:", Some(radio));
        m.stacks[SEC_RADIO] = vec![radio];
        let mut search = m.view("Search", "search:", None);
        search.loaded = true;
        m.stacks[SEC_SEARCH] = vec![search];
        let queue = m.view("Queue", "queue:", None);
        m.stacks[SEC_QUEUE] = vec![queue];
        m
    }

    /// A new view with its own id.
    pub fn view(&mut self, title: &str, key: &str, item: Option<Item>) -> View {
        self.next_view += 1;
        View {
            id: self.next_view,
            title: title.to_string(),
            key: key.to_string(),
            item,
            ..Default::default()
        }
    }

    // ── commands ────────────────────────────────────────────────────────

    pub fn task(&mut self, f: impl Future<Output = Option<Msg>> + Send + 'static) {
        self.cmds.push(Cmd::Task(Box::pin(f)));
    }

    pub fn after(&mut self, d: Duration, m: Msg) {
        self.cmds.push(Cmd::After(d, Box::new(m)));
    }

    pub fn raw(&mut self, s: String) {
        self.cmds.push(Cmd::Raw(s));
    }

    pub fn quit(&mut self) {
        self.cmds.push(Cmd::Quit);
    }

    /// Fires a command at the daemon; failures surface as a flash.
    pub fn send(&mut self, r: Request) {
        let Some(c) = self.client.clone() else { return };
        self.task(async move { c.request(r).await.err().map(|e| Msg::Err(e.to_string())) });
    }

    /// Asks the daemon, and turns the answer into a message.
    pub fn ask(
        &mut self,
        r: Request,
        then: impl FnOnce(Result<Message, String>) -> Option<Msg> + Send + 'static,
    ) {
        let Some(c) = self.client.clone() else { return };
        self.task(async move { then(c.request(r).await.map_err(|e| e.to_string())) });
    }

    pub fn init(&mut self) {
        self.placing = true;
        self.ask_place();
        self.schedule(FRAME_EVERY);
        self.cmds.push(Cmd::Blocking(Box::new(|| {
            Some(Msg::Refresh(super::options::screen_refresh()))
        })));
        self.load_cur();
        self.maybe_fetch_cover();
        self.subscribe();
        self.auto_login();
        self.send(Request::new(ipc::CMD_UPDATE)); // look for an update on every start
        self.read_bar();
        self.fetch_next(false);
        self.after(Duration::from_secs(1), Msg::BinaryCheck);
    }

    /// Sets the next frame of the screen's own, replacing any other: only
    /// the newest counts, so there is always at most one.
    pub fn schedule(&mut self, every: Duration) {
        self.tick_seq += 1;
        self.tick_at = Some(Instant::now() + every);
    }

    /// When the screen next needs a frame of its own, if ever. Nothing
    /// ticks while nothing moves, so an idle brumm costs no wakeups.
    pub fn next_tick(&self) -> Option<Duration> {
        let st = &self.state.player;
        if self.blurred && !self.full {
            return st.playing.then_some(Duration::from_secs(1)); // the clock and the meter, for a glance
        }
        if self.full && !st.playing {
            return Some(2 * FRAME_EVERY); // the paused visualizer only breathes
        }
        if self.full {
            // The visualizer moves by the clock, so it draws at the
            // display's pace, not the spectrum's.
            return Some(Duration::from_secs(1) / self.draw_fps() as u32);
        }
        let busy_update = self
            .upd
            .as_ref()
            .is_some_and(|u| u.checking || u.installing);
        if self.marquee
            || !self.flash.is_empty()
            || self.thumb_busy
            || self.kitty_busy
            || !self.kitty_want.is_empty()
            || st.preview.is_some()
            || busy_update
            || self.state.status == ipc::STATUS_STARTING
            || self.cur().loading
        {
            return Some(FRAME_EVERY);
        }
        if self.card_shown {
            return Some(FRAME_EVERY.max(self.card_left())); // take the card away on time
        }
        if st.playing && self.eq_shown && self.spec.is_empty() {
            return Some(Duration::from_millis(125)); // no spectrum: the row's equalizer wobbles by itself
        }
        if st.playing {
            return Some(self.next_change()); // the spectrum's own frames move the rest
        }
        None
    }

    /// How long until the playing screen changes by itself: the bear's next
    /// step, or the clock's next second, whichever comes first. Ticks land
    /// just after it.
    fn next_change(&self) -> Duration {
        let bear = 5 * FRAME_EVERY;
        let since = self.start.elapsed();
        let mut d = bear - Duration::from_nanos((since.as_nanos() % bear.as_nanos()) as u64);
        let pos = self.position();
        let played = pos.fract();
        let left = (self.state.player.dur - pos).max(0.0).fract(); // the time left turns over on its own
        for s in [1.0 - played, left] {
            if s > 0.0 {
                d = d.min(Duration::from_secs_f64(s));
            }
        }
        Duration::from_millis(20).max(d + Duration::from_millis(5))
    }

    /// The spectrum can be seen — the visualizer, or the playing row's
    /// equalizer in a focused window.
    pub fn want_spec(&self) -> bool {
        self.full || (!self.blurred && self.eq_shown)
    }

    /// Tells the daemon what to stream: the spectrum only while it can be
    /// seen, and faster only for the fullscreen visualizer.
    pub fn subscribe(&mut self) {
        let mut r = Request {
            cmd: ipc::CMD_SUBSCRIBE.into(),
            bands: BANDS,
            fps: METER_FPS,
            ..Default::default()
        };
        if self.full {
            r.wave = 96.max(self.width * 3); // room for the scope to find its trigger
            r.fps = VIZ_FPS;
        } else if !self.want_spec() {
            r.bands = 0;
        }
        self.spec_fps = r.fps;
        self.spec_on = r.bands > 0;
        if !self.spec_on {
            self.spec.clear(); // stale bars would stand still: the wobble until it streams again
        }
        self.send(r);
    }

    /// Retries the daemon once a second, starting it if needed.
    pub fn reconnect(&mut self) {
        let tx = self.tx.clone();
        self.task(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let Ok((c, mut events)) = launch::connect().await else {
                return Some(Msg::Connected(None));
            };
            let r = Request {
                cmd: ipc::CMD_SUBSCRIBE.into(),
                bands: BANDS,
                fps: METER_FPS,
                ..Default::default()
            };
            let Ok(reply) = c.request(r).await else {
                return Some(Msg::Connected(None));
            };
            if let Some(tx) = tx {
                tokio::spawn(async move {
                    while let Some(m) = events.recv().await {
                        if tx.send(Msg::Event(m)).is_err() {
                            return;
                        }
                    }
                    let _ = tx.send(Msg::Closed);
                });
            }
            Some(Msg::Connected(Some((c, reply.state.unwrap_or_default()))))
        });
    }

    pub fn stack(&self) -> &Vec<View> {
        &self.stacks[self.section]
    }

    pub fn cur(&self) -> &View {
        self.stacks[self.section]
            .last()
            .expect("every section has its root view")
    }

    pub fn cur_mut(&mut self) -> &mut View {
        self.stacks[self.section]
            .last_mut()
            .expect("every section has its root view")
    }

    /// The view with this id, wherever it is.
    pub fn view_by_id(&mut self, id: u64) -> Option<&mut View> {
        self.stacks
            .iter_mut()
            .flat_map(|s| s.iter_mut())
            .find(|v| v.id == id)
    }

    pub fn load_cur(&mut self) {
        let id = self.cur().id;
        self.load(id);
    }

    /// Fills a view from the daemon, whose cache usually answers at once.
    pub fn load(&mut self, id: u64) {
        let logged_out = self.state.status == ipc::STATUS_LOGGED_OUT;
        let Some(v) = self.view_by_id(id) else { return };
        if v.loading || logged_out {
            return;
        }
        let req = if let Some(it) = &v.item {
            Request {
                cmd: ipc::CMD_OPEN.into(),
                item: Some(it.clone()),
                ..Default::default()
            }
        } else if let Some(list) = v.key.strip_prefix("list:") {
            Request {
                cmd: ipc::CMD_LIST.into(),
                list: list.into(),
                ..Default::default()
            }
        } else if let Some(q) = v.key.strip_prefix("search:").filter(|q| !q.is_empty()) {
            Request {
                cmd: ipc::CMD_SEARCH.into(),
                query: q.into(),
                ..Default::default()
            }
        } else if v.key == "queue:" {
            Request {
                cmd: ipc::CMD_QUEUE.into(),
                value: 500.0,
                ..Default::default()
            }
        } else if v.key == "home:" {
            Request::new(ipc::CMD_HOME)
        } else {
            return;
        };
        v.loading = true;
        if self.client.is_none() {
            return;
        }
        self.ask(req, move |reply| {
            Some(Msg::Loaded {
                view: id,
                reply,
                select: String::new(),
            })
        });
    }

    /// The cover to show: the preview's, else the playing song's as the
    /// library lists it (the same address the list prefetched), else what
    /// the player reports.
    pub fn stage_artwork(&self) -> String {
        let st = &self.state.player;
        if let Some(p) = &st.preview
            && !p.artwork.is_empty()
        {
            return p.artwork.clone();
        }
        if !st.id.is_empty() {
            for stack in &self.stacks {
                for v in stack {
                    for r in &v.rows {
                        if let Some(t) = &r.track
                            && t.id == st.id
                            && !t.artwork.is_empty()
                        {
                            return t.artwork.clone();
                        }
                    }
                }
            }
        }
        st.artwork.clone()
    }

    pub fn maybe_fetch_cover(&mut self) {
        let url = self.stage_artwork();
        if url == self.cover_url {
            return;
        }
        self.cover_url = url.clone();
        self.cover = None;
        self.cover_dim = None;
        self.rendered.clear();
        if url.is_empty() {
            return;
        }
        if let Some(img) = self.covers.get(&url) {
            self.cover = Some(img.clone());
            return;
        }
        self.fetch_cover(&url);
    }

    /// Loads a cover through the disk cache, once at a time per address.
    pub fn fetch_cover(&mut self, url: &str) {
        if self.fetching.contains_key(url) {
            return;
        }
        self.fetching.insert(url.to_string(), true);
        let (http, url) = (self.http.clone(), url.to_string());
        self.task(async move {
            let dir = crate::config::cache_dir().join("covers");
            let img = crate::art::cached(&http, &dir, &url)
                .await
                .ok()
                .map(Arc::new);
            Some(Msg::Cover { url, img })
        });
    }

    /// How many decoded covers stay: at least all prefetch reaches.
    fn cover_keep(&self) -> usize {
        80.max(3 * self.list_rows() as usize + 8)
    }

    /// Remembers a decoded cover, dropping the oldest past cover_keep.
    pub fn keep_cover(&mut self, url: &str, img: Arc<RgbaImage>) {
        if !self.covers.contains_key(url) {
            self.cover_lru.push(url.to_string());
        }
        self.covers.insert(url.to_string(), img);
        while self.cover_lru.len() > self.cover_keep() {
            let old = self.cover_lru.remove(0);
            self.covers.remove(&old);
        }
    }

    /// Draws a card off the event loop, one at a time: the one the screen
    /// wants, else one for a row next to the selection, so moving on finds
    /// its card drawn.
    pub fn render_thumb(&mut self) {
        if self.thumb_busy {
            return;
        }
        let mut w = self.thumb_want.clone();
        if w.key.is_empty() || self.thumbs.contains_key(&w.key) || !self.covers.contains_key(&w.url)
        {
            w = self.next_thumb();
        }
        let Some(img) = self.covers.get(&w.url).cloned() else {
            return;
        };
        if w.key.is_empty() {
            return;
        }
        self.thumb_busy = true;
        let style = self.opts.cover(self.has_kitty);
        self.cmds.push(Cmd::Blocking(Box::new(move || {
            Some(Msg::Thumb {
                key: w.key,
                lines: super::cover::draw_cover(&style, &img, w.size),
            })
        })));
    }

    /// Loads the covers of the rows on screen and a page either side, so a
    /// card or a song's cover is ready the moment it is wanted.
    pub fn prefetch_covers(&mut self, id: u64) {
        let page = self.list_rows() as usize;
        let Some(v) = self.view_by_id(id) else { return };
        let (lo, hi) = (
            v.off.saturating_sub(page),
            (v.off + 2 * page).min(v.rows.len()),
        );
        let mut order = vec![v.sel]; // the selected row first: its card shows it
        order.extend(lo..hi);
        let urls: Vec<String> = order
            .into_iter()
            .filter_map(|i| v.rows.get(i))
            .map(|r| {
                r.track
                    .as_ref()
                    .map(|t| t.artwork.clone())
                    .or_else(|| r.item.as_ref().map(|i| i.artwork.clone()))
                    .unwrap_or_default()
            })
            .collect();
        let mut seen = std::collections::HashSet::new();
        let mut n = 0;
        for url in urls {
            if n >= 32
                || url.is_empty()
                || !seen.insert(url.clone())
                || self.covers.contains_key(&url)
            {
                continue;
            }
            if !self.fetching.contains_key(&url) {
                n += 1;
            }
            self.fetch_cover(&url);
        }
    }

    pub fn prefetch_cur(&mut self) {
        let id = self.cur().id;
        self.prefetch_covers(id);
    }

    /// Handles one message. `still` says afterwards whether the screen can
    /// stay as drawn.
    pub fn update(&mut self, msg: Msg) {
        self.frame = (self.start.elapsed().as_millis() / FRAME_EVERY.as_millis()) as i64;
        self.still = false;
        if let Msg::Mouse(ms) = &msg
            && ms.kind == MouseKind::Moved
            && ms.button == Button::None
            && !self.dragging
        {
            // Hovering: the pointer's shape, nothing drawn.
            self.still = true;
            let shape = self.shape_at(ms.x, ms.y);
            self.set_pointer(shape);
            return;
        }
        let is_tick = matches!(msg, Msg::Tick(_));
        let was_motion =
            matches!(&msg, Msg::Event(m) if m.state.is_none() && m.options.is_none() && !m.library);
        self.update_inner(msg);
        if was_motion && self.full && self.tick_at.is_some() {
            self.still = true; // the visualizer draws on its own clock: the spectrum waits for the next frame
        }
        if self.client.is_some() && self.drawn && self.want_spec() != self.spec_on {
            self.subscribe(); // the playing row came into view, or left it
        }
        if !is_tick {
            // Something happened: give the screen a frame to settle, and
            // more for as long as it animates. A tick far off (a card
            // waiting to go) is brought forward when something now needs
            // frames sooner.
            match (self.tick_at, self.next_tick()) {
                (None, _) => self.schedule(FRAME_EVERY),
                (Some(at), Some(every))
                    if Instant::now() + every + Duration::from_millis(10) < at =>
                {
                    self.schedule(every)
                }
                _ => {}
            }
        }
        // A cover that just arrived, or a card the last frame lacked, is
        // drawn at once rather than on the next tick.
        self.render_thumb();
    }

    fn update_inner(&mut self, msg: Msg) {
        match msg {
            Msg::Focus(true) => {
                self.blurred = false;
                self.subscribe();
                self.term_bg = super::accent::theme_background().or(self.term_bg); // the theme may have changed
            }
            Msg::Focus(false) => {
                self.blurred = true;
                self.subscribe();
                self.set_pointer(String::new());
            }
            Msg::Probe { bg, releases } => {
                self.term_bg = super::accent::theme_background().or(bg);
                self.releases = self.releases || releases;
            }
            Msg::Resize(w, h) => {
                self.width = w as i64;
                self.height = h as i64;
                self.cell_aspect = super::cell_aspect();
            }
            Msg::Tick(seq) => {
                if seq != self.tick_seq {
                    self.still = true;
                    return; // replaced by a sooner one
                }
                self.tick_at = None;
                if self.full && !self.opts.o.no_viz_cycle && self.viz_at.elapsed() > VIZ_EVERY {
                    self.show_viz((self.viz_style + 1) % viz::NAMES.len());
                }
                if !self.flash.is_empty() && self.flash_at.elapsed() > Duration::from_secs(4) {
                    self.flash.clear();
                }
                self.render_thumb();
                self.kitty_send();
                if let Some(every) = self.next_tick() {
                    self.schedule(every);
                }
            }
            Msg::Kitty { key, seq } => self.kitty_sent(key, seq),
            Msg::UpdateChecked(r) => self.update_checked(r),
            Msg::Installed(r) => self.update_installed(r),
            Msg::Refresh(r) => self.refresh = r,
            Msg::Bar(on) => self.bar_read(on),
            Msg::BarSet { on, err } => self.bar_set(on, err),
            Msg::Thumb { key, lines } => {
                self.thumb_busy = false;
                if self.thumbs.len() > 150 {
                    self.thumbs.clear();
                }
                self.thumbs.insert(key, lines);
                self.render_thumb();
            }
            Msg::Event(m) => self.event(m),
            Msg::Closed => {
                // The daemon went away. After an update it restarts with a
                // new binary; the player restarts into it too.
                if launch::binary_updated() {
                    self.reexec = true;
                    self.quit();
                    return;
                }
                self.state.status = ipc::STATUS_STARTING.into();
                self.state.message = "reconnecting".into();
                self.reconnect();
            }
            Msg::BinaryCheck => {
                if launch::binary_updated() {
                    self.reexec = true;
                    self.quit();
                    return;
                }
                self.still = true;
                self.after(Duration::from_secs(1), Msg::BinaryCheck);
            }
            Msg::Connected(None) => self.reconnect(),
            Msg::Connected(Some((c, st))) => {
                self.client = Some(c);
                self.state = st;
                self.state_at = Instant::now();
                self.opt_queue = None;
                self.load_cur();
                self.fetch_next(true);
            }
            Msg::Loaded {
                view,
                reply,
                select,
            } => self.loaded(view, reply, select),
            Msg::ExploreGroup {
                view,
                load,
                group,
                shelves,
            } => {
                let cur = self.cur().id;
                let Some(v) = self.view_by_id(view) else {
                    return;
                };
                if v.explore.as_ref().map(|e| e.id) != Some(load) {
                    return;
                }
                super::discovery::merge_explore_group(v, group, shelves);
                if view == cur {
                    self.prefetch_covers(view);
                }
            }
            Msg::Ratings(r) => {
                for (id, v) in r {
                    self.rating.insert(id, v);
                }
            }
            Msg::PickerLists(r) => {
                if let Some(p) = self.pick.as_mut() {
                    p.loading = false;
                    match r {
                        Ok(l) => p.lists = l,
                        Err(e) => self.set_flash(&e),
                    }
                }
            }
            Msg::Seek(seq) => {
                if seq == self.seek_seq {
                    let d = self.state.player.dur;
                    if d > 0.0 && self.seek_to >= d - 1.0 {
                        self.skip();
                        return;
                    }
                    let v = self.seek_to;
                    self.send(Request {
                        cmd: ipc::CMD_SEEK.into(),
                        value: v,
                        ..Default::default()
                    });
                }
            }
            Msg::Typed(seq) => {
                if seq == self.typed_seq && self.section == SEC_SEARCH {
                    self.run_search(false); // not after leaving it
                }
            }
            Msg::Flash(s) => self.set_flash(&s),
            Msg::Place(p) => self.got_place(p),
            Msg::Open(it, song) => {
                let v = self.view(&it.name.clone(), &it.key(), Some(it));
                self.push(v, &song);
            }
            Msg::Paste(text) => {
                let t = text.trim().to_string();
                if self.filtering {
                    self.cur_mut().filter.push_str(&t);
                    super::filter::apply_filter(self.cur_mut());
                    self.prefetch_cur();
                    return;
                }
                if self.searching {
                    self.query.push_str(&t);
                    self.typed();
                    return;
                }
                // A pasted music.apple.com link opens right away.
                self.query = t;
                self.run_search(true);
            }
            Msg::Cover { url, img } => {
                self.fetching.remove(&url);
                if let Some(img) = &img {
                    self.keep_cover(&url, img.clone());
                }
                if url == self.cover_url {
                    self.cover = img;
                }
            }
            Msg::Next {
                seq,
                tracks,
                pos,
                ok,
            } => self.got_next(seq, tracks, pos, ok),
            Msg::Album(r) => match r {
                Err(e) => self.set_flash(&e),
                Ok((it, song)) => {
                    let v = self.view(&it.name.clone(), &it.key(), Some(it));
                    self.push(v, &song);
                }
            },
            Msg::Login(r) => match r {
                Err(e) => self.set_flash(&format!("sign-in failed: {e}")),
                Ok(()) => {
                    self.set_flash("signed in");
                    self.send(Request::new(ipc::CMD_RELOAD));
                }
            },
            Msg::Err(e) => self.set_flash(&e),
            Msg::Hold(seq) => self.hold(seq),
            Msg::Key(k) if k.release => {
                // Any release proves the terminal reports them — the
                // startup answer is not always right — so hold-to-preview
                // can work.
                self.releases = true;
                if k.name == "space" {
                    self.space_up();
                } else {
                    self.still = true;
                }
            }
            Msg::Feedback(r) => self.feedback_done(r),
            Msg::FeatureSaved => {
                self.set_flash("Created in Apple Music");
                let id = self.cur().id;
                if let Some(v) = self.view_by_id(id) {
                    v.loading = false;
                }
                self.load(id);
            }
            Msg::Key(k) => self.key_press(k),
            Msg::Mouse(ms) => self.mouse(ms),
            Msg::Wheel => self.wheel_frame(),
            Msg::Quit => self.quit(),
        }
    }

    fn loaded(&mut self, id: u64, reply: Result<Message, String>, select: String) {
        let frame = self.frame;
        let Some(v) = self.view_by_id(id) else { return };
        v.loading = false;
        let reply = match reply {
            Err(e) => {
                v.err = Some(e);
                return;
            }
            Ok(r) => r,
        };
        let explorer = reply.explorer.clone();
        super::browse::fill(v, reply);
        if !select.is_empty() {
            v.want = select;
        }
        let want = std::mem::take(&mut v.want);
        select_song(v, &want, frame);
        let is_explore = v.key == "explore:";
        if is_explore && let Some(page) = explorer {
            self.load_explore_visuals(id, page);
        }
        self.maybe_resume();
        self.fetch_ratings(id);
        self.prefetch_covers(id);
        self.maybe_fetch_cover();
        self.warm(id);
    }

    pub fn event(&mut self, msg: Message) {
        if let Some(mut st) = msg.state {
            let was_logged_out = self.state.status == ipc::STATUS_LOGGED_OUT;
            self.keep_showing(&mut st);
            self.keep_playing(&mut st);
            let song_changed = st.player.id != self.state.player.id;
            self.state = st;
            self.state_at = Instant::now();
            if song_changed && self.section == SEC_QUEUE && self.stack().len() == 1 {
                self.load_cur();
            }
            let e = self.state.player.err.clone();
            if !e.is_empty() && e != self.last_err {
                self.set_flash(&e);
            }
            self.last_err = e;
            if was_logged_out || !self.cur().loaded {
                self.load_cur();
            }
            self.maybe_fetch_cover();
            self.maybe_resume();
            self.auto_login();
            self.fetch_next(false);
        }
        if let Some(o) = msg.options {
            self.options_changed(o);
        }
        if !msg.spectrum.is_empty() {
            self.feed_spectrum(&msg.spectrum);
        }
        if !msg.wave.is_empty() {
            self.wave = msg.wave.iter().map(|&v| v as f64 / 100.0).collect();
        }
        if msg.library {
            // Refreshed in the background: reload what is on screen, keeping
            // the old rows visible until the new ones arrive.
            for s in SEC_PLAYLISTS..=SEC_SONGS {
                let ids: Vec<u64> = self.stacks[s].iter().map(|v| v.id).collect();
                for id in ids {
                    if s == self.section {
                        self.load(id);
                    } else if let Some(v) = self.view_by_id(id) {
                        v.loaded = false;
                    }
                }
            }
        }
    }

    pub fn set_flash(&mut self, s: &str) {
        self.flash = s.to_string();
        self.flash_at = Instant::now();
    }

    /// Eases the bars: they jump up at once and fall back slowly, which
    /// reads as motion rather than flicker.
    fn feed_spectrum(&mut self, input: &[i64]) {
        if self.spec.len() != input.len() {
            self.spec = vec![0.0; input.len()];
        }
        let fall = 0.86f64.powf(FALL_REF / self.spec_fps.max(1) as f64); // the same fall per second at any rate
        for (s, &v) in self.spec.iter_mut().zip(input) {
            *s = (v as f64 / 255.0).max(*s * fall);
        }
    }

    /// The playhead between state updates; right after a seek it shows
    /// where the seek goes, so the bar never jumps back.
    pub fn position(&self) -> f64 {
        let st = &self.state.player;
        if let Some(at) = self.seek_at {
            let since = at.elapsed();
            if since < Duration::from_millis(1500) {
                let mut p = self.seek_to;
                if st.playing && since > Duration::from_millis(250) {
                    p += (since - Duration::from_millis(250)).as_secs_f64();
                }
                return p.min(st.dur.max(p));
            }
        }
        let mut p = st.pos;
        if st.playing {
            p += self.state_at.elapsed().as_secs_f64();
        }
        if st.dur > 0.0 {
            p = p.min(st.dur);
        }
        p
    }

    // ── navigation ──────────────────────────────────────────────────────

    pub fn switch_to(&mut self, s: usize) {
        self.player_view = false;
        self.section = s;
        self.help = false;
        self.filtering = false; // a filter stays with its list, the typing does not
        if s == SEC_QUEUE {
            self.stacks[SEC_QUEUE].truncate(1);
            self.cur_mut().loaded = false; // always fresh
        }
        self.searching = false; // the box stays behind; at Search, typing a letter goes into it
        if !self.cur().loaded {
            self.load_cur();
        }
    }

    /// Opens v over the current view. A view the stack already holds is
    /// gone back to instead, rows and selection kept, so going between an
    /// artist and an album never piles up. song, when given, is selected.
    pub fn push(&mut self, mut v: View, song: &str) {
        let frame = self.frame;
        let s = &mut self.stacks[self.section];
        if let Some(i) = s.iter().position(|o| o.key == v.key) {
            s.truncate(i + 1);
        } else {
            v.sel_at = frame;
            s.push(v);
        }
        let cur = self.cur_mut();
        if cur.loaded {
            select_song(cur, song, frame);
            return;
        }
        if !song.is_empty() {
            cur.want = song.to_string();
        }
        self.load_cur();
    }

    pub fn back(&mut self) {
        if self.stack().len() > 1 {
            self.stacks[self.section].pop();
        }
    }

    /// Opens the selected item or plays the selected song.
    pub fn activate(&mut self) {
        let v = self.cur();
        let Some(r) = v.rows.get(v.sel).cloned() else {
            return;
        };
        if !r.selectable() {
            return;
        }
        if let Some(it) = r.item.as_deref() {
            let it = it.clone();
            if it.route == "action:details" {
                self.inspect_selected();
                return;
            }
            if let Some(kind) = it.route.strip_prefix("action:favorite:") {
                let refs = vec![Ref {
                    kind: kind.into(),
                    id: it.id.clone(),
                }];
                self.ask(
                    Request {
                        cmd: ipc::CMD_FAVORITE.into(),
                        refs,
                        ..Default::default()
                    },
                    |r| match r {
                        Err(e) => Some(Msg::Err(e)),
                        Ok(_) => Some(Msg::Flash(
                            "Favorite request accepted by Apple Music".into(),
                        )),
                    },
                );
                return;
            }
            if it.route.starts_with("action:") {
                let mut parent = String::new();
                if let Some(vi) = &self.cur().item {
                    if vi.route == "app:folders" {
                        parent = "p.playlistsroot".into();
                    } else if let Some((_, tail)) = vi.route.split_once("/playlist-folders/") {
                        parent = tail
                            .split('/')
                            .next()
                            .unwrap_or("")
                            .split('?')
                            .next()
                            .unwrap_or("")
                            .to_string();
                    }
                }
                self.feature_form = Some(FeatureForm {
                    item: it,
                    input: String::new(),
                    parent,
                });
                return;
            }
            match it.kind.as_str() {
                KIND_VIDEO => return self.open_music_video(it),
                KIND_STATION => {
                    return self.start_station(Request {
                        cmd: ipc::CMD_STATION.into(),
                        item: Some(it),
                        ..Default::default()
                    });
                }
                KIND_TERM => {
                    self.query = it.name.clone();
                    return self.run_search(true);
                }
                _ => {}
            }
            let nv = self.view(&it.name.clone(), &it.key(), Some(it));
            return self.push(nv, "");
        }
        let t = r
            .track
            .as_deref()
            .expect("a selectable row without an item has a track")
            .clone();
        let (key, qpos, sel) = (v.key.clone(), v.qpos, v.sel);
        if key == "queue:" {
            let src = self.state.player.source.clone();
            self.show_playing_at(t, &src, qpos + sel as i64);
            self.send(Request {
                cmd: ipc::CMD_JUMP.into(),
                value: (qpos + sel as i64) as f64,
                ..Default::default()
            });
            return;
        }
        let (ids, at) = self.cur().songs();
        let i = at[&sel];
        let mut window: Vec<String> = ids[i..(i + QUEUE_AFTER).min(ids.len())].to_vec();
        if self.state.player.shuffle && ids.len() > window.len() {
            // Shuffle draws from the whole list, not just the songs around
            // the chosen one: it first, then a random pick of the rest.
            window = std::iter::once(t.id.clone())
                .chain(sample(&ids, &t.id, QUEUE_AFTER - 1))
                .collect();
        }
        let start = t.id.clone();
        self.show_playing(t, &key);
        self.send(Request {
            cmd: ipc::CMD_PLAY.into(),
            ids: window,
            start,
            source: key,
            ..Default::default()
        });
    }

    /// Moves the playhead to sec. Repeated seek keys collect into one
    /// request, sent once they stop; the bar shows the target at once.
    pub fn seek(&mut self, sec: f64) {
        let d = self.state.player.dur;
        if d <= 0.0 {
            return;
        }
        self.seek_to = sec.clamp(0.0, d);
        self.seek_at = Some(Instant::now());
        self.seek_seq += 1;
        self.after(Duration::from_millis(250), Msg::Seek(self.seek_seq));
    }

    /// The song under the cursor, or else the one playing.
    pub fn selected_track(&self) -> Option<String> {
        let v = self.cur();
        if let Some(t) = v.rows.get(v.sel).and_then(|r| r.track.as_ref()) {
            return Some(t.id.clone());
        }
        Some(self.state.player.id.clone()).filter(|id| !id.is_empty())
    }

    /// Opens what a song belongs to (CMD_ARTIST, CMD_ALBUM).
    pub fn lookup(&mut self, cmd: &str) {
        if let Some(id) = self.selected_track() {
            self.lookup_id(cmd, &id);
        }
    }

    pub fn lookup_id(&mut self, cmd: &str, id: &str) {
        let song = id.to_string();
        self.ask(
            Request {
                cmd: cmd.into(),
                start: id.into(),
                ..Default::default()
            },
            move |r| match r {
                Ok(m) if !m.items.is_empty() => Some(Msg::Album(Ok((m.items[0].clone(), song)))),
                _ => Some(Msg::Err("nothing found for this song".into())),
            },
        );
    }

    /// Adds the selected song — or everything in the selected album or
    /// playlist — to play next or at the end of the queue.
    pub fn enqueue(&mut self, next: bool) {
        let v = self.cur();
        let Some(r) = v.rows.get(v.sel).cloned() else {
            return;
        };
        let Some(c) = self.client.clone() else { return };
        let done = if next {
            "plays next"
        } else {
            "added to the queue"
        };
        self.task(async move {
            let ids = match super::browse::songs_of(&c, &r).await {
                Err(e) => return Some(Msg::Err(e)),
                Ok(ids) => ids,
            };
            if ids.is_empty() {
                return None;
            }
            let req = Request {
                cmd: ipc::CMD_ENQUEUE.into(),
                ids,
                value: if next { 1.0 } else { 0.0 },
                ..Default::default()
            };
            match c.request(req).await {
                Err(e) => Some(Msg::Err(e.to_string())),
                Ok(_) => Some(Msg::Flash(done.into())),
            }
        });
    }

    /// Puts the song's music.apple.com link on the clipboard.
    pub fn copy_link(&mut self) {
        let Some(id) = self.selected_track() else {
            return;
        };
        self.ask(
            Request {
                cmd: ipc::CMD_LINK.into(),
                start: id,
                ..Default::default()
            },
            |r| {
                let link = match r {
                    Err(e) => return Some(Msg::Err(e)),
                    Ok(m) => m.link,
                };
                let copied = std::process::Command::new("wl-copy")
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .and_then(|mut c| {
                        use std::io::Write;
                        c.stdin
                            .take()
                            .map(|mut s| s.write_all(link.as_bytes()))
                            .transpose()?;
                        c.wait()
                    });
                match copied {
                    Ok(s) if s.success() => Some(Msg::Flash("link copied".into())),
                    Ok(s) => Some(Msg::Err(format!("copy: {s}"))),
                    Err(e) => Some(Msg::Err(format!("copy: {e}"))),
                }
            },
        );
    }

    /// Starts a tap-or-hold: a tap toggles playback on release, a hold
    /// previews the selected song until release. Terminals that do not
    /// report releases just toggle.
    pub fn space_down_key(&mut self, repeat: bool) {
        if !self.releases {
            return self.toggle();
        }
        if repeat || self.space_down {
            return;
        }
        self.space_down = true;
        self.space_seq += 1;
        self.after(HOLD_DELAY, Msg::Hold(self.space_seq));
    }

    fn hold(&mut self, seq: u64) {
        if !self.space_down || seq != self.space_seq {
            return;
        }
        let v = self.cur();
        let Some(t) = v.rows.get(v.sel).and_then(|r| r.track.clone()) else {
            return;
        }; // nothing to preview: release will toggle
        self.previewing = true;
        self.send(Request {
            cmd: ipc::CMD_PREVIEW.into(),
            start: t.id.clone(),
            value: 1.0,
            ..Default::default()
        });
    }

    /// Starts previewing the selected song, or ends a preview — as holding
    /// space does, for terminals that cannot report releases.
    pub fn toggle_preview(&mut self) {
        if self.previewing {
            self.previewing = false;
            return self.send(Request::new(ipc::CMD_PREVIEW));
        }
        let v = self.cur();
        let Some(t) = v.rows.get(v.sel).and_then(|r| r.track.clone()) else {
            return;
        };
        self.previewing = true;
        self.send(Request {
            cmd: ipc::CMD_PREVIEW.into(),
            start: t.id.clone(),
            value: 1.0,
            ..Default::default()
        });
    }

    fn space_up(&mut self) {
        if !self.space_down {
            return;
        }
        self.space_down = false;
        if self.previewing {
            self.previewing = false;
            return self.send(Request::new(ipc::CMD_PREVIEW));
        }
        self.toggle();
    }

    /// Switches the fullscreen visualizer, asking the daemon for waveform
    /// data only while it shows.
    pub fn toggle_full(&mut self) {
        self.full = !self.full;
        self.viz_list = false;
        if self.full {
            self.viz_style = VIZ_OPENING;
            self.viz_prev = None;
            self.viz_at = Instant::now();
        }
        self.subscribe();
    }

    /// The keys that mean something else in fullscreen: tab steps through
    /// the styles, v lists them, a switches the minutely change on and
    /// off; other keys (n, space…) work as ever. Says whether k was one.
    pub fn full_key(&mut self, k: &str) -> bool {
        let n = viz::NAMES.len();
        if self.viz_list {
            match k {
                "up" | "k" | "shift+tab" => {
                    self.viz_sel = (self.viz_sel + n - 1) % n;
                    self.preview_viz(self.viz_sel);
                }
                "down" | "j" | "tab" => {
                    self.viz_sel = (self.viz_sel + 1) % n;
                    self.preview_viz(self.viz_sel);
                }
                "enter" | "space" | " " | "l" | "v" => self.viz_list = false, // keep what shows
                "esc" | "q" | "h" => {
                    self.preview_viz(self.viz_from); // back to the style before the list
                    self.viz_list = false;
                }
                "a" => self.toggle_viz_cycle(),
                _ => {
                    if let Some(i) = viz_digit(k) {
                        self.pick_viz(i);
                    }
                }
            }
            return true;
        }
        match k {
            "esc" | "f" | "q" => self.toggle_full(),
            "tab" | "right" | "down" | "j" | "l" => self.show_viz((self.viz_style + 1) % n),
            "shift+tab" | "left" | "up" | "k" | "h" => self.show_viz((self.viz_style + n - 1) % n),
            "v" => {
                self.viz_list = true;
                self.viz_sel = self.viz_style;
                self.viz_from = self.viz_style;
            }
            "a" => self.toggle_viz_cycle(),
            "F" => {
                self.opts.o.viz_fps = step(&super::options::DRAW_RATES, self.opts.o.viz_fps, 1);
                self.save_options();
                let label = self.fps_label();
                self.set_flash(&format!("visualizer: {label}"));
            }
            _ => {
                let Some(i) = viz_digit(k) else { return false };
                if i != self.viz_style {
                    self.show_viz(i);
                }
            }
        }
        true
    }

    /// Shows style i at once, without the dissolve: moving through the
    /// list shows each style as the cursor lands on it.
    pub fn preview_viz(&mut self, i: usize) {
        if i != self.viz_style {
            self.viz_prev = None;
            self.viz_style = i;
            self.viz_at = Instant::now();
        }
    }

    /// Shows style i and closes the list.
    pub fn pick_viz(&mut self, i: usize) {
        self.viz_list = false;
        if i != self.viz_style {
            self.show_viz(i);
        }
    }

    /// Switches the minutely change of style, and remembers it.
    fn toggle_viz_cycle(&mut self) {
        self.opts.o.no_viz_cycle = !self.opts.o.no_viz_cycle;
        self.save_options();
        self.viz_at = Instant::now(); // switched back on: the next change is a minute away
        let msg = if self.opts.o.no_viz_cycle {
            "style stays"
        } else {
            "styles change every minute"
        };
        self.set_flash(msg);
    }

    pub fn show_viz(&mut self, style: usize) {
        self.viz_prev = Some(self.viz_style);
        self.viz_style = style;
        self.viz_at = Instant::now();
    }

    /// Shows the list the current song was started from with the song
    /// selected, reopening that list if needed.
    pub fn jump_to_playing(&mut self) {
        let (src, id) = (
            self.state.player.source.clone(),
            self.state.player.id.clone(),
        );
        if src.is_empty() {
            return;
        }
        let frame = self.frame;
        for s in 0..NUM_SECTIONS {
            if let Some(i) = self.stacks[s].iter().position(|v| v.key == src) {
                self.section = s;
                self.stacks[s].truncate(i + 1);
                let v = self.cur_mut();
                if let Some(j) = v
                    .rows
                    .iter()
                    .position(|r| r.track.as_ref().is_some_and(|t| t.id == id))
                {
                    v.sel = j;
                    v.sel_at = frame;
                }
                return;
            }
        }
        for s in SEC_PLAYLISTS..=SEC_ARTISTS {
            let found = self.stacks[s][0].rows.iter().find_map(|r| {
                r.item
                    .as_ref()
                    .filter(|it| it.key() == src)
                    .map(|it| (**it).clone())
            });
            if let Some(it) = found {
                let mut v = self.view(&it.name.clone(), &src, Some(it.clone()));
                v.loading = true;
                let vid = v.id;
                self.section = s;
                self.stacks[s].truncate(1);
                self.stacks[s].push(v);
                self.ask(
                    Request {
                        cmd: ipc::CMD_OPEN.into(),
                        item: Some(it),
                        ..Default::default()
                    },
                    move |reply| {
                        Some(Msg::Loaded {
                            view: vid,
                            reply,
                            select: id,
                        })
                    },
                );
                return;
            }
        }
    }

    /// Lands on what is playing the first time brumm opens mid-song.
    pub fn maybe_resume(&mut self) {
        if self.resumed || self.placing || self.state.player.source.is_empty() {
            return;
        }
        let root = if self.state.player.source.contains(":album:") {
            SEC_ALBUMS
        } else {
            SEC_PLAYLISTS
        };
        if !self.stacks[root][0].loaded {
            let id = self.stacks[root][0].id;
            self.load(id);
            return;
        }
        self.resumed = true;
        self.jump_to_playing();
    }

    /// Whether k, pressed on the Search section's results with the box not
    /// focused, starts typing into it: any character but a digit (the
    /// sections) and the few keys that mean something everywhere.
    pub fn types_search(&self, k: &str) -> bool {
        if self.section != SEC_SEARCH
            || self.stack().len() != 1
            || self.full
            || k.chars().count() != 1
        {
            return false;
        }
        let r = k.chars().next().unwrap_or(' ');
        !r.is_control() && r != ' ' && !r.is_numeric() && !"/?ofgqQ".contains(r)
    }

    pub fn key_press(&mut self, msg: Key) {
        if self.feature_form.is_some() {
            return self.feature_key(&msg);
        }
        if self.fb.is_some() {
            return self.feedback_key(&msg);
        }
        if self.upd.is_some() {
            return self.update_key(&msg.name);
        }
        if self.bar_asking() {
            return self.bar_key(&msg.name);
        }
        if self.pick.is_some() {
            return self.picker_key(&msg);
        }
        if self.opt_open {
            return self.options_key(&msg.name);
        }
        if self.searching {
            return self.search_key(&msg);
        }
        if self.filtering {
            return self.filter_key(&msg);
        }
        if msg.name == "space" || msg.name == " " {
            return self.space_down_key(msg.repeat);
        }
        if msg.name == "?" && !self.full {
            self.help = !self.help;
            return;
        }
        if self.full && self.full_key(&msg.name) {
            return;
        }
        self.key(&msg.name);
    }

    pub fn key(&mut self, k: &str) {
        if self.types_search(k) {
            self.searching = true;
            self.help = false;
            self.query.push_str(k);
            return self.typed();
        }
        match k {
            // Done listening: the music stops and brumm with it.
            "q" => {
                let c = self.client.clone();
                self.task(async move {
                    if let Some(c) = c {
                        let _ = c.request(Request::new(ipc::CMD_QUIT)).await;
                    }
                    Some(Msg::Quit)
                });
            }
            "Q" | "ctrl+c" | "ctrl+q" => self.quit(), // close the window; the music plays on
            "L" => self.sign_in(),
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => {
                self.switch_to(k.as_bytes()[0] as usize - b'1' as usize)
            }
            "tab" | "right" => {
                if self.section == SEC_QUEUE {
                    self.switch_to(SEC_HOME);
                } else {
                    self.switch_to((self.section + 1) % SEC_QUEUE);
                }
            }
            "shift+tab" | "left" => self.switch_to((self.section + SEC_QUEUE - 1) % SEC_QUEUE),
            "/" => {
                if self.filterable() {
                    self.start_filter(); // here first; tab goes on to all of Apple Music
                    return;
                }
                self.section = SEC_SEARCH;
                self.searching = true;
                self.help = false;
            }
            "f" => self.toggle_full(),
            "n" => self.skip(),
            "p" | "b" => self.go_back(),
            "shift+left" | "shift+right" => {
                let delta = if k == "shift+left" { -10.0 } else { 10.0 };
                self.seek(self.position() + delta);
            }
            "+" | "=" | "-" => {
                let delta = if k == "-" { -0.05 } else { 0.05 };
                self.set_volume(self.state.player.volume + delta);
            }
            "m" => {
                if self.state.player.volume > 0.0 {
                    self.last_vol = self.state.player.volume;
                    self.set_volume(0.0);
                } else {
                    self.set_volume(self.last_vol);
                }
            }
            "s" => {
                let on = !self.state.player.shuffle;
                self.state.player.shuffle = on;
                self.set_flash(if on { "shuffle on" } else { "shuffle off" });
                self.send(Request {
                    cmd: ipc::CMD_SHUFFLE.into(),
                    value: if on { 1.0 } else { 0.0 },
                    ..Default::default()
                });
            }
            "r" => {
                // off → all → one → off, the order most players use.
                let next = [2, 0, 1][self.state.player.repeat.clamp(0, 2) as usize];
                self.state.player.repeat = next;
                self.set_flash(["repeat off", "repeat one", "repeat all"][next as usize]);
                self.send(Request {
                    cmd: ipc::CMD_REPEAT.into(),
                    value: next as f64,
                    ..Default::default()
                });
            }
            "[" | "]" => {
                let delta = if k == "[" { -0.05 } else { 0.05 };
                let s = self.shown_split() + delta;
                self.set_split(s);
                save_split(self.split);
            }
            "c" => {
                self.jump_to_playing();
                self.player_view = true;
            }
            "o" => {
                self.opt_open = true;
                self.help = false;
            }
            "O" => self.toggle_preview(),
            "ctrl+i" => self.inspect_selected(),
            "i" => self.add_to_library(),
            "a" => self.lookup(ipc::CMD_ALBUM), // the selected song's, else the playing one's
            "A" => self.lookup(ipc::CMD_ARTIST),
            "z" | "Z" => self.enqueue(k == "Z"),
            "*" => self.rate(apple::LOVE),
            "d" => self.rate(apple::DISLIKE),
            "R" => self.play_station(),
            "P" => self.open_picker(),
            "y" => self.copy_link(),
            "!" => {
                self.fb = Some(Feedback {
                    agent: crate::update::has_agent(),
                    ..Default::default()
                });
                self.help = false;
            }
            "U" => self.check_update(),
            "g" => self.open_window(),
            "esc" | "h" | "backspace" => {
                if self.help {
                    self.help = false;
                    return;
                }
                if self.cur().all.is_some() && k == "esc" {
                    super::filter::clear_filter(self.cur_mut()); // the filter goes before the view does
                    self.filtering = false;
                    return;
                }
                if self.previewing {
                    return self.toggle_preview();
                }
                self.back();
            }
            "enter" | "l" => {
                let v = self.cur();
                if self.section == SEC_SEARCH
                    && self.stack().len() == 1
                    && v.rows.get(v.sel).is_none_or(|r| !r.selectable())
                {
                    self.searching = true; // no results to open: into the box
                    return;
                }
                self.activate();
            }
            _ => self.move_sel(k),
        }
    }

    /// Asks for the list's share; render keeps both sides their minimum,
    /// and a cover held back by the height gives the list the rest.
    pub fn set_split(&mut self, f: f64) {
        self.split = f.clamp(0.1, 0.95);
    }

    /// The list's share as drawn.
    pub fn shown_split(&self) -> f64 {
        let inner = self.width - 2 * super::view::MARGIN;
        let (nav_w, col_w, _) = self.layout(inner, self.height - super::view::BODY_TOP - 2);
        if col_w > 0 {
            return nav_w as f64 / inner as f64;
        }
        self.split
    }

    fn set_volume(&mut self, v: f64) {
        let v = v.clamp(0.0, 1.0);
        self.state.player.volume = v; // feel immediate; the daemon confirms
        self.set_flash(&format!("volume {}%", (v * 100.0).round() as i64));
        self.send(Request {
            cmd: ipc::CMD_VOLUME.into(),
            value: v,
            ..Default::default()
        });
    }

    /// Edits the search box; results follow the typing.
    fn search_key(&mut self, msg: &Key) {
        match msg.name.as_str() {
            "esc" => {
                self.searching = false;
                return;
            }
            "enter" => {
                self.searching = false;
                return self.run_search(true);
            }
            "backspace" => {
                self.query.pop();
            }
            "ctrl+w" => {
                let q = self.query.trim_end_matches(' ');
                self.query = match q.rfind(' ') {
                    Some(i) => q[..=i].to_string(),
                    None => String::new(),
                };
            }
            "ctrl+u" => self.query.clear(),
            "ctrl+c" | "ctrl+q" => return self.quit(),
            "down" | "up" => {
                self.searching = false; // move into the results
                return self.move_sel(&msg.name);
            }
            "left" | "right" | "tab" | "shift+tab" => {
                self.searching = false; // on to the next section: the box has no cursor to move
                return self.key(&msg.name);
            }
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" if self.query.is_empty() => {
                // nothing typed yet: a digit is a section, as outside the box
                self.searching = false;
                return self.key(&msg.name);
            }
            _ => {
                if msg.text.is_empty() {
                    return;
                }
                self.query.push_str(&msg.text);
            }
        }
        self.typed();
    }

    /// Runs the search once the query rests for a moment.
    pub fn typed(&mut self) {
        self.typed_seq += 1;
        self.after(Duration::from_millis(350), Msg::Typed(self.typed_seq));
    }

    /// Searches for the query — or opens it, if it is a music.apple.com
    /// link. final is true for enter or a paste.
    pub fn run_search(&mut self, final_: bool) {
        let q = self.query.trim().to_string();
        self.section = SEC_SEARCH;
        if let Some((it, song)) = apple::parse_link(&q) {
            if !final_ {
                return;
            }
            self.searching = false;
            self.stacks[SEC_SEARCH].truncate(1);
            if it.id.is_empty() {
                // a song link: open its album
                self.ask(
                    Request {
                        cmd: ipc::CMD_ALBUM.into(),
                        start: song.clone(),
                        ..Default::default()
                    },
                    move |r| match r {
                        Ok(m) if !m.items.is_empty() => Some(Msg::Open(m.items[0].clone(), song)),
                        _ => Some(Msg::Err("could not open that link".into())),
                    },
                );
                return;
            }
            self.after(Duration::ZERO, Msg::Open(it, song));
            return;
        }
        if q.chars().count() < 2 {
            return;
        }
        if self.stacks[SEC_SEARCH][0].key == format!("search:{q}") {
            self.stacks[SEC_SEARCH].truncate(1); // already found: back to the results
            return;
        }
        let v = self.view("Search", &format!("search:{q}"), None);
        self.stacks[SEC_SEARCH] = vec![v];
        self.load_cur();
    }

    pub fn move_sel(&mut self, k: &str) {
        let prev = self.cur().sel;
        let ok = self.step(k);
        if self.cur().sel == prev
            && (k == "up" || k == "k")
            && self.section == SEC_SEARCH
            && self.stack().len() == 1
        {
            self.searching = true; // up from the first result: back into the box
            return;
        }
        if ok {
            self.prefetch_cur();
        }
    }

    /// Moves the selection by a key; false if k is no move.
    pub fn step(&mut self, k: &str) -> bool {
        let page = (self.list_rows() - 1).max(1);
        let frame = self.frame;
        let v = self.cur_mut();
        let n = v.rows.len() as i64;
        if n == 0 {
            return false;
        }
        let prev = v.sel as i64;
        let mut sel = prev;
        match k {
            "up" | "k" => sel -= 1,
            "down" | "j" => sel += 1,
            "pgup" | "ctrl+u" => sel -= page,
            "pgdown" | "ctrl+d" => sel += page,
            "home" => sel = 0,
            "G" | "end" => sel = n - 1,
            _ => return false,
        }
        sel = sel.clamp(0, n - 1);
        let mut dir = if sel < prev || k == "G" || k == "end" {
            -1
        } else {
            1
        };
        if k == "home" {
            dir = 1;
        }
        v.sel = super::browse::nearest(&v.rows, sel as usize, dir);
        if v.sel as i64 != prev {
            v.sel_at = frame;
        }
        true
    }

    /// Moves the selection a row per notch. It stays in the list: up from
    /// the first search result does not fall into the search box.
    fn wheel(&mut self, up: bool) {
        if !self.step(if up { "up" } else { "down" }) {
            return;
        }
        let now = Instant::now();
        self.wheel_at = Some(now);
        if self.wheel_drawn.is_none_or(|d| now - d >= WHEEL_EVERY) {
            self.wheel_drawn = Some(now); // a lone notch shows at once
        } else {
            self.still = true;
            self.wheel_moved = true;
        }
        if self.wheel_tick {
            return;
        }
        self.wheel_tick = true;
        self.after(WHEEL_EVERY, Msg::Wheel);
    }

    /// Draws what the notches since the last frame moved, and once the
    /// wheel rests, loads the covers around where it stopped.
    fn wheel_frame(&mut self) {
        self.wheel_tick = false;
        if self.wheel_moved {
            self.wheel_moved = false;
            self.wheel_drawn = Some(Instant::now());
        } else {
            self.still = true;
        }
        if self.wheel_at.is_none_or(|a| a.elapsed() >= WHEEL_REST) {
            return self.prefetch_cur();
        }
        self.wheel_tick = true;
        self.after(WHEEL_EVERY, Msg::Wheel);
    }

    fn mouse(&mut self, ms: Mouse) {
        match ms.kind {
            MouseKind::WheelUp => self.wheel(true),
            MouseKind::WheelDown => self.wheel(false),
            MouseKind::Down => {
                if self.upd.is_some() {
                    return self.update_click(ms.x, ms.y);
                }
                if self.bar_asking() {
                    return self.bar_click(ms.x, ms.y);
                }
                if self.pick.is_some() {
                    return self.picker_click(ms.x, ms.y);
                }
                if self.opt_open {
                    return self.options_click(ms.x, ms.y);
                }
                if ms.button == Button::Left && self.geo.divider.has(ms.x, ms.y) {
                    self.dragging = true;
                    self.drag_off = ms.x - self.geo.divider.x0;
                    return;
                }
                self.click(ms);
            }
            MouseKind::Moved => {
                if self.full && self.viz_list {
                    let i = ms.y - self.geo.opt_row0;
                    if self.geo.options.has(ms.x, ms.y) && i >= 0 && (i as usize) < viz::NAMES.len()
                    {
                        self.viz_sel = i as usize;
                        self.preview_viz(i as usize);
                    }
                    return;
                }
                if self.dragging {
                    // the divider stays under the mouse where it grabbed it
                    let m = super::view::MARGIN;
                    let f =
                        (ms.x - self.drag_off + 1 - m) as f64 / (self.width - 2 * m).max(1) as f64;
                    self.set_split(f);
                }
            }
            MouseKind::Up => {
                if self.dragging {
                    self.dragging = false;
                    let s = self.shown_split(); // dragged past where the cover stops growing: where it stopped
                    self.set_split(s);
                    save_split(self.split);
                }
            }
        }
    }

    /// Maps a mouse click onto the layout recorded by the last render. A
    /// click plays a song or opens an item; right click goes back.
    fn click(&mut self, ms: Mouse) {
        if self.full {
            return self.full_click(ms);
        }
        if ms.button == Button::Right {
            return self.back();
        }
        if ms.button != Button::Left {
            return;
        }
        let (x, y) = (ms.x, ms.y);
        let g = self.geo.clone();
        for card in &g.cards {
            if card.r.has(x, y) {
                self.cur_mut().sel = card.index;
                return self.activate();
            }
        }
        if g.now_playing.has(x, y) {
            return self.key("c");
        }
        let searching = self.searching;
        self.searching = false; // a click anywhere but the box leaves it
        for (i, r) in g.tabs.iter().enumerate() {
            if r.has(x, y) {
                return self.switch_to(i);
            }
        }
        for f in &g.foot {
            if f.r.has(x, y) {
                if searching && f.action == "esc" {
                    return; // done: the click left the box
                }
                if searching && f.action == "enter" {
                    return self.run_search(true);
                }
                if f.action == "?" {
                    self.help = !self.help;
                    return;
                }
                return self.key(&f.action);
            }
        }
        if self.next_click(x, y) {
            return;
        }
        let has_song = !self.state.player.id.is_empty();
        let id = self.state.player.id.clone();
        if g.search.has(x, y) {
            self.searching = true;
        } else if g.crumb.has(x, y) {
            self.back();
        } else if g.play.has(x, y) {
            self.toggle();
        } else if g.prev.has(x, y) {
            self.go_back();
        } else if g.next.has(x, y) {
            self.skip();
        } else if g.shuffle.has(x, y) {
            self.key("s");
        } else if g.repeat.has(x, y) {
            self.key("r");
        } else if g.queue.has(x, y) {
            self.switch_to(SEC_QUEUE);
        } else if g.volume.has(x, y) {
            self.key("m");
        } else if g.artist.has(x, y) && has_song {
            self.lookup_id(ipc::CMD_ARTIST, &id);
        } else if g.album.has(x, y) && has_song {
            self.lookup_id(ipc::CMD_ALBUM, &id);
        } else if g.bar.has(x, y) && self.state.player.dur > 0.0 {
            let f = (x - g.bar.x0) as f64 / (g.bar.x1 - g.bar.x0 - 1).max(1) as f64;
            self.seek(f * self.state.player.dur);
        } else if g.list.has(x, y) {
            let frame = self.frame;
            let v = self.cur_mut();
            let idx = v.off + (y - g.list.y0) as usize;
            if idx >= v.rows.len() || !v.rows[idx].selectable() {
                return;
            }
            v.sel = idx;
            v.sel_at = frame;
            self.activate();
        }
    }

    /// Opens the Apple Music sign-in page in the browser.
    pub fn sign_in(&mut self) {
        self.task(async move {
            Some(Msg::Login(
                crate::login::run(|_| {}).await.map_err(|e| e.to_string()),
            ))
        });
    }

    /// Opens the sign-in page by itself the first time brumm finds it is
    /// signed out, so nobody has to hunt for the key.
    pub fn auto_login(&mut self) {
        if self.login_tried || self.state.status != ipc::STATUS_LOGGED_OUT || self.client.is_none()
        {
            return;
        }
        self.login_tried = true;
        self.set_flash("opening the Apple Music sign-in in your browser…");
        self.sign_in();
    }

    /// Adds the selected catalog song, album or playlist — or the song
    /// playing — to the library.
    fn add_to_library(&mut self) {
        let v = self.cur();
        let row = v.rows.get(v.sel);
        let (kind, id, name) = if let Some(t) = row.and_then(|r| r.track.as_ref()) {
            ("songs".to_string(), t.id.clone(), t.title.clone())
        } else if let Some(it) = row.and_then(|r| r.item.as_ref()) {
            if it.kind == apple::KIND_ARTIST {
                return self.set_flash("artists cannot be added; add one of their albums");
            }
            if !it.catalog {
                let msg = format!("{} is already in your library", it.name);
                return self.set_flash(&msg);
            }
            (format!("{}s", it.kind), it.id.clone(), it.name.clone())
        } else if !self.state.player.id.is_empty() {
            (
                "songs".to_string(),
                self.state.player.id.clone(),
                self.state.player.title.clone(),
            )
        } else {
            return;
        };
        if id.starts_with("i.") {
            return self.set_flash(&format!("{name} is already in your library"));
        }
        self.ask(
            Request {
                cmd: ipc::CMD_ADD.into(),
                list: kind,
                ids: vec![id],
                ..Default::default()
            },
            move |r| match r {
                Err(e) => Some(Msg::Err(e)),
                Ok(_) => Some(Msg::Flash(format!("added {name} to your library"))),
            },
        );
    }

    // ── playing at once ─────────────────────────────────────────────────
    // A play takes the player a moment — a license, the first bytes — so
    // the screen does not wait for it: the song shows as playing at once,
    // cover and all, and the player's reports keep showing it until they
    // catch up.

    /// Puts track on the stage now, as if it already played.
    pub fn show_playing(&mut self, t: Track, source: &str) {
        self.show_playing_at(t, source, -1);
    }

    pub fn show_playing_at(&mut self, t: Track, source: &str, index: i64) {
        self.pending = Some(PendingPlay {
            track: t,
            source: source.to_string(),
            index,
            at: Instant::now(),
        });
        self.want_play = Some(WantPlaying {
            playing: true,
            at: Instant::now(),
        }); // no flash of "paused" while it loads
        let mut st = self.state.clone();
        self.keep_showing(&mut st);
        self.state = st;
        self.state_at = Instant::now();
        self.maybe_fetch_cover();
    }

    /// Lays the pause or play asked for over a report that does not have
    /// it yet, for a few seconds at most.
    fn keep_playing(&mut self, st: &mut State) {
        let Some(w) = &self.want_play else { return };
        if st.player.playing == w.playing || w.at.elapsed() > Duration::from_secs(6) {
            self.want_play = None;
            return;
        }
        st.player.playing = w.playing;
    }

    /// Pauses or plays, shown at once.
    pub fn toggle(&mut self) {
        if !self.state.player.id.is_empty() {
            let playing = !self.state.player.playing;
            self.state.player.pos = self.position();
            self.state_at = Instant::now();
            self.state.player.playing = playing;
            self.want_play = Some(WantPlaying {
                playing,
                at: Instant::now(),
            });
        }
        self.send(Request::new(ipc::CMD_TOGGLE));
    }

    /// Keeps the button on pause through a change of song: the player
    /// reports "not playing" while it loads the next one.
    fn stay_playing(&mut self) {
        if self.state.player.playing {
            self.want_play = Some(WantPlaying {
                playing: true,
                at: Instant::now(),
            });
        }
    }

    /// Goes to the next song, which shows at once when the queue ahead is
    /// known.
    pub fn skip(&mut self) {
        self.stay_playing();
        if self.state.player.preview.is_none()
            && let Some(pos) = self.after_playing()
            && let Some(t) = self.up_track(pos)
        {
            let src = self.state.player.source.clone();
            self.show_playing_at(t, &src, pos);
        }
        self.send(Request::new(ipc::CMD_NEXT));
    }

    /// Goes to the previous song, or after 3 s back to the start of this
    /// one, as the player does; the playhead shows that at once.
    pub fn go_back(&mut self) {
        self.stay_playing();
        if self.state.player.dur > 0.0 && self.position() > 3.0 {
            self.seek_to = 0.0;
            self.seek_at = Some(Instant::now());
        }
        self.send(Request::new(ipc::CMD_PREV));
    }

    /// Plays the song at queue position pos, shown at once when known.
    pub fn jump(&mut self, pos: i64) {
        if let Some(t) = self.up_track(pos) {
            let src = self.state.player.source.clone();
            self.show_playing_at(t, &src, pos);
        }
        self.send(Request {
            cmd: ipc::CMD_JUMP.into(),
            value: pos as f64,
            ..Default::default()
        });
    }

    /// Lays the song being started over a report from the player that does
    /// not have it yet; once the player plays it, fails, or takes longer
    /// than a few seconds, its reports are the truth again.
    fn keep_showing(&mut self, st: &mut State) {
        let Some(p) = &self.pending else { return };
        if st.player.id == p.track.id
            || (!st.player.err.is_empty() && st.player.err != self.state.player.err)
            || p.at.elapsed() > Duration::from_secs(6)
        {
            self.pending = None;
            return;
        }
        let t = &p.track;
        let pl = &mut st.player;
        pl.id.clone_from(&t.id);
        pl.title.clone_from(&t.title);
        pl.artist.clone_from(&t.artist);
        pl.album.clone_from(&t.album);
        pl.dur = t.duration;
        pl.pos = 0.0;
        pl.source.clone_from(&p.source);
        pl.playing = true;
        pl.preview = None;
        if p.index >= 0 {
            pl.index = p.index;
        }
        if !t.artwork.is_empty() {
            pl.artwork.clone_from(&t.artwork);
        }
    }

    /// Tells the player which songs a list holds, so starting one skips
    /// the lookup at Apple; the first few hundred, which is what gets
    /// played.
    fn warm(&mut self, id: u64) {
        let Some(v) = self.view_by_id(id) else { return };
        let (mut ids, _) = v.songs();
        if ids.is_empty() || v.key == "queue:" {
            return;
        }
        ids.truncate(300);
        let Some(c) = self.client.clone() else { return };
        self.task(async move {
            let _ = c
                .request(Request {
                    cmd: ipc::CMD_WARM.into(),
                    ids,
                    ..Default::default()
                })
                .await;
            None
        });
    }

    /// The mouse over the fullscreen visualizer: its buttons, and the list
    /// of styles while it shows.
    fn full_click(&mut self, ms: Mouse) {
        if ms.button != Button::Left {
            return;
        }
        if self.viz_list {
            if !self.geo.options.has(ms.x, ms.y) {
                self.viz_list = false;
                return;
            }
            let i = ms.y - self.geo.opt_row0;
            if i >= 0 && (i as usize) < viz::NAMES.len() {
                self.pick_viz(i as usize);
            }
            return;
        }
        let foot = self.geo.foot.clone();
        for f in foot {
            if f.r.has(ms.x, ms.y) {
                self.full_key(&f.action);
                return;
            }
        }
    }

    /// The rate the fullscreen visualizer draws at.
    pub fn draw_fps(&self) -> i64 {
        match self.opts.o.viz_fps {
            r @ (60 | 120) => r,
            r if r == auto_fps() => self.refresh,
            _ => 30,
        }
    }

    /// Names the draw rate for the visualizer's button.
    pub fn fps_label(&self) -> String {
        if self.opts.o.viz_fps == auto_fps() {
            return format!("auto {} fps", self.refresh);
        }
        format!("{} fps", self.draw_fps())
    }
}

/// Selects the row playing id — or the item id — if the view lists it.
pub fn select_song(v: &mut View, id: &str, frame: i64) {
    if id.is_empty() {
        return;
    }
    if let Some(i) = v.rows.iter().position(|r| {
        r.track
            .as_ref()
            .is_some_and(|t| t.id == id || t.id.ends_with(id))
            || r.item.as_ref().is_some_and(|it| it.id == id)
    }) {
        v.sel = i;
        v.sel_at = frame;
    }
}

/// Maps 1–9 and 0 to the first ten styles.
pub fn viz_digit(k: &str) -> Option<usize> {
    let b = k.as_bytes();
    if b.len() != 1 || !b[0].is_ascii_digit() {
        return None;
    }
    let i = if b[0] == b'0' {
        9
    } else {
        (b[0] - b'1') as usize
    };
    (i < viz::NAMES.len()).then_some(i)
}

/// Moves through a choice by dir, wrapping.
pub fn step<T: PartialEq + Clone>(list: &[T], cur: T, dir: i64) -> T {
    let i = list.iter().position(|v| *v == cur).unwrap_or(0) as i64;
    let n = list.len() as i64;
    list[((i + dir).rem_euclid(n)) as usize].clone()
}
