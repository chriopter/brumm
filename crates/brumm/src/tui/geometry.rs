//! Where things were drawn, for the mouse.

use super::model::NUM_SECTIONS;
use super::upnext::NEXT_MAX;

/// A clickable screen area, x1/y1 exclusive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x0: i64,
    pub y0: i64,
    pub x1: i64,
    pub y1: i64,
}

impl Rect {
    pub fn new(x0: i64, y0: i64, x1: i64, y1: i64) -> Rect {
        Rect { x0, y0, x1, y1 }
    }

    pub fn has(&self, x: i64, y: i64) -> bool {
        x >= self.x0 && x < self.x1 && y >= self.y0 && y < self.y1
    }
}

/// A footer entry and the key it stands for.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FootHit {
    pub r: Rect,
    pub action: String,
}

/// A card in a discovery grid and the row it stands for.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CardHit {
    pub r: Rect,
    pub index: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Geometry {
    pub list: Rect,
    pub crumb: Rect,
    pub search: Rect,
    pub divider: Rect,
    pub tabs: [Rect; NUM_SECTIONS],
    pub prev: Rect,
    pub play: Rect,
    pub next: Rect,
    pub shuffle: Rect,
    pub repeat: Rect,
    pub queue: Rect,
    pub volume: Rect,
    pub bar: Rect,
    /// Under the now-playing title.
    pub artist: Rect,
    pub album: Rect,
    /// The covers coming up.
    pub upnext: [Rect; NEXT_MAX],
    /// Or the songs, one row each.
    pub upsongs: Rect,
    /// The menu box over the screen.
    pub options: Rect,
    pub foot: Vec<FootHit>,
    pub cards: Vec<CardHit>,
    pub now_playing: Rect,
    /// The menu's first row.
    pub opt_row0: i64,
}
