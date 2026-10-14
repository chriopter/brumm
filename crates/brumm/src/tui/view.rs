//! Drawing the screen: the browser panel, the stage, the footer, the menus
//! over them, and the fullscreen visualizer.

use std::time::Instant;

use ratatui::buffer::Buffer;

use super::geometry::{FootHit, Geometry, Rect};
use super::model::Model;
use super::text::*;
use super::viz;
use crate::art::Size;
use crate::ipc;
use crate::music::{KIND_ALBUM, KIND_ARTIST, KIND_PLAYLIST, KIND_STATION, KIND_TERM};

// Nerd Font glyphs (Material Design), one cell each.
pub const IC_PREV: &str = "󰒮";
pub const IC_PLAY: &str = "󰐊";
pub const IC_PAUSE: &str = "󰏤";
pub const IC_NEXT: &str = "󰒭";
pub const IC_SHUFFLE: &str = "󰒝";
pub const IC_REPEAT: &str = "󰑖";
pub const IC_REPEAT_ONE: &str = "󰑘";
pub const IC_VOLUME: &str = "󰕾";
pub const IC_MUTED: &str = "󰖁";
pub const IC_PLAYLIST: &str = "󰲸";
pub const IC_ALBUM: &str = "󰀥";
pub const IC_ARTIST: &str = "󰠃";
pub const IC_SEARCH: &str = "󰍉";
pub const IC_HOME: &str = "󰋜";
pub const IC_SONG: &str = "󰎈";
pub const IC_QUEUE: &str = "󰐑";
pub const IC_STATION: &str = "󰐹";
pub const IC_SHELF: &str = "󰄨";
pub const IC_DISLIKE: &str = "󰔑";

pub fn navigation_icon(icon: &str) -> &'static str {
    match icon {
        "home" => IC_HOME,
        "explore" => "󰭎",
        "playlists" => IC_PLAYLIST,
        "albums" => IC_ALBUM,
        "artists" => IC_ARTIST,
        "songs" => IC_SONG,
        "radio" => IC_STATION,
        "search" => IC_SEARCH,
        "queue" => IC_QUEUE,
        _ => IC_SHELF,
    }
}

/// The glyph in front of an item of kind.
pub fn kind_icon(kind: &str) -> &'static str {
    match kind {
        KIND_PLAYLIST => IC_PLAYLIST,
        KIND_ALBUM => IC_ALBUM,
        KIND_ARTIST => IC_ARTIST,
        KIND_STATION => IC_STATION,
        KIND_TERM => IC_SEARCH,
        _ => IC_SHELF,
    }
}

pub const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Columns left and right of everything.
pub const MARGIN: i64 = 2;
/// One blank line above the panels.
pub const BODY_TOP: i64 = 1;
/// The narrowest the list can be dragged: less shows nothing worth reading.
const NAV_MIN: i64 = 16;
/// The list may take this much from the stage's controls.
const NAV_ROOM: i64 = 44;
/// Narrower, songs drop their lengths.
const DUR_MIN: i64 = 40;
/// Narrower, the stage gives way to the mini player.
const COL_MIN: i64 = 30;
/// The controls' width, however small the cover.
const CTL_MIN: i64 = 40;
const STAGE_PAD: i64 = 2;
/// List rows start this far below the panel's top border.
const LIST_TOP: i64 = 3;

/// One line of the stage column, with an optional click target resolved
/// once its screen position is known.
#[derive(Clone, Debug, Default)]
pub struct StageLine {
    pub text: L,
    pub center: bool,
    pub hit: Option<Hit>,
    /// Known display width; skips measuring long colored lines.
    pub width: usize,
}

impl StageLine {
    pub fn of(text: L) -> StageLine {
        StageLine {
            text,
            ..Default::default()
        }
    }
}

/// What a stage line's place makes clickable.
#[derive(Clone, Debug)]
pub enum Hit {
    /// The artist and album under the title.
    Meta { aw: i64, album: Option<i64> },
    /// The meter and the times under it.
    Bar { w: i64 },
    /// The transport row.
    Controls {
        w: i64,
        start: i64,
        gap: i64,
        tw: i64,
        vw: Option<i64>,
    },
    /// The row of covers coming up.
    UpNext { n: usize, tw: i64, th: i64 },
    /// The songs coming up.
    UpSongs { w: i64, n: i64 },
}

impl Model {
    /// How many list rows fit in the browser panel.
    pub fn list_rows(&self) -> i64 {
        (self.height - BODY_TOP - 3 - LIST_TOP - 1).max(1)
    }

    /// Draws the whole screen into buf.
    pub fn draw(&mut self, buf: &mut Buffer) {
        self.drawn = true;
        if self.full && self.width >= 20 && self.height >= 6 {
            // The browser is not on screen: nothing of it is drawn or wanted.
            self.marquee = false;
            self.eq_shown = false;
            self.card_shown = false;
            self.kitty_want.clear();
            self.fullscreen(buf);
            if self.viz_list {
                let b = self.viz_list_box();
                self.overlay(buf, &b);
            }
            return;
        }
        let lines = self.render();
        for (y, l) in lines.iter().enumerate() {
            put(buf, 0, y as i64, l);
        }
        let boxed = if self.state.auth.as_ref().is_some_and(|p| p.code.is_some()) {
            Some(self.auth_box())
        } else if self.provider_picker_open
            || self.state.status == crate::ipc::STATUS_CHOOSE_PROVIDER
        {
            Some(self.provider_box())
        } else if self.feature_form.is_some() {
            Some(self.feature_box())
        } else if self.fb.is_some() {
            Some(self.feedback_box())
        } else if self.upd.is_some() {
            Some(self.update_box())
        } else if self.bar_asking() {
            Some(self.bar_box())
        } else if self.pick.is_some() {
            Some(self.picker_box())
        } else if self.opt_open {
            Some(self.options_box())
        } else {
            None
        };
        if let Some(b) = boxed {
            self.overlay(buf, &b);
        }
    }

