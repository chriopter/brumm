//! The few things that say "this plays" — the meter's fill, the
//! selection's gutter, the playing row and its equalizer, the play button
//! — take the cover's most vivid color, made readable on the background
//! (brumm_shared::accent, the window's pick too). It is picked once per
//! cover and background; a frame only compares a key. Nothing playing, a
//! gray cover or the option off, they are the theme's own.

use brumm_shared::accent;
use ratatui::style::{Color, Style};

use super::model::Model;
use super::text::{HERE, PILL, PLAYS, bold};

/// What the styles were picked for.
#[derive(Clone, Debug, Default, PartialEq)]
struct AccentKey {
    /// The cover; empty for the theme's.
    url: String,
    /// Its image is here.
    ready: bool,
    bg: Option<[u8; 3]>,
}

/// The ready styles.
#[derive(Clone, Debug)]
pub struct Accent {
    key: AccentKey,
    /// The meter's fill, the selection's gutter.
    pub here: Style,
    /// The playing row, its equalizer, the play button's rim.
    pub plays: Style,
    /// plays, bold: the selected playing row.
    pub bold: Style,
    /// The play button.
    pub pill: Style,
}

impl Accent {
    /// The theme's own colors.
    pub fn theme() -> Accent {
        Accent {
            key: AccentKey::default(),
            here: HERE,
            plays: PLAYS,
            bold: bold(PLAYS),
            pill: PILL,
        }
    }
}

/// The background of the Omarchy theme the terminal wears, when there is
/// one: what accents must stand out from.
pub fn theme_background() -> Option<[u8; 3]> {
    let path = crate::config::home().join(".local/state/omarchy/current/theme/colors.toml");
    let text = std::fs::read_to_string(path).ok()?;
    for l in text.lines() {
        let Some((k, v)) = l.split_once('=') else {
            continue;
        };
        if k.trim() != "background" {
            continue;
        }
        let v = v.trim().trim_matches(['"', '\'']);
        let hex = v.strip_prefix('#')?;
        if hex.len() != 6 {
            return None;
        }
        let n = u32::from_str_radix(hex, 16).ok()?;
        return Some([(n >> 16) as u8, (n >> 8) as u8, n as u8]);
    }
    None
}

impl Model {
    /// Picks the accent again when the cover or the background changed;
    /// otherwise it is a comparison.
    pub fn sync_accent(&mut self) {
        let st = &self.state.player;
        let playing = !st.title.is_empty() || st.preview.is_some();
        let mut k = AccentKey::default();
        if playing && !self.opts.o.no_cover_colors && !self.cover_url.is_empty() {
            k = AccentKey {
                url: self.cover_url.clone(),
                ready: self.cover.is_some(),
                bg: self.term_bg,
            };
        }
        if k == self.acc.key {
            return;
        }
        self.acc = Accent::theme();
        self.acc.key = k.clone();
        if !k.ready {
            return;
        }
        let Some(img) = &self.cover else { return };
        let Some(c) = accent::vivid(
            img.as_raw(),
            img.width() as usize,
            img.height() as usize,
            img.width() as usize * 4,
        ) else {
            return; // a gray cover: the theme's
        };
        let c = accent::readable(c, k.bg);
        let s = Style::new().fg(Color::Rgb(c[0], c[1], c[2]));
        self.acc.here = s;
        self.acc.plays = s;
        self.acc.bold = bold(s);
        self.acc.pill = s.add_modifier(ratatui::style::Modifier::REVERSED);
    }
}
