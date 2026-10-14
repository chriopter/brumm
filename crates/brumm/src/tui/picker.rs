//! P: adds the selected song, album or playlist — or the song playing —
//! to a playlist, or to a new one named here.

use std::sync::Arc;

use super::browse::{addable, songs_of};
use super::model::{Key, Model, Msg, Row};
use super::text::*;
use super::view::{IC_PLAYLIST, MARGIN, SPINNER, box_lines, scroll};
use crate::ipc::{self, Request};
use crate::music::{Item, Track};

/// Rows the list of playlists shows at once.
const PICKER_ROWS: usize = 10;

/// Chooses the playlist songs go to, or names a new one.
#[derive(Clone, Debug, Default)]
pub struct Picker {
    /// What is being added.
    pub what: Row,
    /// Its name, for the title.
    pub name: String,
    pub lists: Vec<Item>,
    pub loading: bool,
    /// 0 is "new playlist".
    pub sel: usize,
    pub off: usize,
    pub naming: bool,
    pub input: String,
}

/// Takes the last word off s, as ctrl+w does.
pub fn drop_word(s: &str) -> String {
    let f = s.trim_end_matches(' ');
    match f.rfind(' ') {
        Some(i) => f[..=i].to_string(),
        None => String::new(),
    }
}

impl Model {
    pub fn open_picker(&mut self) {
        let v = self.cur();
        let row = v.rows.get(v.sel);
        let (what, name) = if let Some(t) = row.and_then(|r| r.track.as_ref()) {
            (row.cloned().unwrap_or_default(), t.title.clone())
        } else if let Some(it) = row.and_then(|r| r.item.as_ref()).filter(|it| addable(it)) {
            (row.cloned().unwrap_or_default(), it.name.clone())
        } else if !self.state.player.id.is_empty() {
            let st = &self.state.player;
            (
                Row {
                    track: Some(Arc::new(Track {
                        id: st.id.clone(),
                        title: st.title.clone(),
                        ..Default::default()
                    })),
                    ..Default::default()
                },
                st.title.clone(),
            )
        } else {
            return self.set_flash("select a song, album or playlist to add");
        };
        self.pick = Some(Picker {
            what,
            name,
            loading: true,
            ..Default::default()
        });
        self.opt_open = false;
        self.help = false;
        self.ask(
            Request {
                cmd: ipc::CMD_LIST.into(),
                list: ipc::LIST_PLAYLISTS.into(),
                ..Default::default()
            },
            |r| {
                Some(Msg::PickerLists(r.map(|m| {
                    m.items.into_iter().filter(|it| it.editable).collect()
                })))
            },
        );
    }

    pub fn picker_key(&mut self, msg: &Key) {
        let Some(p) = self.pick.as_mut() else { return };
        let k = msg.name.as_str();
        if p.naming {
            match k {
                "esc" => p.naming = false,
                "enter" => {
                    let name = p.input.trim().to_string();
                    if !name.is_empty() {
                        self.add_to_playlist(String::new(), name);
                    }
                }
                "backspace" => {
                    p.input.pop();
                }
                "ctrl+u" => p.input.clear(),
                "ctrl+w" => p.input = drop_word(&p.input),
                _ => p.input.push_str(&msg.text),
            }
            return;
        }
        let n = p.lists.len() + 1;
        match k {
            "up" | "k" => p.sel = (p.sel + n - 1) % n,
            "down" | "j" => p.sel = (p.sel + 1) % n,
            "enter" | "space" | " " | "l" => {
                let i = p.sel;
                self.pick_row(i);
            }
            "esc" | "q" | "P" | "h" => self.pick = None,
            _ => {}
        }
    }

    /// Acts on row i: 0 names a new playlist, the rest add to one.
    fn pick_row(&mut self, i: usize) {
        let Some(p) = self.pick.as_mut() else { return };
        if i == 0 {
            p.naming = true;
            p.input = p.name.clone();
            return;
        }
        if let Some(l) = p.lists.get(i - 1) {
            let (id, name) = (l.id.clone(), l.name.clone());
            self.add_to_playlist(id, name);
        }
    }