    pub fn render(&mut self) -> Vec<L> {
        self.geo = Geometry::default();
        self.marquee = false;
        self.eq_shown = false;
        self.card_shown = false;
        self.kitty_want.clear();
        if self.width < 40 || self.height < 14 {
            return vec![one("ʕ•ᴥ•ʔ brumm needs a bigger window", DIM)];
        }
        self.sync_accent();
        let inner = self.width - 2 * MARGIN;
        let body_h = self.height - BODY_TOP - 2; // blank line + footer

        // The divider sets the cover's size: the stage is the cover's
        // column and no wider. What the cover cannot use, held back by the
        // height, goes to the list; the controls under it keep CTL_MIN.
        let gap_w = 2 + 2 * STAGE_PAD;
        let pad = " ".repeat(MARGIN as usize);
        let mut lines: Vec<L> = vec![L::default()];
        let (nav_w, col_w, cover_h) = self.layout(inner, body_h);
        if self.discovering() {
            let dock_h = 8.min(body_h / 2);
            let nav = self.nav(inner, body_h - dock_h, MARGIN, BODY_TOP);
            let dock_w = if col_w == 0 { inner } else { col_w };
            let dock_x = inner - dock_w;
            let dock = self.stage(
                dock_w,
                0,
                dock_h,
                MARGIN + dock_x,
                BODY_TOP + body_h - dock_h,
            );
            for l in nav {
                lines.push(cat(plain(pad.clone()), l));
            }
            for l in dock {
                lines.push(cat(
                    plain(format!("{pad}{}", " ".repeat(dock_x.max(0) as usize))),
                    l,
                ));
            }
        } else if col_w == 0 {
            let mini = self.mini_player(inner);
            let nav = self.nav(inner, body_h - mini.len() as i64, MARGIN, BODY_TOP);
            for l in nav.into_iter().chain(mini) {
                lines.push(cat(plain(pad.clone()), l));
            }
        } else {
            let stage_x = MARGIN + nav_w + gap_w;
            self.geo.divider = Rect::new(
                MARGIN + nav_w - 1,
                BODY_TOP,
                MARGIN + nav_w + gap_w,
                BODY_TOP + body_h,
            );
            let stage = self.stage(col_w, cover_h, body_h, stage_x, BODY_TOP);
            // Both sides are exactly as wide as their column.
            let nav = self.nav(nav_w, body_h, MARGIN, BODY_TOP);
            let gap = " ".repeat(gap_w as usize);
            let n = nav.len().max(stage.len());
            for i in 0..n {
                let l = nav
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| plain(" ".repeat(nav_w as usize)));
                let r = stage
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| plain(" ".repeat(col_w as usize)));
                lines.push(cat(cat(cat(plain(pad.clone()), l), plain(gap.clone())), r));
            }
        }
        lines.push(L::default());
        lines.push(self.footer());
        lines
    }

    /// Splits inner cells between the list and the stage's column: the
    /// list's width, the column's, and the cover's height; col_w is 0 when
    /// only the mini player fits.
    pub fn layout(&self, inner: i64, body_h: i64) -> (i64, i64, i64) {
        let gap_w = 2 + 2 * STAGE_PAD;
        if inner - gap_w - NAV_ROOM < COL_MIN {
            return (inner, 0, 0);
        }
        // Where the divider asks to be, leaving the stage room for the controls.
        let nav_w = ((inner as f64 * self.split).round() as i64)
            .min(NAV_ROOM.max(inner - gap_w - CTL_MIN))
            .max(NAV_MIN);
        let room = inner - nav_w - gap_w;
        let cover_h = (body_h - 1 - super::player::STAGE_BELOW)
            .min((room as f64 / self.cell_aspect) as i64)
            .max(0);
        let cover_w = room.min((cover_h as f64 * self.cell_aspect).round() as i64);
        let col_w = room.min(cover_w.max(CTL_MIN));
        (inner - gap_w - col_w, col_w, cover_h)
    }

    // ── footer ──────────────────────────────────────────────────────────

    pub fn bear(&self) -> &'static str {
        if !self.state.player.playing {
            return "ʕ-ᴥ-ʔ";
        }
        ["ʕ•ᴥ•ʔ", "ʕ•ᴥ•ʔ♪", "ʕ·ᴥ·ʔ♫", "ʕ•ᴥ•ʔ♪"][((self.frame / 5) % 4) as usize]
    }

    pub fn footer(&mut self) -> L {
        // Each entry is also a button: a click does what the key does.
        let e = |k: &str, l: &str, a: &str| (k.to_string(), l.to_string(), a.to_string());
        let preview = if self.releases {
            e("space", "preview", "O")
        } else {
            e("O", "preview", "O")
        };
        let mut keys = vec![
            e("enter", "play", "enter"),
            e("/", "find", "/"),
            preview,
            e("o", "options", "o"),
            e("f", "visualizer", "f"),
            e("g", "gui", "g"),
            e("?", "keys", "?"),
            e("!", "feedback", "!"),
        ];
        if self.show_tips() {
            // the rest is on the buttons: the footer has what is not
            keys = vec![
                e("esc", "back", "esc"),
                e("f", "visualizer", "f"),
                e("c", "now playing", "c"),
                e("Q", "close, music plays on", "Q"),
                e("q", "quit", "q"),
                e("?", "all keys", "?"),
            ];
        }
        if self.searching {
            keys = vec![e("esc", "done", "esc"), e("enter", "search", "enter")]; // the box has the keys
        }
        if self.help {
            keys = vec![e("esc", "back", "esc"), e("?", "close the keys", "?")];
        }
        if self.state.status == ipc::STATUS_LOGGED_OUT {
            keys.insert(0, e("shift+L", "sign in", "L"));
        }
        let entry_w = |k: &(String, String, String)| sw(&k.0) + 1 + sw(&k.1);
        let width = |es: &[(String, String, String)]| {
            MARGIN as usize
                + 3
                + es.iter().map(entry_w).sum::<usize>()
                + 3 * es.len().saturating_sub(1)
        };
        // what does not fit is in the list; ? stays
        while keys.len() > 2 && width(&keys) as i64 > self.width - MARGIN - 10 {
            let last = keys.pop().expect("more than two keys");
            keys.pop();
            keys.push(last);
        }
        let mut left = vec![spaces((MARGIN + 3) as usize)];
        let mut x = MARGIN + 3;
        self.geo.foot.clear();
        for (i, k) in keys.iter().enumerate() {
            if i > 0 {
                left.push(spaces(3));
            }
            left.push(sp(k.0.clone(), KEY));
            left.push(sp(format!(" {}", k.1), DIM));
            let w = entry_w(k) as i64;
            self.geo.foot.push(FootHit {
                r: Rect::new(x, self.height - 1, x + w, self.height),
                action: k.2.clone(),
            });
            x += w + 3;
        }
        let left = line(left);

        let bear = one(self.bear(), HERE);
        let st = &self.state;
        let mut right = bear.clone();
        if !st.update.is_empty() && self.flash.is_empty() {
            right = line(vec![
                sp(format!("{} is available · ", st.update), DIM),
                sp("U", KEY),
                sp(" update", DIM),
                spaces(3),
            ]);
            right = cat(right, bear);
        } else if st.expires_in > 0 && self.flash.is_empty() {
            let msg = format!(
                "{} access in this build ends in {} days: press U to update",
                self.provider_name, st.expires_in
            );
            right = cat(line(vec![sp(msg, ERR), spaces(3)]), bear);
        } else if !self.flash.is_empty() {
            right = cat(line(vec![sp(self.flash.clone(), DIM), spaces(3)]), bear);
        } else if st.status == ipc::STATUS_LOGGED_OUT {
            right = cat(line(vec![sp("not signed in", ERR), spaces(3)]), bear);
        } else if st.status == ipc::STATUS_ERROR {
            right = cat(line(vec![sp(st.message.clone(), ERR), spaces(3)]), bear);
        } else if st.status == ipc::STATUS_STARTING {
            let msg = format!(
                "{} {}",
                SPINNER[(self.frame % SPINNER.len() as i64) as usize],
                st.message
            );
            right = cat(line(vec![sp(msg, DIM), spaces(3)]), bear);
        }
        let room = self.width - MARGIN - w(&left) as i64 - 3;
        let rw = w(&right) as i64;
        if rw > room {
            right = trunc_left(&right, (rw - room.max(0)) as usize, "…");
        }
        let gap = (self.width - MARGIN - w(&left) as i64 - w(&right) as i64).max(1);
        cat(cat(left, plain(" ".repeat(gap as usize))), right)
    }

    // ── browser panel ───────────────────────────────────────────────────

    pub fn nav(&mut self, wd: i64, h: i64, x: i64, y: i64) -> Vec<L> {
        let inner = wd - 3; // between the borders, less one cell of right padding
        let tx0 = x + 3; // text column: under the breadcrumb, after the gutter

        // Tabs: the active one where-you-are colored, the rest dim; spaced
        // tighter when the list is narrow, and only icons for the inactive
        // ones when even that does not fit.
        let nav_names = self.navigation[..self.sec("queue")]
            .iter()
            .map(|e| e.label.as_str())
            .collect::<Vec<_>>();
        self.geo.tabs = vec![Rect::default(); self.navigation.len()];
        let names_w: i64 = nav_names.iter().map(|n| sw(n) as i64).sum();
        let digits = if self.show_tips() {
            2 * nav_names.len() as i64
        } else {
            0
        };
        let n = nav_names.len() as i64 - 1;
        let (sep, icons) = if names_w + digits + 3 * n <= inner - 2 {
            (3, false)
        } else if names_w + digits + 2 * n <= inner - 2 {
            (2, false)
        } else {
            (2, true)
        };
        let mut tabs: Vec<L> = Vec::new();
        let mut tx = tx0;
        for (i, name) in nav_names.iter().enumerate() {
            if i == self.sec("search") && tx + 15 < x + wd - 1 {
                let style = if self.player_view { bold(HERE) } else { DIM };
                tabs.push(one("♪ Now Playing", style));
                self.geo.now_playing = Rect::new(tx, y + 1, tx + 13, y + 2);
                tx += 13 + sep;
            }
            let active = i == self.section;
            let name = if icons && !active {
                navigation_icon(&self.navigation[i].icon)
            } else {
                name
            };
            let mut label = one(name, if active { bold(HERE) } else { DIM });
            let mut w0 = sw(name) as i64;
            if digits > 0 {
                label = cat(
                    line(vec![
                        sp(((b'1' + i as u8) as char).to_string(), KEY),
                        sp(" ", PLAIN),
                    ]),
                    label,
                );
                w0 += 2;
            }
            self.geo.tabs[i] = Rect::new(tx, y + 1, tx + w0, y + 2);
            tabs.push(label);
            tx += w0 + sep;
        }
        let mut tab_line = plain("  ");
        for (i, t) in tabs.into_iter().enumerate() {
            if i > 0 {
                tab_line.spans.push(spaces(sep as usize));
            }
            tab_line = cat(tab_line, t);
        }
        let mut lines: Vec<L> = vec![tab_line, L::default()];

        let mut rows = h - 2 - lines.len() as i64 - 1;
        let mut top = y + 1 + lines.len() as i64;
        let (is_search_root, has_filter) = (
            self.section == self.sec("search") && self.stack().len() == 1,
            self.filtering || self.cur().all.is_some(),
        );
        if is_search_root {
            lines.push(cat(plain("  "), self.search_box(inner - 2)));
            lines.push(L::default());
            self.geo.search = Rect::new(tx0, top, x + wd - 1, top + 1);
            rows -= 2;
            top += 2;
        } else if has_filter {
            lines.push(cat(plain("  "), self.filter_box(inner - 2)));
            lines.push(L::default());
            rows -= 2;
            top += 2;
        }

        let mut head: Option<usize> = None;
        let v = self.cur();
        let (err, loaded, key, nrows) = (v.err.clone(), v.loaded, v.key.clone(), v.rows.len());
        let msg = |s: L| centered(s, inner, rows);
        if err.is_some() && nrows == 0 {
            lines.extend(msg(one(err.unwrap_or_default(), ERR)));
        } else if !loaded && self.state.status == ipc::STATUS_LOGGED_OUT {
            lines.extend(msg(one("sign in to see your library", DIM)));
        } else if !loaded {
            lines.extend(msg(one(
                format!("{}  loading", SPINNER[(self.frame % 10) as usize]),
                DIM,
            )));
        } else if key == "search:" {
            lines.extend(msg(line(vec![
                sp(
                    format!("search {}: type and press ", self.provider_name),
                    DIM,
                ),
                sp("enter", KEY),
            ])));
        } else if nrows == 0 {
            lines.extend(msg(one("nothing here", DIM)));
        } else {
            head = Some(lines.len()); // list rows below are fitted as they are built
            if self.discovering() {
                let d = self.discovery_rows(inner, rows, x + 1, top);
                lines.extend(d);
            } else {
                let v = self.cur_mut();
                v.off = scroll(v.sel, v.off, rows.max(0) as usize);
                let (off, n) = (v.off, v.rows.len());
                self.geo.list = Rect::new(x + 1, top, x + wd - 1, top + rows);
                for i in off..off + rows.max(0) as usize {
                    if i >= n {
                        break;
                    }
                    let l = self.row(i, inner);
                    lines.push(fit(&l, inner as usize));
                }
            }
        }

        // Breadcrumb in the top border; position in the bottom one.
        let mut crumb: Vec<String> = self.stack().iter().map(|v| v.title.clone()).collect();
        // Too long, it gives up its first parts: … / Deftones / Koi No Yokan.
        if sw(&crumb.join(" / ")) as i64 > wd - 8 {
            for k in 1..crumb.len().saturating_sub(1) {
                let mut c = vec!["…".to_string()];
                c.extend_from_slice(&crumb[k..]);
                if k == crumb.len() - 2 || sw(&c.join(" / ")) as i64 <= wd - 8 {
                    crumb = c;
                    break;
                }
            }
        }
        let title = crumb.join(" / ");
        if crumb.len() > 1 {
            let parent = crumb[..crumb.len() - 1].join(" / ");
            self.geo.crumb = Rect::new(tx0, y, tx0 + sw(&parent) as i64, y + 1);
        }
        let v = self.cur();
        let pos = if !v.rows.is_empty() && v.loaded {
            format!("{} of {}", v.sel + 1, v.rows.len())
        } else {
            String::new()
        };
        let head = head.unwrap_or(lines.len());
        for l in lines.iter_mut().take(head) {
            *l = fit(l, inner as usize);
        }
        let mut label = one(pos.clone(), DIM);
        if self.show_tips() {
            let t = self.row_tips(self.cur());
            if w(&t) > 0 {
                label = cat(t, one(format!("   {pos}"), DIM));
            }
        }
        box_lines(&title, crumb.len() > 1, label, lines, wd, h)
    }

    pub fn search_box(&self, w0: i64) -> L {
        let q = &self.query;
        if q.is_empty() && !self.searching {
            return line(vec![
                sp(format!("{IC_SEARCH}  search {}  ", self.provider_name), DIM),
                sp("/", KEY),
            ]);
        }
        let shown = trunc_left(
            &plain(q.clone()),
            (sw(q) as i64 - (w0 - 5)).max(0) as usize,
            "…",
        );
        let mut l = cat(line(vec![sp(IC_SEARCH, HERE), sp("  ", PLAIN)]), shown);
        if self.searching {
            l.spans.push(sp("▏", HERE));
        }
        l
    }

    /// One list line: the selection gutter, the text (the selected row
    /// scrolls when it does not fit) and a right-aligned detail.
    fn row(&mut self, i: usize, w0: i64) -> L {
        let v = self.cur();
        let r = v.rows[i].clone();
        if !r.head.is_empty() {
            return cat(
                plain("  "),
                fit(&one(r.head.clone(), bold(MUSIC)), (w0 - 2).max(0) as usize),
            );
        }
        if !r.note.is_empty() {
            return cat(
                plain("  "),
                fit(&one(r.note.clone(), DIM), (w0 - 2).max(0) as usize),
            );
        }
        if !r.selectable() {
            return L::default();
        }
        let sel = i == v.sel;
        let (sel_at, view_item) = (v.sel_at, v.item.clone());
        let gutter = if sel {
            line(vec![sp("▌", self.acc.here), sp(" ", PLAIN)])
        } else {
            plain("  ")
        };
        let (mut text, detail);
        if let Some(t) = &r.track {
            // Fixed columns on the right — heart, duration — so hearts and
            // times line up whatever else a row shows. The playing song has
            // its little equalizer in front of the title.
            let playing = !t.id.is_empty() && t.id == self.state.player.id;
            let title = if playing {
                let style = if sel { self.acc.bold } else { self.acc.plays };
                self.eq_shown = true;
                cat(
                    self.mini_eq(),
                    line(vec![sp(" ", PLAIN), sp(t.title.clone(), style)]),
                )
            } else if sel {
                one(t.title.clone(), BOLD)
            } else {
                plain(t.title.clone())
            };
            text = cat(
                title.clone(),
                line(vec![sp("  ", PLAIN), sp(t.artist.clone(), DIM)]),
            );
            if let Some(vi) = &view_item
                && vi.kind == KIND_ALBUM
            {
                let mut n = t.number;
                if n == 0 {
                    // cached before numbers were kept: count
                    n = self.cur().rows[..=i]
                        .iter()
                        .filter(|r| r.track.is_some())
                        .count() as i64;
                }
                text = cat(one(format!("{n:2}  "), DIM), title);
                if t.artist != vi.artist {
                    // compilations, features
                    text.spans.push(sp("  ", PLAIN));
                    text.spans.push(sp(t.artist.clone(), DIM));
                }
            }
            let mut d = self.rating_mark(&t.id);
            if w0 >= DUR_MIN {
                // a narrow list keeps the titles, not the times
                d.spans.push(sp("  ", PLAIN));
                d.spans.push(sp(format!("{:>5}", clock(t.duration)), DIM));
            }
            detail = d;
        } else {
            let it = r
                .item
                .as_ref()
                .expect("a selectable row without a track has an item");
            let icon = kind_icon(&it.kind);
            text = if sel {
                line(vec![
                    sp(icon, HERE),
                    sp("  ", PLAIN),
                    sp(it.name.clone(), BOLD),
                ])
            } else {
                line(vec![
                    sp(icon, DIM),
                    sp("  ", PLAIN),
                    sp(it.name.clone(), PLAIN),
                ])
            };
            if !it.artist.is_empty() {
                text.spans.push(sp("  ", PLAIN));
                text.spans.push(sp(it.artist.clone(), DIM));
            }
            let src = &self.state.player.source;
            let playing = it.key() == *src
                || (it.kind == KIND_STATION && *src == format!("station:{}", it.id));
            let mut d = self.rating_mark_for(&it.kind, &it.id);
            if playing && !self.state.player.title.is_empty() {
                d = cat(line(vec![sp("♪", self.acc.plays), sp(" ", PLAIN)]), d);
            }
            detail = d;
        }
        let avail = (w0 - 2 - 2 - w(&detail) as i64).max(0);
        if sel && !self.opts.o.reduce_motion {
            self.marquee = self.marquee || w(&text) as i64 > avail;
            text = marquee(&text, avail as usize, self.frame - sel_at);
        } else {
            text = trunc(&text, avail as usize);
        }
        cat(
            cat(cat(gutter, pad(text, avail as usize)), plain("  ")),
            detail,
        )
    }

    /// The playing row's four-bar equalizer: the real spectrum, or four
    /// bars wobbling on their own while none streams. Paused, the bars rest
    /// low and dim.
    fn mini_eq(&mut self) -> L {
        if !self.state.player.playing {
            return one("▁▂▁▃", DIM);
        }
        // The bars move at most every EQ_EVERY: frames in between keep them.
        let now = Instant::now();
        if self.eq_bars.is_empty() || self.eq_at.is_none_or(|a| now - a >= EQ_EVERY) {
            self.eq_bars = self.eq_levels(now);
            self.eq_at = Some(now);
        }
        one(self.eq_bars.clone(), self.acc.plays)
    }

    /// The four bars now.
    fn eq_levels(&self, now: Instant) -> String {
        const RAMP: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        let t = now.saturating_duration_since(self.start).as_secs_f64();
        let n = self.spec.len();
        (0..4)
            .map(|i| {
                let lv = if n > 0 {
                    self.spec[(n - 1).min([1, n / 5, n * 2 / 5, n * 3 / 5][i])]
                } else {
                    // Two sines per bar at unrelated speeds never quite repeat.
                    let f = [2.1, 3.3, 2.7, 3.9][i];
                    0.5 + 0.3 * (t * f + i as f64 * 1.7).sin()
                        + 0.2 * (t * f * 2.3 + i as f64).sin()
                };
                RAMP[((lv * 8.0) as i64).clamp(0, 7) as usize]
            })
            .collect()
    }

    // ── stage ───────────────────────────────────────────────────────────

    /// The column col_w wide, a line per row.
    pub fn stage(&mut self, col_w: i64, cover_h: i64, h: i64, x: i64, y: i64) -> Vec<L> {
        let st = &self.state;
        let playing = !st.player.title.is_empty() || st.player.preview.is_some();
        let mut top: Vec<StageLine> = Vec::new();
        let mut bottom: Option<Vec<StageLine>> = None;
        let add = |top: &mut Vec<StageLine>, l: L| top.push(StageLine::of(l));
        if self.help {
            top = self.help_lines();
        } else if st.status == ipc::STATUS_LOGGED_OUT {
            add(&mut top, one("ʕ•ᴥ•ʔ", HERE));
            add(&mut top, L::default());
            add(
                &mut top,
                one(format!("Sign in to {}", self.provider_name), BOLD),
            );
            add(
                &mut top,
                line(vec![
                    sp("it opens in your browser; ", DIM),
                    sp("shift+L", KEY),
                    sp(" opens it again", DIM),
                ]),
            );
        } else if !playing && let Some(card) = self.idle_card(col_w, cover_h) {
            top = card;
        } else if !playing {
            add(&mut top, one("ʕ-ᴥ-ʔ zZ", HERE));
            add(&mut top, L::default());
            add(&mut top, one("nothing playing", BOLD));
            add(
                &mut top,
                line(vec![sp("pick something and press ", DIM), sp("enter", KEY)]),
            );
        } else {
            let (t, b) = self.now_playing(col_w, cover_h);
            top = t;
            bottom = Some(b);
        }

        let blank = plain(" ".repeat(col_w.max(0) as usize));
        let mut out: Vec<L> = vec![blank; h.max(0) as usize];
        let mut hits: Vec<(Hit, i64, i64)> = Vec::new();
        let mut put_line = |out: &mut Vec<L>, row: i64, l: &StageLine| {
            if row < 0 || row >= h {
                return;
            }
            let (text, left) = place_at(l, col_w);
            out[row as usize] = text;
            if let Some(hit) = &l.hit {
                hits.push((hit.clone(), x + left, y + row));
            }
        };
        match bottom {
            None => {
                // Idle and help: centered in the stage.
                let start = if self.help {
                    1
                } else {
                    ((h - top.len() as i64) / 2).max(0)
                };
                for (i, l) in top.iter().enumerate() {
                    put_line(&mut out, start + i as i64, l);
                }
            }
            Some(bottom) => {
                // Playing: title and artist above the cover, progress and
                // controls below, the group centered so the cover sits in
                // the middle.
                let (head, rest) = if bottom.len() > 3 {
                    (bottom[..3].to_vec(), bottom[3..].to_vec())
                } else {
                    (bottom, Vec::new())
                };
                let mut group = head;
                let had_top = !top.is_empty();
                group.extend(top);
                if had_top {
                    group.push(StageLine::default());
                }
                group.extend(rest);
                let free = h - group.len() as i64;
                group.extend(self.up_next(col_w, free)); // room to spare: what comes next
                let start = ((h - group.len() as i64) / 2).max(0); // the cover sits in the vertical middle
                for (i, l) in group.iter().enumerate() {
                    put_line(&mut out, start + i as i64, l);
                }
            }
        }
        for (hit, hx, hy) in hits {
            self.apply_hit(hit, hx, hy);
        }
        out
    }

    fn apply_hit(&mut self, hit: Hit, x: i64, y: i64) {
        match hit {
            Hit::Meta { aw, album } => {
                self.geo.artist = Rect::new(x, y, x + aw, y + 1);
                if let Some(end) = album {
                    self.geo.album = Rect::new(x + aw + 5, y, x + end, y + 1);
                }
            }
            Hit::Bar { w: wd } => self.geo.bar = Rect::new(x, y, x + wd, y + 2), // the times row seeks too
            Hit::Controls {
                w: wd,
                start,
                gap,
                tw,
                vw,
            } => {
                let cell = |c: i64, r: i64| Rect::new(x + c - r, y, x + c + 1 + r, y + 1);
                if let Some(vw) = vw {
                    self.geo.shuffle = Rect::new(x, y, x + 2, y + 1);
                    self.geo.repeat = Rect::new(x + 2, y, x + 5, y + 1);
                    self.geo.queue = Rect::new(x + wd - vw - 1, y, x + wd - vw + 2, y + 1);
                    self.geo.volume = Rect::new(x + wd - vw + 2, y, x + wd, y + 1);
                }
                self.geo.prev = cell(start, 1);
                self.geo.play = Rect::new(x + start + 1 + gap, y, x + start + 1 + gap + 5, y + 1);
                self.geo.next = cell(start + tw - 1, 1);
            }
            Hit::UpNext { n, tw, th } => {
                for j in 0..n {
                    let x0 = x + j as i64 * (tw + 2);
                    self.geo.upnext[j] = Rect::new(x0, y, x0 + tw, y + th);
                }
            }
            Hit::UpSongs { w: wd, n } => self.geo.upsongs = Rect::new(x, y, x + wd, y + n),
        }
    }

    /// The stage's cover at size; dim darkens it, to sit behind a card
    /// (kitty images cannot be dimmed and stay as they are).
    pub fn cover_lines(&mut self, size: Size, dim: bool) -> Vec<L> {
        let Some(cover) = self.cover.clone() else {
            return super::cover::placeholder(size, ""); // while the cover downloads, same footprint
        };
        let style = self.opts.cover(self.has_kitty);
        if style == super::options::COVER_ORIGINAL
            && let Some(lines) = self.kitty_lines(&self.cover_url.clone(), size)
        {
            return lines;
        }
        let key = (size, dim);
        if let Some(l) = self.rendered.get(&key) {
            return l.clone();
        }
        let img = if dim {
            if self.cover_dim.is_none() {
                self.cover_dim = Some(std::sync::Arc::new(crate::art::dimmed(&cover)));
            }
            self.cover_dim.clone().expect("just made")
        } else {
            cover
        };
        let lines = super::cover::draw_cover(&style, &img, size);
        if self.rendered.len() > 8 {
            self.rendered.clear();
        }
        self.rendered.insert(key, lines.clone());
        lines
    }

    fn help_lines(&self) -> Vec<StageLine> {
        let sections = format!("← → tab 1–{}", self.navigation.len());
        let filtering = format!(
            "filter the list; tab searches all of {} (or paste a link)",
            self.provider_name
        );
        let groups: [(&str, &[(&str, &str)]); 4] = [
            (
                "browse",
                &[
                    ("↑↓ jk", "move"),
                    ("enter l", "open / play"),
                    ("esc h", "back"),
                    (&sections, "sections"),
                    ("/", &filtering),
                    ("a A", "the song's album / artist (or click them)"),
                    ("c", "go to what's playing"),
                ],
            ),
            (
                "play",
                &[
                    ("space", "play / pause"),
                    ("hold space  O", "preview the selected song"),
                    ("n p", "next / previous (p restarts after 3 s)"),
                    ("R", "radio: a station from the song or artist"),
                    ("z Z", "add to queue / play next"),
                    ("shift ← →", "seek 10 s"),
                    ("s", "shuffle"),
                    ("r", "repeat off / all / one"),
                    ("+ - m", "volume, mute"),
                ],
            ),
            (
                "library",
                &[
                    ("* d", "love / dislike"),
                    ("i", "add to your library"),
                    ("P", "add to a playlist, or a new one"),
                    ("y", "copy the song's link"),
                ],
            ),
            (
                "brumm",
                &[
                    ("?", "this list (keys on the buttons: in the options)"),
                    ("o", "options: covers, autoplay and more"),
                    (
                        "f",
                        "fullscreen visualizer (tab: next, v: all styles, a: auto-change, F: frame rate)",
                    ),
                    ("[ ]", "narrower / wider list"),
                    ("g", "go over to the gui (and g there comes back)"),
                    ("Q", "close, music keeps playing"),
                    ("q", "quit: stop the music"),
                    ("shift+L", "sign in again"),
                    ("U", "look for an update, install it"),
                    ("!", "send feedback: a new issue on GitHub"),
                ],
            ),
        ];
        let mut out = Vec::new();
        for (i, (name, keys)) in groups.iter().enumerate() {
            if i > 0 {
                out.push(StageLine::default());
            }
            out.push(StageLine::of(one(*name, BOLD)));
            for (k, d) in keys.iter() {
                out.push(StageLine::of(line(vec![
                    sp(format!("{k:<13}"), KEY),
                    sp("  ", PLAIN),
                    sp(*d, DIM),
                ])));
            }
        }
        out.push(StageLine::default());
        out.push(StageLine::of(one(
            "mouse: click to open or play, right click to go back",
            DIM,
        )));
        out
    }

    /// The two-line player for narrow windows.
    fn mini_player(&self, wd: i64) -> Vec<L> {
        let st = &self.state.player;
        if st.title.is_empty() {
            return vec![L::default(), one("nothing playing", DIM)];
        }
        let icon = if st.playing {
            sp(IC_PAUSE, self.acc.plays)
        } else {
            sp(IC_PLAY, DIM)
        };
        let l = line(vec![
            icon,
            sp("  ", PLAIN),
            sp(st.title.clone(), BOLD),
            sp(format!("  {}", st.artist), DIM),
        ]);
        vec![
            L::default(),
            trunc(&l, wd.max(0) as usize),
            self.progress(wd),
        ]
    }

    // ── the fullscreen visualizer ───────────────────────────────────────

    /// The visualizer over the whole terminal with one quiet line of what
    /// is playing.
    fn fullscreen(&mut self, buf: &mut Buffer) {
        // The stage's spectrum is tuned hot for a few rows; spread it out so
        // full-height bars mean loud, not merely audible.
        self.viz_spec.clear();
        self.viz_spec.extend(self.spec.iter().map(|v| v * v * v));
        let h = (self.height - 1).max(0) as usize;
        // A new style dissolves in over the old one, block by block.
        let f = self.viz_at.elapsed().as_secs_f64() / super::model::VIZ_FADE.as_secs_f64();
        let from = match self.viz_prev {
            Some(p) if f < 1.0 => Some(p),
            _ => {
                self.viz_prev = None;
                None
            }
        };
        let f = if from.is_some() { f } else { 1.0 };
        let playing = self.state.player.playing;
        self.viz.render_mix(
            from,
            self.viz_style,
            f,
            &self.viz_spec,
            &self.wave,
            self.width as usize,
            h,
            playing,
        );
        self.viz.blit(buf, 0, 0);
        let status = self.viz_status_line(h as i64);
        put(buf, 0, h as i64, &status);
    }

    /// The line under the visualizer at row h: what plays, and the
    /// buttons, each one a click target too.
    fn viz_status_line(&mut self, h: i64) -> L {
        let st = &self.state.player;
        let pos = self.position();
        let info = if st.title.is_empty() {
            one("nothing playing", DIM)
        } else {
            line(vec![
                sp(st.title.clone(), BOLD),
                sp(
                    format!("  {}  ·  {} / {}", st.artist, clock(pos), clock(st.dur)),
                    DIM,
                ),
            ])
        };
        // The buttons on the right: each one a click target too.
        let auto = if self.opts.o.no_viz_cycle {
            sp("○", DIM)
        } else {
            sp("●", HERE)
        };
        let fps = self.fps_label();
        let buttons: Vec<(String, L, String)> = vec![
            (
                "tab".into(),
                one(viz::NAMES[self.viz_style], DIM),
                "tab".into(),
            ),
            ("v".into(), one("styles", DIM), "v".into()),
            ("a".into(), line(vec![sp("auto ", DIM), auto]), "a".into()),
            ("F".into(), one(fps, DIM), "F".into()),
            ("f".into(), one("close", DIM), "f".into()),
        ];
        let parts: Vec<L> = buttons
            .iter()
            .map(|(k, l, _)| cat(line(vec![sp(k.clone(), KEY), sp(" ", PLAIN)]), l.clone()))
            .collect();
        let mut hint = L::default();
        for (i, p) in parts.iter().enumerate() {
            if i > 0 {
                hint.spans.push(spaces(3));
            }
            hint = cat(hint, p.clone());
        }
        if !self.flash.is_empty() {
            hint = cat(one(format!("{}   ", self.flash), DIM), hint);
        }
        let gap = (self.width - 4 - w(&info) as i64 - w(&hint) as i64).max(1);
        let status = cat(
            plain("  "),
            trunc(
                &cat(cat(info.clone(), plain(" ".repeat(gap as usize))), hint),
                (self.width - 4).max(0) as usize,
            ),
        );
        self.geo = Geometry::default(); // the browser is not on screen: nothing of it is clickable
        let mut x = 2 + w(&info) as i64 + gap;
        if !self.flash.is_empty() {
            x += sw(&self.flash) as i64 + 3;
        }
        for (i, b) in buttons.iter().enumerate() {
            let pw = w(&parts[i]) as i64;
            if x + pw <= self.width - 2 {
                self.geo.foot.push(FootHit {
                    r: Rect::new(x, h, x + pw, h + 1),
                    action: b.2.clone(),
                });
            }
            x += pw + 3;
        }
        status
    }

    /// The list of styles, over the visualizer's lower left.
    fn viz_list_box(&self) -> Vec<L> {
        let mut lines: Vec<L> = Vec::new();
        for (i, name) in viz::NAMES.iter().enumerate() {
            let gutter = if i == self.viz_sel {
                line(vec![sp("▌", HERE), sp(" ", PLAIN)])
            } else {
                plain("  ")
            };
            let dot = if i == self.viz_style {
                sp("●", HERE)
            } else {
                sp("○", DIM)
            };
            let digit = if i < 10 {
                ((i + 1) % 10).to_string()
            } else {
                " ".into()
            }; // 1–9 and 0 pick the first ten
            lines.push(cat(
                gutter,
                line(vec![
                    sp(digit, KEY),
                    sp("  ", PLAIN),
                    dot,
                    sp(format!(" {name}"), PLAIN),
                ]),
            ));
        }
        let auto = if self.opts.o.no_viz_cycle {
            "off"
        } else {
            "on"
        };
        lines.push(L::default());
        lines.push(line(vec![
            sp("enter", KEY),
            sp(" keep  ", DIM),
            sp("esc", KEY),
            sp(" back  ", DIM),
            sp("a", KEY),
            sp(format!(" auto: {auto}  "), DIM),
            sp("v", KEY),
            sp(" close", DIM),
        ]));
        let wd = lines.iter().map(w).max().unwrap_or(0);
        let lines: Vec<L> = lines
            .iter()
            .map(|l| cat(plain(" "), fit(l, wd + 1)))
            .collect();
        let n = lines.len() as i64;
        box_lines("styles", false, L::default(), lines, wd as i64 + 5, n + 2)
    }

    /// Lays a menu box over the lower left of the screen, its bottom on the
    /// blank line above the footer, and records where its rows are for the
    /// mouse.
    pub fn overlay(&mut self, buf: &mut Buffer, b: &[L]) {
        let bw = b.first().map(w).unwrap_or(0) as i64;
        let y0 = (self.height - 1 - b.len() as i64).max(0);
        self.geo.options = Rect::new(MARGIN, y0, MARGIN + bw, y0 + b.len() as i64);
        self.geo.opt_row0 = y0 + 1;
        for (i, l) in b.iter().enumerate() {
            let y = y0 + i as i64;
            if y >= self.height {
                break;
            }
            clear_span(buf, MARGIN, y, bw);
            put(buf, MARGIN, y, l);
        }
    }
}

