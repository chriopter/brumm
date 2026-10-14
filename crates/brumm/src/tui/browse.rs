//! Lists from the daemon as rows, and what acts on a row: ratings,
//! stations, the songs a row stands for.

use super::filter::apply_filter;
use super::model::{Model, Msg, Row, View};
use super::text::*;
use super::view::IC_DISLIKE;
use crate::ipc::{self, Client, Message, Request};
use crate::music::{
    self, DISLIKE, Item, KIND_ALBUM, KIND_ARTIST, KIND_PLAYLIST, KIND_SHELF, KIND_STATION,
    KIND_TERM, LOVE, Ref, Shelf,
};

/// Turns a daemon reply into rows: shelves become a heading and their
/// contents, plain lists stay plain. An album or playlist opens with its
/// facts and Apple's note above the songs.
pub fn fill(v: &mut View, mut reply: Message) {
    let explorer = reply.explorer.take();
    if let Some(page) = &explorer {
        if !page.title.is_empty() {
            v.title.clone_from(&page.title);
        }
        reply.shelves = page.shelves.clone();
        reply.items = page.items.clone();
        reply.tracks = page.tracks.clone();
        if !page.info.is_empty() {
            reply.shelves.insert(0, Shelf::titled(&page.info));
        }
        if !page.next.is_empty() {
            reply.items.push(Item {
                kind: KIND_SHELF.into(),
                id: page.next.clone(),
                name: "More".into(),
                route: page.next.clone(),
                catalog: true,
                ..Default::default()
            });
        }
    }
    let mut rows: Vec<Row> = Vec::new();
    if let Some(page) = &explorer
        && !page.details.is_empty()
    {
        rows.push(Row::item(Item {
            kind: KIND_SHELF.into(),
            id: format!("details:{}", v.key),
            name: "Details".into(),
            route: "action:details".into(),
            details: page.details.clone(),
            ..Default::default()
        }));
    }
    if let Some(it) = &v.item
        && (it.kind == KIND_ALBUM || it.kind == KIND_PLAYLIST)
    {
        if !it.info.is_empty() {
            rows.push(Row::note(it.info.clone()));
        }
        if !it.note.is_empty() {
            rows.push(Row::note(it.note.clone()));
        }
        if !rows.is_empty() {
            rows.push(Row::default());
        }
    }
    let first_album = reply.tracks.first().map(|t| t.album.clone());
    for sh in reply.shelves {
        if !rows.is_empty() {
            rows.push(Row::default());
        }
        rows.push(Row::head(sh.title));
        rows.extend(sh.tracks.into_iter().map(Row::track));
        rows.extend(sh.items.into_iter().map(Row::item));
    }
    rows.extend(reply.items.into_iter().map(Row::item));
    rows.extend(reply.tracks.into_iter().map(Row::track));
    v.loaded = true;
    v.err = None;
    if v.all.is_some() {
        // a filter shows: keep it over the new rows
        v.all = Some(rows.clone());
        v.rows = rows;
        apply_filter(v);
    } else {
        v.rows = rows;
    }
    v.qpos = reply.pos.max(0);
    // An album opened from a link has no name yet; its songs carry it.
    if let (Some(it), Some(album)) = (&v.item, first_album)
        && it.kind == KIND_ALBUM
        && v.title == "Album"
    {
        v.title = album;
    }
    v.sel = v.sel.min(v.rows.len().saturating_sub(1));
    v.sel = nearest(&v.rows, v.sel, 1);
}

/// The selectable row at i, or the next one in direction dir, or failing
/// that the next one the other way.
pub fn nearest(rows: &[Row], i: usize, dir: i64) -> usize {
    for d in [dir, -dir] {
        let mut j = i as i64;
        while j >= 0 && (j as usize) < rows.len() {
            if rows[j as usize].selectable() {
                return j as usize;
            }
            j += d;
        }
    }
    i.min(rows.len().saturating_sub(1))
}

/// The songs a row stands for: itself, or what is inside.
pub async fn songs_of(c: &Client, r: &Row) -> Result<Vec<String>, String> {
    if let Some(t) = &r.track {
        return Ok(vec![t.id.clone()]);
    }
    let Some(it) = r
        .item
        .as_deref()
        .filter(|it| it.kind != KIND_STATION && it.kind != KIND_TERM)
    else {
        return Err("nothing to add here".into());
    };
    let reply = c
        .request(Request {
            cmd: ipc::CMD_OPEN.into(),
            item: Some(it.clone()),
            ..Default::default()
        })
        .await
        .map_err(|e| e.to_string())?;
    let mut ids: Vec<String> = reply.tracks.iter().map(|t| t.id.clone()).collect();
    for s in &reply.shelves {
        ids.extend(s.tracks.iter().map(|t| t.id.clone()));
    }
    Ok(ids)
}

