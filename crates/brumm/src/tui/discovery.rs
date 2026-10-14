//! Explore and artist pages as a grid of cards rather than a list, and
//! Explore's artwork shelves, which arrive after its first links.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use super::browse::fill;
use super::geometry::{CardHit, Rect};
use super::model::{ExploreLoad, Model, Msg, Row, View};
use super::text::*;
use crate::ipc::{self, Message, Request};
use crate::music::{ExplorePage, KIND_ALBUM, KIND_ARTIST, KIND_PLAYLIST, Shelf};

/// Each Explore load's number, so a late group for an older one is dropped.
static LOADS: AtomicU64 = AtomicU64::new(0);

impl Model {
    /// Whether the view on screen is a grid of cards.
    pub fn discovering(&self) -> bool {
        let v = self.cur();
        if self.player_view || v.rows.is_empty() {
            return false;
        }
        if let Some(it) = &v.item {
            if it.kind == KIND_ALBUM || it.kind == KIND_PLAYLIST {
                return false;
            }
            if it.kind == KIND_ARTIST {
                return true;
            }
        }
        self.section == self.sec("explore") && v.rows.iter().all(|r| r.track.is_none())
    }

    /// Shelves filling the panel w×h at x, y; each card keeps the row it
    /// stands for, for the mouse.
    pub fn discovery_rows(&mut self, wd: i64, h: i64, x: i64, y: i64) -> Vec<L> {
        let cols = (wd / 32).max(2) as usize;
        let cw = ((wd - (cols as i64 - 1) * 3) / cols as i64).max(1) as usize;
        let v = self.cur();
        let sel = v.sel;
        let mut lines: Vec<L> = Vec::new();
        let mut positions: HashMap<usize, Rect> = HashMap::new();
        let mut pending: Vec<usize> = Vec::new();
        let flush =
            |pending: &mut Vec<usize>, lines: &mut Vec<L>, positions: &mut HashMap<usize, Rect>| {
                if pending.is_empty() {
                    return;
                }
                let (mut titles, mut subs) = (L::default(), L::default());
                for (col, &i) in pending.iter().enumerate() {
                    let r: &Row = &v.rows[i];
                    let (mut title, mut sub) = (String::new(), String::new());
                    if let Some(it) = &r.item {
                        title.clone_from(&it.name);
                        sub = if it.artist.is_empty() {
                            "Explore ›".into()
                        } else {
                            it.artist.clone()
                        };
                    }
                    if let Some(t) = &r.track {
                        title.clone_from(&t.title);
                        sub.clone_from(&t.artist);
                    }
                    let (style, mark) = if i == sel {
                        (bold(HERE), "› ")
                    } else {
                        (MUSIC, "  ")
                    };
                    if col > 0 {
                        titles.spans.push(spaces(3));
                        subs.spans.push(spaces(3));
                    }
                    titles = cat(titles, fit(&one(format!("{mark}{title}"), style), cw));
                    subs = cat(subs, fit(&one(format!("  {sub}"), DIM), cw));
                    let x0 = x + (col * (cw + 3)) as i64;
                    positions.insert(
                        i,
                        Rect::new(
                            x0,
                            lines.len() as i64,
                            x0 + cw as i64,
                            lines.len() as i64 + 2,
                        ),
                    );
                }
                lines.push(titles);
                lines.push(subs);
                lines.push(L::default());
                pending.clear();
            };
        for (i, r) in v.rows.iter().enumerate() {
            if r.selectable() {
                pending.push(i);
                if pending.len() == cols {
                    flush(&mut pending, &mut lines, &mut positions);
                }
                continue;
            }
            flush(&mut pending, &mut lines, &mut positions);
            let text = if r.head.is_empty() { &r.note } else { &r.head };
            if !text.is_empty() {
                lines.push(one(text.clone(), BOLD));
                lines.push(L::default());
            }
        }
        flush(&mut pending, &mut lines, &mut positions);
        let selected = positions.get(&sel).copied().unwrap_or_default();
        let h = h.max(0);
        let mut off = (self.cur().off as i64)
            .min((lines.len() as i64 - h).max(0))
            .max(0);
        if selected.y0 < off {
            off = selected.y0;
        }
        if selected.y1 > off + h {
            off = selected.y1 - h;
        }
        let off = off.max(0);
        self.cur_mut().off = off as usize;
        let mut cards: Vec<CardHit> = positions
            .into_iter()
            .filter(|(_, r)| r.y0 >= off && r.y1 <= off + h)
            .map(|(i, r)| CardHit {
                r: Rect::new(r.x0, y + r.y0 - off, r.x1, y + r.y1 - off),
                index: i,
            })
            .collect();
        cards.sort_by_key(|c| c.index);
        self.geo.cards.extend(cards);
        let end = (lines.len() as i64).min(off + h) as usize;
        let start = (off as usize).min(end);
        lines[start..end]
            .iter()
            .map(|l| fit(l, wd.max(0) as usize))
            .collect()
    }