/// How often the playing row's equalizer moves.
pub const EQ_EVERY: std::time::Duration = std::time::Duration::from_millis(90);

/// Scrolls text that does not fit: it rests, glides to the end, rests
/// again, and starts over.
pub fn marquee(s: &L, wd: usize, t: i64) -> L {
    let over = w(s) as i64 - wd as i64;
    if over <= 0 {
        return s.clone();
    }
    const REST: i64 = 8; // ticks of 100 ms
    let step = (t - REST).max(0) % (over + 2 * REST);
    let off = step.min(over) as usize;
    cut(s, off, off + wd)
}

/// A stage line laid out in its column, col_w wide, and where in the
/// column its text starts.
pub fn place_at(l: &StageLine, col_w: i64) -> (L, i64) {
    let col_w = col_w.max(0) as usize;
    let (text, wd) = if l.width == 0 || l.width > col_w {
        let t = trunc(&l.text, col_w);
        let wd = w(&t);
        (t, wd)
    } else {
        (l.text.clone(), l.width)
    };
    let left = if l.center {
        col_w.saturating_sub(wd) / 2
    } else {
        0
    };
    let mut out = plain(" ".repeat(left));
    out = cat(out, text);
    out.spans.push(spaces(col_w.saturating_sub(left + wd)));
    (out, left as i64)
}