impl Model {
    /// What a rating applies to: the selected song, album, playlist or
    /// station, else the song playing.
    fn rateable(&self) -> Option<(Ref, String)> {
        let v = self.cur();
        if let Some(r) = v.rows.get(v.sel) {
            if let Some(t) = &r.track {
                return Some((
                    Ref {
                        kind: "song".into(),
                        id: t.id.clone(),
                    },
                    t.title.clone(),
                ));
            }
            if let Some(it) = &r.item
                && matches!(it.kind.as_str(), KIND_ALBUM | KIND_PLAYLIST | KIND_STATION)
            {
                return Some((
                    Ref {
                        kind: it.kind.clone(),
                        id: it.id.clone(),
                    },
                    it.name.clone(),
                ));
            }
        }
        let st = &self.state.player;
        if !st.id.is_empty() && st.preview.is_none() {
            return Some((
                Ref {
                    kind: "song".into(),
                    id: st.id.clone(),
                },
                st.title.clone(),
            ));
        }
        None
    }

    /// Toggles a love (LOVE) or a dislike (DISLIKE).
    pub fn rate(&mut self, value: i64) {
        if value == DISLIKE && self.lacking("dislike", "dislikes") {
            return;
        }
        let Some((r, name)) = self.rateable() else {
            return;
        };
        if r.kind != "song" && self.lacks("love-items") {
            let msg = format!(
                "{} loves songs only; press i to add it to your library",
                self.service()
            );
            return self.set_flash(&msg);
        }
        let value = if self.rating.get(&r.id).copied().unwrap_or(0) == value {
            0
        } else {
            value
        };
        if value == 0 {
            self.rating.remove(&r.id);
        } else {
            self.rating.insert(r.id.clone(), value);
        }
        let msg = match value {
            LOVE => format!("♥ {name}"),
            DISLIKE => format!("disliked {name}"),
            _ => format!("{name}: no rating"),
        };
        self.set_flash(&msg);
        self.send(Request {
            cmd: ipc::CMD_RATE.into(),
            refs: vec![r],
            value: value as f64,
            ..Default::default()
        });
    }

    /// Asks which songs and albums, playlists and stations of a view are
    /// loved or disliked.
    pub fn fetch_ratings(&mut self, id: u64) {
        let Some(v) = self.view_by_id(id) else { return };
        let mut refs = Vec::new();
        for r in &v.rows {
            if refs.len() >= 1000 {
                break;
            }
            if let Some(t) = &r.track {
                refs.push(Ref {
                    kind: "song".into(),
                    id: t.id.clone(),
                });
            } else if let Some(it) = &r.item
                && matches!(it.kind.as_str(), KIND_ALBUM | KIND_PLAYLIST | KIND_STATION)
            {
                refs.push(Ref {
                    kind: it.kind.clone(),
                    id: it.id.clone(),
                });
            }
        }
        if refs.is_empty() {
            return;
        }
        self.ask(
            Request {
                cmd: ipc::CMD_RATINGS.into(),
                refs,
                ..Default::default()
            },
            |r| r.ok().map(|m| Msg::Ratings(m.ratings)),
        );
    }

    /// The heart or thumb shown for id, one cell wide.
    pub fn rating_mark(&self, id: &str) -> L {
        match self.rating.get(id).copied().unwrap_or(0) {
            LOVE => one("♥", ERR),
            DISLIKE => one(IC_DISLIKE, DIM),
            _ => plain(" "),
        }
    }

    /// Starts a station: the selected one, or the one Apple makes from the
    /// selected song or artist, or from the song playing.
    pub fn play_station(&mut self) {
        let mut req = Request::new(ipc::CMD_STATION);
        let v = self.cur();
        let row = v.rows.get(v.sel);
        let station = row
            .and_then(|r| r.item.as_ref())
            .is_some_and(|it| it.kind == KIND_STATION);
        if !station && self.lacking("radio", "radio") {
            return; // a station there is plays; none is made
        }
        let v = self.cur();
        let row = v.rows.get(v.sel);
        if let Some(t) = row.and_then(|r| r.track.as_ref()) {
            req.start = t.id.clone();
        } else if let Some(it) = row
            .and_then(|r| r.item.as_ref())
            .filter(|it| it.kind == KIND_STATION || it.kind == KIND_ARTIST)
        {
            req.item = Some((**it).clone());
        } else if !self.state.player.id.is_empty() {
            req.start = self.state.player.id.clone();
        } else {
            return self.set_flash("select a song or an artist to start its station");
        }
        self.start_station(req);
    }

    pub fn start_station(&mut self, req: Request) {
        self.set_flash("tuning in…");
        self.ask(req, |r| match r {
            Err(e) => Some(Msg::Err(e)),
            Ok(m) => m
                .items
                .first()
                .map(|it| Msg::Flash(format!("▶ {}", it.name))),
        });
    }
}

/// The kinds a picked row can add to a playlist.
pub fn addable(it: &music::Item) -> bool {
    it.kind == KIND_ALBUM || it.kind == KIND_PLAYLIST
}
