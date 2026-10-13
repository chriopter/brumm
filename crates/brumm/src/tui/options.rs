//! The options menu (o): the few switches both players share, saved by
//! the daemon and pushed to every player, and the cover styles.

use serde_json::{Map, Value};

use super::model::{Model, Msg, step};
use super::text::*;
use super::view::box_lines;
use crate::config::Options;
use crate::ipc::{self, Request};
use crate::launch;

// Cover styles.
/// Dithered sextant pixel art in a few colors.
pub const COVER_PIXEL: &str = "pixel";
/// True-color half blocks.
pub const COVER_SMOOTH: &str = "smooth";
/// The real image, through the kitty graphics protocol.
pub const COVER_ORIGINAL: &str = "original";

pub const COVER_STYLES: [&str; 3] = [COVER_PIXEL, COVER_SMOOTH, COVER_ORIGINAL];

/// The fullscreen visualizer's frame rates; the loop draws up to 120. 0,
/// the default, is 30: plenty for character cells, and every frame costs
/// the terminal a whole screen. -1 is auto: the screen's own refresh rate.
pub const DRAW_RATES: [i64; 4] = [0, 60, 120, AUTO_FPS];
const AUTO_FPS: i64 = -1;
pub const MAX_DRAW_FPS: i64 = 120;

pub fn auto_fps() -> i64 {
    AUTO_FPS
}

