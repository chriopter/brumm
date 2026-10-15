//! The mouse pointer turns into a hand over what a click does something
//! to, and into a resize arrow over the divider (OSC 22: kitty, ghostty,
//! foot, wezterm; other terminals ignore it). The mouse is seen moving all
//! the time for it, so a bare move is cheap: it sets the pointer when the
//! shape changes and does nothing else, not even draw.

use super::model::Model;

pub const SHAPE_DEFAULT: &str = "";
pub const SHAPE_POINTER: &str = "pointer";
pub const SHAPE_RESIZE: &str = "ew-resize";

/// The escape sequence that sets the pointer's shape.
pub fn pointer_seq(shape: &str) -> String {
    format!("\x1b]22;{shape}\x07")
}

impl Model {
    /// The pointer shape over x, y in the layout the last render recorded.
    /// List rows keep the default: all of the list is clickable.
    pub fn shape_at(&self, x: i64, y: i64) -> String {
        let g = &self.geo;
        let shape = if self.fb.is_some()
            || self.upd.is_some()
            || self.pick.is_some()
            || self.opt_open
            || self.viz_list
        {
            if g.options.has(x, y) {
                SHAPE_POINTER
            } else {
                SHAPE_DEFAULT
            }
        } else if g.cards.iter().any(|c| c.r.has(x, y)) || g.foot.iter().any(|f| f.r.has(x, y)) {
            SHAPE_POINTER
        } else if self.full {
            SHAPE_DEFAULT
        } else if g.now_playing.has(x, y) {
            SHAPE_POINTER
        } else if g.divider.has(x, y) {
            SHAPE_RESIZE
        } else if g.tabs.iter().any(|r| r.has(x, y))
            || [
                g.search, g.crumb, g.play, g.prev, g.next, g.shuffle, g.repeat, g.queue, g.volume,
                g.bar, g.upsongs,
            ]
            .iter()
            .any(|r| r.has(x, y))
            || g.upnext.iter().any(|r| r.has(x, y))
            || (!self.state.player.id.is_empty() && (g.artist.has(x, y) || g.album.has(x, y)))
        {
            SHAPE_POINTER
        } else {
            SHAPE_DEFAULT
        };
        shape.to_string()
    }

    /// Sends shape to the terminal, or nothing when it shows already.
    pub fn set_pointer(&mut self, shape: String) {
        if shape == self.pointer {
            return;
        }
        let seq = pointer_seq(if shape.is_empty() { "default" } else { &shape });
        self.pointer = shape;
        self.raw(seq);
    }

    /// The default pointer back, for when brumm quits.
    pub fn pointer_reset(&self) -> String {
        if self.pointer.is_empty() {
            String::new()
        } else {
            pointer_seq("default")
        }
    }
}