    /// Asks for Explore's artwork shelves, each group a request of its own,
    /// once its first links are shown.
    pub fn load_explore_visuals(&mut self, id: u64, page: ExplorePage) {
        let Some(c) = self.client.clone() else { return };
        let load = LOADS.fetch_add(1, Ordering::Relaxed) + 1;
        let visuals: Vec<_> = page.visuals.clone();
        let Some(v) = self.view_by_id(id) else { return };
        v.explore = Some(ExploreLoad {
            id: load,
            page,
            groups: vec![Vec::new(); visuals.len()],
        });
        for (i, link) in visuals.into_iter().enumerate() {
            let (c, query) = (c.clone(), link.route);
            self.task(async move {
                let mut shelves = Vec::new();
                if let Ok(data) = c
                    .request(Request {
                        cmd: ipc::CMD_EXPLORE.into(),
                        query,
                        ..Default::default()
                    })
                    .await
                    && let Some(page) = data.explorer
                {
                    for mut sh in page.shelves {
                        sh.items.truncate(6);
                        sh.tracks.truncate(6);
                        shelves.push(sh);
                    }
                    if !page.items.is_empty() {
                        let title = link.name;
                        shelves.push(Shelf {
                            title,
                            items: page.items.into_iter().take(6).collect(),
                            ..Default::default()
                        });
                    }
                }
                Some(Msg::ExploreGroup {
                    view: id,
                    load,
                    group: i,
                    shelves,
                })
            });
        }
    }
}

/// Lays a group of Explore's shelves in, above its links, keeping the
/// selected row and the scroll.
pub fn merge_explore_group(v: &mut View, group: usize, shelves: Vec<Shelf>) {
    let Some(load) = v.explore.as_mut() else {
        return;
    };
    if group >= load.groups.len() {
        return;
    }
    load.groups[group] = shelves;
    let mut page = ExplorePage {
        title: load.page.title.clone(),
        details: load.page.details.clone(),
        next: load.page.next.clone(),
        ..Default::default()
    };
    for g in &load.groups {
        page.shelves.extend(g.iter().cloned());
    }
    page.shelves.extend(load.page.shelves.clone());
    page.shelves.push(Shelf {
        title: "Browse".into(),
        items: load.page.items.clone(),
        ..Default::default()
    });
    page.tracks = load.page.tracks.clone();
    let selected = v.rows.get(v.sel).cloned();
    let off = v.off;
    fill(
        v,
        Message {
            explorer: Some(page),
            ..Default::default()
        },
    );
    if let Some(s) = selected
        && let Some(i) = v
            .rows
            .iter()
            .position(|r| match (&s.item, &r.item, &s.track, &r.track) {
                (Some(a), Some(b), _, _) => a.key() == b.key(),
                (_, _, Some(a), Some(b)) => a.id == b.id,
                _ => false,
            })
    {
        v.sel = i;
    }
    v.off = off;
}