/// The focused monitor's refresh rate, as Hyprland reports it, kept
/// between 30 and what the loop draws at most; 60 if unknown.
pub fn screen_refresh() -> i64 {
    #[derive(serde::Deserialize)]
    struct Mon {
        #[serde(default)]
        focused: bool,
        #[serde(default, rename = "refreshRate")]
        rate: f64,
    }
    let Ok(out) = std::process::Command::new("hyprctl")
        .args(["monitors", "-j"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
    else {
        return 60;
    };
    let Ok(mons) = serde_json::from_slice::<Vec<Mon>>(&out.stdout) else {
        return 60;
    };
    mons.iter()
        .find(|m| m.focused && m.rate > 0.0)
        .map(|m| (m.rate.round() as i64).clamp(30, MAX_DRAW_FPS))
        .unwrap_or(60)
}

/// The saved switches, with what the menu needs.
#[derive(Clone, Debug, Default)]
pub struct Opts {
    pub o: Options,
}

impl Opts {
    /// Reads them; switches reduce motion took over are written back as
    /// it, so the bar widget reads it too.
    pub fn load() -> Opts {
        let o = Options::load();
        if o.reduce_motion != o.no_bar_scroll {
            let _ = o.save();
        }
        Opts { o }
    }

    /// The style to draw covers in: original falls back to smooth in
    /// terminals without kitty graphics.
    pub fn cover(&self, has_kitty: bool) -> String {
        match self.o.cover.as_str() {
            COVER_ORIGINAL if !has_kitty => COVER_SMOOTH.into(),
            c @ (COVER_SMOOTH | COVER_ORIGINAL) => c.into(),
            _ => COVER_PIXEL.into(),
        }
    }

    /// The cover style picked in the menu, whatever this terminal shows.
    pub fn chosen(&self) -> &'static str {
        match self.o.cover.as_str() {
            COVER_SMOOTH => COVER_SMOOTH,
            COVER_ORIGINAL => COVER_ORIGINAL,
            _ => COVER_PIXEL,
        }
    }

    /// The bar switch until the shell answers: as last set or seen here,
    /// else on when the widget is installed.
    pub fn bar_known(&self) -> bool {
        if !self.o.bar.is_empty() {
            return self.o.bar == "on";
        }
        crate::update::plugin_linked()
    }
}

pub fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

// The menu's rows, in order: those the window's menu has too, in its
// order and words (gui/qml/Store.qml menuRows), the terminal's own before
// the top bar's.
pub const OPT_COLORS: usize = 0;
pub const OPT_AUTOPLAY: usize = 1;
pub const OPT_MOTION: usize = 2;
pub const OPT_COVER: usize = 3;
pub const OPT_KEYS: usize = 4;
pub const OPT_BAR: usize = 5;
pub const OPT_MUSIC_KEYS: usize = 6;
pub const NUM_OPTIONS: usize = 7;

const OPT_LABELS: [&str; NUM_OPTIONS] = [
    "Cover Colors",
    "Autoplay",
    "Reduce Motion",
    "Cover Style",
    "Show Shortcuts",
    "Show in Top Bar",
    "Music Keys",
];
const OPT_HINTS: [&str; NUM_OPTIONS] = [
    "Use the cover's colors for the app.",
    "Play on after the last song.",
    "No scrolling text, fewer wobbles.",
    "How album covers are drawn.",
    "Show keys on the buttons.",
    "The song in Omarchy's top bar.",
    "Super+Shift+M opens brumm, not Spotify.",
];

/// A pick of one, cur marked.
fn choices(list: &[&str], cur: &str) -> L {
    let mut out = L::default();
    for (i, v) in list.iter().enumerate() {
        if i > 0 {
            out.spans.push(spaces(3));
        }
        if *v == cur {
            out.spans.push(sp("●", HERE));
            out.spans.push(sp(format!(" {v}"), PLAIN));
        } else {
            out.spans.push(sp(format!("○ {v}"), DIM));
        }
    }
    out
}

impl Model {
    /// Explains row i; the cover's says what original needs where the
    /// terminal cannot show it.
    pub fn opt_hint(&self, i: usize) -> &'static str {
        if i == OPT_COVER && !self.has_kitty {
            return "Covers as pixels or smooth; original needs kitty or Ghostty.";
        }
        OPT_HINTS[i]
    }

    /// Names row i; the colors' by what they are now.
    pub fn opt_label(&self, i: usize) -> &'static str {
        if i == OPT_COLORS && self.opts.o.no_cover_colors {
            return "Theme Colors";
        }
        OPT_LABELS[i]
    }

    /// The top bar's rows only where omarchy can switch them.
    pub fn opt_shown(&self, i: usize) -> bool {
        match i {
            OPT_BAR => self.has_omarchy,
            OPT_MUSIC_KEYS => self.has_omarchy && crate::update::can_music_keys(),
            _ => true,
        }
    }

    /// Moves the selection by dir, over rows that do not show.
    fn step_option(&mut self, dir: i64) {
        loop {
            self.opt_sel = (self.opt_sel as i64 + dir).rem_euclid(NUM_OPTIONS as i64) as usize;
            if self.opt_shown(self.opt_sel) {
                return;
            }
        }
    }

    /// Keeps the options: the daemon saves them and tells every player;
    /// without one they go to the file here.
    pub fn save_options(&mut self) {
        let Some(c) = self.client.clone() else {
            let _ = self.opts.o.save();
            return;
        };
        // Only what changed, and in order: a switch set in the window
        // meanwhile is not overwritten by an old copy from here.
        let mut now = self.opts.o.fields();
        now.remove("bar"); // switched by set_bar alone: it can be refused
        let mut set = Map::new();
        for (k, v) in &now {
            if self.sent_opts.as_ref().is_none_or(|s| s.get(k) != Some(v)) {
                set.insert(k.clone(), v.clone());
            }
        }
        self.sent_opts = Some(now);
        if set.is_empty() {
            return;
        }
        if self.opt_queue.is_none() {
            let (tx, mut rx) = tokio::sync::mpsc::channel::<Map<String, Value>>(32);
            self.opt_queue = Some(tx);
            self.task(async move {
                while let Some(set) = rx.recv().await {
                    let _ = c
                        .request(Request {
                            cmd: ipc::CMD_OPTIONS.into(),
                            options: Some(set),
                            ..Default::default()
                        })
                        .await;
                }
                None
            });
        }
        if let Some(q) = &self.opt_queue {
            let _ = q.try_send(set); // 32 behind: the daemon is gone; the next start reads the file
        }
    }

    /// Takes options another player set.
    pub fn options_changed(&mut self, o: Options) {
        if o.cover != self.opts.o.cover {
            self.rendered.clear();
            self.thumbs.clear();
        }
        let mut sent = o.fields(); // what the daemon has now
        sent.remove("bar");
        self.sent_opts = Some(sent);
        if self.has_omarchy && !o.bar.is_empty() {
            self.bar_on = o.bar == "on";
        }
        self.opts.o = o;
    }

    /// Handles a key while the options are open; every key stays in the
    /// menu.
    pub fn options_key(&mut self, k: &str) {
        match k {
            "up" | "k" | "shift+tab" => self.step_option(-1),
            "down" | "j" | "tab" => self.step_option(1),
            "space" | " " | "enter" => self.change_option(self.opt_sel, 1),
            "right" | "l" | "left" | "h" => {
                // The cover style steps either way; a switch goes on to the
                // right, off to the left, whatever it was.
                let on = k == "right" || k == "l";
                if self.opt_sel == OPT_COVER || self.option_on(self.opt_sel) != on {
                    self.change_option(self.opt_sel, if on { 1 } else { -1 });
                }
            }
            "o" | "esc" | "q" => self.opt_open = false,
            _ => {}
        }
    }

    /// g: brumm goes over to its window, which is what it opens from now
    /// on; g there comes back here. The music plays on.
    pub fn open_window(&mut self) {
        if let Err(e) = launch::gui_path() {
            return self.set_flash(&e.to_string());
        }
        self.opts.o.start = launch::GUI.into();
        self.set_flash("opening the gui…");
        let (client, o, here) = (self.client.clone(), self.opts.o.clone(), self.here());
        self.task(async move {
            super::place::leave_place(client.as_ref(), &here).await; // the window opens where this was
            match &client {
                None => {
                    let _ = o.save();
                }
                Some(c) => {
                    let mut set = Map::new();
                    set.insert("start".into(), launch::GUI.into());
                    if let Err(e) = c
                        .request(Request {
                            cmd: ipc::CMD_OPTIONS.into(),
                            options: Some(set),
                            ..Default::default()
                        })
                        .await
                    {
                        return Some(Msg::Err(e.to_string()));
                    }
                }
            }
            if let Err(e) = launch::open(launch::GUI) {
                return Some(Msg::Err(e.to_string()));
            }
            Some(Msg::Quit)
        });
    }

    /// Whether switch i is on.
    pub fn option_on(&self, i: usize) -> bool {
        match i {
            OPT_COLORS => !self.opts.o.no_cover_colors,
            OPT_AUTOPLAY => !self.opts.o.no_autoplay,
            OPT_MOTION => self.opts.o.reduce_motion,
            OPT_KEYS => self.opts.o.always_tips,
            OPT_BAR => self.bar_on,
            OPT_MUSIC_KEYS => self.opts.o.music_keys,
            _ => false,
        }
    }

    /// Flips a switch, or steps a choice by dir.
    pub fn change_option(&mut self, i: usize, dir: i64) {
        match i {
            OPT_COVER => {
                self.opts.o.cover = step(&COVER_STYLES, self.opts.chosen(), dir).to_string();
                self.rendered.clear();
                self.thumbs.clear();
            }
            OPT_COLORS => self.opts.o.no_cover_colors = !self.opts.o.no_cover_colors, // the next frame picks them
            OPT_AUTOPLAY => {
                self.opts.o.no_autoplay = !self.opts.o.no_autoplay;
                let v = if self.opts.o.no_autoplay { 0.0 } else { 1.0 };
                self.send(Request {
                    cmd: ipc::CMD_AUTOPLAY.into(),
                    value: v,
                    ..Default::default()
                });
            }
            OPT_MOTION => self.opts.o.reduce_motion = !self.opts.o.reduce_motion, // the bar widget follows the saved file
            OPT_KEYS => self.opts.o.always_tips = !self.opts.o.always_tips,
            OPT_BAR => {
                let on = !self.bar_on;
                self.set_bar(on);
            }
            OPT_MUSIC_KEYS => {
                self.opts.o.music_keys = !self.opts.o.music_keys; // the daemon rebinds the keys
                if self.client.is_none() {
                    let _ = crate::update::set_music_keys(self.opts.o.music_keys);
                }
            }
            _ => {}
        }
        self.save_options();
    }

    /// The menu, drawn over the lower left corner: the rows, a gap, what
    /// the selected one does, the keys. opt_lines records which line each
    /// option is on, for the mouse.
    pub fn options_box(&mut self) -> Vec<L> {
        let check = |on: bool| {
            if on {
                one("󰄲", HERE)
            } else {
                one("󰄱", DIM)
            }
        };
        let mut values: Vec<L> = vec![L::default(); NUM_OPTIONS];
        values[OPT_COLORS] = choices(
            &["theme", "cover"],
            if self.option_on(OPT_COLORS) {
                "cover"
            } else {
                "theme"
            },
        );
        values[OPT_COVER] = choices(&COVER_STYLES, self.opts.chosen());
        for i in [OPT_AUTOPLAY, OPT_MOTION, OPT_KEYS, OPT_BAR, OPT_MUSIC_KEYS] {
            values[i] = check(self.option_on(i));
        }
        let keys = line(vec![
            sp("↑↓", KEY),
            sp(" move   ", DIM),
            sp("space", KEY),
            sp(" change   ", DIM),
            sp("esc", KEY),
            sp(" close", DIM),
        ]);
        let mut wd = w(&keys) as i64;
        for (i, v) in values.iter().enumerate() {
            if self.opt_shown(i) {
                wd = wd.max(16 + w(v) as i64).max(self.opt_hint(i).len() as i64);
            }
        }
        let wd = (wd + 3)
            .min(self.width - 2 * super::view::MARGIN - 6)
            .max(8); // as wide as the widest hint: it keeps its size
        let fw = (wd - 3) as usize;
        let mut lines: Vec<L> = Vec::new();
        self.opt_lines.clear();
        for (i, v) in values.into_iter().enumerate() {
            if !self.opt_shown(i) {
                self.opt_lines.push(-1);
                continue;
            }
            let gutter = if i == self.opt_sel {
                line(vec![sp("▌", HERE), spaces(2)])
            } else {
                plain("   ")
            };
            self.opt_lines.push(lines.len() as i64 + 1); // +1: the padding line
            lines.push(cat(
                gutter,
                fit(&cat(plain(format!("{:<16}", self.opt_label(i))), v), fw),
            ));
        }
        let hint = if self.opt_sel < NUM_OPTIONS {
            self.opt_hint(self.opt_sel)
        } else {
            ""
        };
        let mut all = vec![L::default()];
        all.extend(lines);
        all.push(L::default());
        all.push(cat(plain("   "), fit(&one(hint, DIM), fw)));
        all.push(cat(plain("   "), fit(&keys, fw)));
        all.push(L::default());
        let all: Vec<L> = all
            .iter()
            .map(|l| cat(plain(" "), fit(l, wd as usize)))
            .collect();
        let n = all.len() as i64;
        box_lines("options", false, L::default(), all, wd + 4, n + 2)
    }

    /// Changes the option under the mouse; a click outside closes the menu.
    pub fn options_click(&mut self, x: i64, y: i64) {
        if !self.geo.options.has(x, y) {
            self.opt_open = false;
            return;
        }
        let row = y - self.geo.opt_row0;
        if let Some(i) = self.opt_lines.iter().position(|&l| l >= 0 && l == row) {
            self.opt_sel = i;
            self.change_option(i, 1);
        }
    }
}