    /// Sends the picked songs to playlist id, or to a new one.
    fn add_to_playlist(&mut self, id: String, name: String) {
        let Some(p) = self.pick.take() else { return };
        self.set_flash(&format!("adding to {name}…"));
        let Some(c) = self.client.clone() else { return };
        self.task(async move {
            let ids = match songs_of(&c, &p.what).await {
                Ok(ids) => ids,
                Err(e) => return Some(Msg::Err(e)),
            };
            let n = ids.len();
            let mut req = Request {
                cmd: ipc::CMD_PLAYLIST.into(),
                start: id.clone(),
                ids,
                ..Default::default()
            };
            if id.is_empty() {
                req.query = name.clone();
            }
            if let Err(e) = c.request(req).await {
                return Some(Msg::Err(e.to_string()));
            }
            Some(Msg::Flash(if id.is_empty() {
                format!("made the playlist {name}")
            } else {
                format!("added {n} songs to {name}")
            }))
        });
    }

    /// The picker, for the overlay.
    pub fn picker_box(&mut self) -> Vec<L> {
        let frame = self.frame;
        let width = self.width;
        let Some(p) = self.pick.as_mut() else {
            return Vec::new();
        };
        let mut lines: Vec<L> = Vec::new();
        if p.naming {
            lines.push(one("name of the new playlist", DIM));
            lines.push(L::default());
            lines.push(line(vec![
                sp("▌", HERE),
                sp(format!(" {}", p.input), PLAIN),
                sp("▏", HERE),
            ]));
            lines.push(L::default());
            lines.push(line(vec![
                sp("enter", KEY),
                sp(" create  ", DIM),
                sp("esc", KEY),
                sp(" back", DIM),
            ]));
        } else {
            let n = p.lists.len() + 1;
            p.off = scroll(p.sel, p.off, PICKER_ROWS);
            for i in p.off..n.min(p.off + PICKER_ROWS) {
                let text = if i == 0 {
                    line(vec![sp("＋", HERE), sp(" new playlist…", PLAIN)])
                } else {
                    line(vec![
                        sp(IC_PLAYLIST, DIM),
                        sp(format!("  {}", p.lists[i - 1].name), PLAIN),
                    ])
                };
                let gutter = if i == p.sel {
                    line(vec![sp("▌", HERE), sp(" ", PLAIN)])
                } else {
                    plain("  ")
                };
                lines.push(cat(gutter, text));
            }
            if p.loading {
                lines.push(one(
                    format!(
                        "  {} loading your playlists",
                        SPINNER[(frame.rem_euclid(SPINNER.len() as i64)) as usize]
                    ),
                    DIM,
                ));
            } else if p.lists.is_empty() && self.state.service == "Spotify" {
                lines.push(one("  Spotify lets apps add only to your own", DIM));
                lines.push(one("  playlists, like a new one here", DIM));
            } else if p.lists.is_empty() {
                lines.push(one("  Apple lets apps add only to playlists", DIM));
                lines.push(one("  made with an app — like a new one here", DIM));
            }
            lines.push(L::default());
            lines.push(line(vec![
                sp("↑↓", KEY),
                sp(" choose  ", DIM),
                sp("enter", KEY),
                sp(" add  ", DIM),
                sp("esc", KEY),
                sp(" cancel", DIM),
            ]));
        }
        let wd = lines
            .iter()
            .map(|l| w(l) as i64 + 2)
            .fold(44, i64::max)
            .min(width - 2 * MARGIN - 4)
            .max(8);
        let lines: Vec<L> = lines
            .iter()
            .map(|l| cat(plain(" "), fit(l, wd as usize)))
            .collect();
        let n = lines.len() as i64;
        box_lines(
            &format!("add {} to", p.name),
            false,
            L::default(),
            lines,
            wd + 4,
            n + 2,
        )
    }

    /// Picks the row under the mouse; outside closes the picker.
    pub fn picker_click(&mut self, x: i64, y: i64) {
        if !self.geo.options.has(x, y) {
            self.pick = None;
            return;
        }
        let row0 = self.geo.opt_row0;
        let Some(p) = self.pick.as_mut() else { return };
        if p.naming {
            return;
        }
        let i = y - row0;
        if i >= 0 && (i as usize) < (p.lists.len() + 1).min(PICKER_ROWS) {
            p.sel = p.off + i as usize;
            let s = p.sel;
            self.pick_row(s);
        }
    }
}