/// A rounded panel of exactly w×h cells: the breadcrumb set into the top
/// border (all but its last part dimmed) and a label in the bottom one.
/// Lines start right after the left border; they carry their own gutter.
pub fn box_lines(title: &str, crumbed: bool, label: L, lines: Vec<L>, wd: i64, h: i64) -> Vec<L> {
    let wd = wd.max(4);
    let t = trunc(&plain(title.to_string()), (wd - 8).max(1) as usize);
    let tt: String = t.spans.iter().map(|s| s.content.as_ref()).collect();
    let tl = match tt.rfind(" / ") {
        Some(i) if crumbed => line(vec![
            sp(tt[..i + 3].to_string(), DIM),
            sp(tt[i + 3..].to_string(), BOLD),
        ]),
        _ => one(tt.clone(), BOLD),
    };
    let top = cat(cat(one("╭─ ", DIM), tl), plain(" "));
    let fill = (wd - 1 - w(&top) as i64).max(0) as usize;
    let mut out = vec![cat(top, one(format!("{}╮", "─".repeat(fill)), DIM))];
    let inner = (wd - 3) as usize;
    let side = sp("│", DIM);
    for i in 0..(h - 2).max(0) as usize {
        // Lines come fitted to inner; an empty one is a blank row.
        let l = lines
            .get(i)
            .filter(|l| w(l) > 0)
            .cloned()
            .unwrap_or_else(|| plain(" ".repeat(inner)));
        let mut row = L::from(vec![side.clone()]);
        row = cat(row, l);
        row.spans.push(sp(" ", PLAIN));
        row.spans.push(side.clone());
        out.push(row);
    }
    if w(&label) > 0 {
        let lab = cat(
            cat(plain(" "), trunc(&label, (wd - 8).max(0) as usize)),
            plain(" "),
        );
        let fill = (wd - 3 - w(&lab) as i64).max(0) as usize;
        out.push(cat(
            cat(one(format!("╰{}", "─".repeat(fill)), DIM), lab),
            one("─╯", DIM),
        ));
    } else {
        out.push(one(format!("╰{}╯", "─".repeat((wd - 2) as usize)), DIM));
    }
    out
}

