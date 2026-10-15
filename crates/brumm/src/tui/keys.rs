//! Keytips: the keys drawn on the things they act on — tabs, the
//! transport, the artist and album, the selected row — when the options
//! say so. ? shows the full list either way.

use super::model::{Model, View};
use super::text::*;
use crate::music::{KIND_ALBUM, KIND_ARTIST, KIND_PLAYLIST, KIND_STATION, KIND_TERM};

/// A key badge at a column.
#[derive(Clone, Debug)]
pub struct Badge {
    pub col: i64,
    pub key: String,
}

impl Model {
    /// Whether keytips are drawn now. The key list and the fullscreen
    /// visualizer carry their own hints; the menus do not hide them, so
    /// switching them on in the options shows at once.
    pub fn show_tips(&self) -> bool {
        self.opts.o.always_tips && !self.full && !self.help
    }

    /// Lays key badges out on a line of w cells: each at its column,
    /// dropped where it would run into the one before or off the edge.
    pub fn badges(&self, wd: i64, mut bs: Vec<Badge>) -> L {
        bs.sort_by_key(|b| b.col);
        let mut out = L::default();
        let mut at = 0i64;
        for b in bs {
            let kw = sw(&b.key) as i64;
            if b.col < at || b.col + kw > wd {
                continue;
            }
            out.spans.push(spaces((b.col - at) as usize));
            out.spans.push(sp(b.key.clone(), KEY));
            at = (b.col + kw + 1).min(wd);
            if b.col + kw < wd {
                out.spans.push(sp(" ", PLAIN));
            }
        }
        out.spans.push(spaces((wd - at).max(0) as usize));
        out
    }

    /// The keys for the selected row, most useful first, for the list's
    /// bottom border.
    pub fn row_tips(&self, v: &View) -> L {
        let Some(r) = v.rows.get(v.sel).filter(|r| r.selectable()) else {
            return L::default();
        };
        let preview = if self.releases { "space" } else { "O" };
        let keys: Vec<(&str, &str)> = if v.key == "queue:" {
            vec![("enter", "play"), ("a A", "album artist"), ("*", "love")]
        } else if r.track.is_some() {
            let mut k = vec![
                ("enter", "play"),
                (preview, "preview"),
                ("z Z", "queue"),
                ("a A", "album artist"),
                ("R", "radio"),
                ("* d", "rate"),
                ("P", "playlist"),
                ("y", "link"),
            ];
            if self.capabilities.collection_write {
                k.insert(6, ("i", "library"));
            }
            k
        } else {
            let it = r
                .item
                .as_ref()
                .expect("a selectable row without a track has an item");
            match it.kind.as_str() {
                KIND_ALBUM | KIND_PLAYLIST => {
                    let mut k = vec![
                        ("enter", "open"),
                        ("z Z", "queue"),
                        ("* d", "rate"),
                        ("P", "playlist"),
                    ];
                    if it.catalog {
                        k.push(("i", "library"));
                    }
                    k
                }
                KIND_ARTIST => vec![("enter", "open"), ("R", "radio")],
                KIND_STATION => vec![("enter", "play"), ("* d", "rate")],
                KIND_TERM => vec![("enter", "search")],
                _ => vec![("enter", "open")],
            }
        };
        let mut out = L::default();
        let keys = keys
            .into_iter()
            .filter(|(_, action)| match *action {
                "preview" => self.capabilities.previews,
                "radio" => self.capabilities.stations,
                "rate" | "love" => {
                    self.capabilities.ratings || self.capabilities.preference.is_some()
                }
                "playlist" => self.capabilities.playlist_write,
                "library" => self.capabilities.collection_write,
                _ => true,
            })
            .collect::<Vec<_>>();
        for (i, (k, d)) in keys.iter().enumerate() {
            if i > 0 {
                out.spans.push(sp("  ", DIM));
            }
            out.spans.push(sp(*k, KEY));
            out.spans.push(sp(format!(" {d}"), DIM));
        }
        out
    }
}
