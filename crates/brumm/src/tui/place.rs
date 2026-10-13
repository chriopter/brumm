//! A place is where a player is — the section, the views opened in it, the
//! selected row and the search — so that g opens the other one right
//! there. The daemon keeps it (CMD_PLACE) without reading it; the window
//! (gui/qml/Store.qml) writes and reads the same shape.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::model::{Model, NUM_SECTIONS, SEC_QUEUE, SEC_SEARCH, SECTION_NAMES, View, select_song};
use crate::apple::Item;
use crate::ipc::{self, Client, Request};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Place {
    pub section: i64,
    #[serde(rename = "sectionName", skip_serializing_if = "String::is_empty")]
    pub section_name: String,
    #[serde(rename = "playerView", skip_serializing_if = "std::ops::Not::not")]
    pub player_view: bool,
    /// The stack above the section's root.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<PlaceView>,
    pub sel: PlaceSel,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub query: String,
}

/// An opened view, enough to open it again.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PlaceView {
    pub key: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<Item>,
}

/// The selected row: by its song's or item's id, else its index.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PlaceSel {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub id: String,
    pub index: i64,
}

/// Tells the daemon where the player is, for the other one.
pub async fn leave_place(c: Option<&Client>, p: &Place) {
    let (Some(c), Ok(v)) = (c, serde_json::to_value(p)) else {
        return;
    };
    let _ = c
        .request(Request {
            cmd: ipc::CMD_PLACE.into(),
            place: Some(v),
            ..Default::default()
        })
        .await;
}

impl Model {
    /// The place the player is at.
    pub fn here(&self) -> Place {
        let mut p = Place {
            section: self.section as i64,
            section_name: SECTION_NAMES[self.section].into(),
            player_view: self.player_view,
            query: self.query.clone(),
            ..Default::default()
        };
        for v in &self.stack()[1..] {
            if let Some(it) = &v.item {
                p.views.push(PlaceView {
                    key: v.key.clone(),
                    title: v.title.clone(),
                    item: Some(it.clone()),
                });
            }
        }
        let v = self.cur();
        p.sel.index = v.sel as i64;
        if let Some(r) = v.rows.get(v.sel) {
            if let Some(t) = &r.track {
                p.sel.id.clone_from(&t.id);
            } else if let Some(it) = &r.item {
                p.sel.id.clone_from(&it.id);
            }
        }
        p
    }

    /// Asks the daemon for a place left by the other player, and takes it
    /// off, so a later start opens as ever.
    pub fn ask_place(&mut self) {
        let Some(c) = self.client.clone() else {
            self.placing = false;
            return;
        };
        self.task(async move {
            let p = match c.request(Request::new(ipc::CMD_PLACE)).await {
                Ok(m) => m.place.filter(|v| !v.is_null()),
                Err(_) => None,
            };
            if p.is_some() {
                let _ = c
                    .request(Request {
                        cmd: ipc::CMD_PLACE.into(),
                        place: Some(Value::Null),
                        ..Default::default()
                    })
                    .await;
            }
            Some(super::model::Msg::Place(p))
        });
    }

    /// Opens the place left, instead of going to what plays; with none,
    /// brumm resumes as ever.
    pub fn got_place(&mut self, p: Option<Value>) {
        self.placing = false;
        match p.and_then(|v| serde_json::from_value::<Place>(v).ok()) {
            None => self.maybe_resume(),
            Some(p) => {
                self.resumed = true;
                self.go_place(p);
            }
        }
    }

    /// Opens section, views and selection as p has them, loading what is
    /// not yet.
    pub fn go_place(&mut self, p: Place) {
        let mut s = p.section.clamp(0, NUM_SECTIONS as i64 - 1) as usize;
        if let Some(i) = SECTION_NAMES.iter().position(|n| *n == p.section_name) {
            s = i;
        }
        self.player_view = p.player_view;
        self.query.clone_from(&p.query);
        let q = p.query.trim();
        if q.chars().count() >= 2 {
            let v = self.view("Search", &format!("search:{q}"), None);
            self.stacks[SEC_SEARCH] = vec![v];
        }
        self.stacks[s].truncate(1);
        for pv in p.views {
            if let Some(it) = pv.item.filter(|_| !pv.key.is_empty()) {
                let v: View = self.view(&pv.title, &pv.key, Some(it));
                self.stacks[s].push(v);
            }
        }
        self.section = s;
        self.searching = false;
        self.filtering = false;
        let frame = self.frame;
        let top = self.cur_mut();
        top.sel = p.sel.index.max(0) as usize;
        top.want = p.sel.id;
        let top_id = top.id;
        let ids: Vec<u64> = self.stacks[s].iter().map(|v| v.id).collect();
        for id in ids {
            let Some(v) = self.view_by_id(id) else {
                continue;
            };
            if s == SEC_QUEUE {
                v.loaded = false; // always fresh
            }
            if !v.loaded {
                self.load(id);
            } else if id == top_id {
                let want = std::mem::take(&mut v.want);
                select_song(v, &want, frame);
            }
        }
    }
}