pub fn centered(s: L, wd: i64, rows: i64) -> Vec<L> {
    let mut out: Vec<L> = (0..(rows / 2 - 1).max(0)).map(|_| L::default()).collect();
    out.push(cat(
        plain(" ".repeat((wd - w(&s) as i64).max(0) as usize / 2)),
        s,
    ));
    out
}

/// Keeps sel inside a window of rows starting at off.
pub fn scroll(sel: usize, off: usize, rows: usize) -> usize {
    if sel < off {
        sel
    } else if sel >= off + rows {
        (sel + 1).saturating_sub(rows)
    } else {
        off
    }
}

pub fn clock(sec: f64) -> String {
    let s = sec.max(0.0) as i64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Writes a line into buf at (x, y), clipped to the buffer.
pub fn put(buf: &mut Buffer, x: i64, y: i64, l: &L) {
    let area = buf.area;
    if y < 0 || y >= area.height as i64 || x >= area.width as i64 {
        return;
    }
    buf.set_line(x as u16, y as u16, l, area.width - x as u16);
}

/// Blanks n cells of row y of buf from x on.
pub fn clear_span(buf: &mut Buffer, x: i64, y: i64, n: i64) {
    let area = buf.area;
    if y < 0 || y >= area.height as i64 {
        return;
    }
    for cx in x.max(0)..(x + n).min(area.width as i64) {
        buf[(cx as u16, y as u16)].reset();
    }
}
