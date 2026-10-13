//! Up next. Room to spare under the player — a cover held back by the
//! width, a tall window — shows what comes next: a row of the next covers
//! in the queue, one per album, so while an album plays through it shows
//! the ones after it. A click plays from there. The queue is asked for
//! only when the song or the queue changes; the row is drawn once and then
//! reused. When only the rest of the playing album comes, its songs show
//! instead, one line each.

use std::collections::HashSet;

use super::model::{Model, Msg};
use super::text::*;
use super::view::{Hit, IC_ALBUM, StageLine, clock};
use crate::apple::Track;
use crate::art::Size;
use crate::ipc::{self, Request};

/// Covers in the row at most.
pub const NEXT_MAX: usize = 5;
/// A cover is at most a quarter of the row, however few come.
const NEXT_FOUR: i64 = 4;
/// Queue songs looked through for them.
const NEXT_LOOK: f64 = 150.0;
/// Rows a cover needs to be worth showing.
const NEXT_MIN_H: i64 = 4;
/// Songs listed at most, when no other cover comes.
const SONG_MAX: i64 = 8;

/// A song coming up, listed when no other cover does.
#[derive(Clone, Debug, PartialEq)]
pub struct NextSong {
    pub title: String,
    /// Only when not the playing one's.
    pub artist: String,
    pub num: i64,
    pub dur: f64,
    pub pos: i64,
}

/// The list as last drawn, for drawing it again as it is.
#[derive(Clone, Debug, Default)]
pub struct NextList {
    generation: u64,
    w: i64,
    n: i64,
    rows: Option<Vec<L>>,
}

/// A cover coming up and the first queue position that shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct NextUp {
    pub url: String,
    pub name: String,
    pub artist: String,
    pub pos: i64,
}

/// What the queue ahead depends on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NextKey {
    id: String,
    index: i64,
    length: i64,
    shuffle: bool,
}

/// The row as last drawn, for drawing it again as it is.
#[derive(Clone, Debug, Default)]
pub struct NextStrip {
    generation: u64,
    w: i64,
    tw: i64,
    th: i64,
    n: usize,
    style: String,
    /// Every cover was there: nothing to look up again.
    done: bool,
    rows: Vec<L>,
    /// The line above them.
    label: L,
    label_w: usize,
    /// Its kitty images, kept from eviction while shown.
    kitty: Vec<String>,
}

/// The queue's covers after its first song (the playing one), each once:
/// songs of one album share one cover, and the one on the stage is not
/// shown again.
pub fn upcoming(tracks: &[Track], pos: i64, playing: &str) -> Vec<NextUp> {
    let album = |t: &Track| format!("album:{}\0{}", t.album, t.artist);
    let mut seen: HashSet<String> = HashSet::from([playing.to_string()]);
    let mut out = Vec::new();
    for (i, t) in tracks.iter().enumerate() {
        if i == 0 {
            seen.insert(t.artwork.clone());
            if !t.album.is_empty() {
                seen.insert(album(t));
            }
            continue;
        }
        if t.artwork.is_empty()
            || seen.contains(&t.artwork)
            || (!t.album.is_empty() && seen.contains(&album(t)))
        {
            continue;
        }
        seen.insert(t.artwork.clone());
        if !t.album.is_empty() {
            seen.insert(album(t));
        }
        let name = if t.album.is_empty() {
            t.title.clone()
        } else {
            t.album.clone()
        };
        out.push(NextUp {
            url: t.artwork.clone(),
            name,
            artist: t.artist.clone(),
            pos: pos + i as i64,
        });
        if out.len() == NEXT_MAX {
            break;
        }
    }
    out
}

/// The queue's first songs after the playing one.
pub fn songs_ahead(tracks: &[Track], pos: i64) -> Vec<NextSong> {
    if tracks.len() < 2 {
        return Vec::new();
    }
    tracks[1..tracks.len().min(SONG_MAX as usize + 1)]
        .iter()
        .enumerate()
        .map(|(i, t)| NextSong {
            title: t.title.clone(),
            artist: if t.artist == tracks[0].artist {
                String::new()
            } else {
                t.artist.clone()
            },
            // not on an album: its place in the queue
            num: if t.number == 0 {
                i as i64 + 1
            } else {
                t.number
            },
            dur: t.duration,
            pos: pos + i as i64 + 1,
        })
        .collect()
}

impl Model {
    /// Asks the daemon for the queue ahead when it may have changed since
    /// last asked, or at once with force.
    pub fn fetch_next(&mut self, force: bool) {
        let st = &self.state.player;
        if st.id.is_empty() || self.client.is_none() {
            self.set_next(Vec::new(), Vec::new(), 0);
            return;
        }
        let k = NextKey {
            id: st.id.clone(),
            index: st.index,
            length: st.length,
            shuffle: st.shuffle,
        };
        if k == self.next_asked && !force {
            return;
        }
        self.next_asked = k;
        self.next_seq += 1;
        let seq = self.next_seq;
        self.ask(
            Request {
                cmd: ipc::CMD_QUEUE.into(),
                value: NEXT_LOOK,
                ..Default::default()
            },
            move |r| {
                Some(match r {
                    Err(_) => Msg::Next {
                        seq,
                        tracks: Vec::new(),
                        pos: 0,
                        ok: false,
                    },
                    Ok(m) => Msg::Next {
                        seq,
                        tracks: m.tracks,
                        pos: m.pos,
                        ok: true,
                    },
                })
            },
        );
    }

    /// Takes the queue's answer: its distinct covers after the playing
    /// song's, whose images are then fetched; with none, its songs.
    pub fn got_next(&mut self, seq: u64, mut tracks: Vec<Track>, mut pos: i64, ok: bool) {
        if seq != self.next_seq {
            return;
        }
        if !ok || pos < 0 {
            // the player was not ready: ask again on its next change
            self.next_asked = NextKey::default();
            self.set_next(Vec::new(), Vec::new(), 0);
            self.up_tracks.clear();
            return;
        }
        // A song shown before the player moved on to it is further down the
        // queue it answered with: count from there.
        if let Some(i) = tracks.iter().position(|t| t.id == self.state.player.id) {
            tracks.drain(..i);
            pos += i as i64;
        }
        let next = upcoming(&tracks, pos, &self.cover_url);
        let songs = if next.is_empty() {
            songs_ahead(&tracks, pos)
        } else {
            Vec::new()
        };
        let ahead = (tracks.len() as i64 - 1).max(0);
        self.up_tracks = tracks;
        self.up_pos = pos;
        let urls: Vec<String> = next
            .iter()
            .filter(|n| !self.covers.contains_key(&n.url))
            .map(|n| n.url.clone())
            .collect();
        self.set_next(next, songs, ahead);
        for u in urls {
            self.fetch_cover(&u);
        }
    }

    /// Replaces the covers or songs coming up and the count of songs ahead,
    /// if they changed.
    fn set_next(&mut self, next: Vec<NextUp>, songs: Vec<NextSong>, ahead: i64) {
        if next == self.next && songs == self.songs && (ahead == self.ahead || songs.is_empty()) {
            return;
        }
        self.next = next;
        self.songs = songs;
        self.ahead = ahead;
        self.next_gen += 1;
    }

    /// Sizes the row for a column col_w wide with free rows under the
    /// player: how many covers and each one's size; n is 0 when none fits.
    pub fn next_fit(&self, col_w: i64, free: i64) -> (usize, i64, i64) {
        if self.next.is_empty() {
            return (0, 0, 0);
        }
        let mut tw = (col_w - 2 * (NEXT_FOUR - 1)) / NEXT_FOUR;
        let mut th = (tw as f64 / self.cell_aspect) as i64;
        if th < NEXT_MIN_H {
            th = NEXT_MIN_H;
            tw = (th as f64 * self.cell_aspect).ceil() as i64;
        }
        if th > free - 2 {
            th = free - 2;
            tw = (th as f64 * self.cell_aspect) as i64;
        }
        if th < NEXT_MIN_H || tw < 1 || tw > col_w {
            return (0, 0, 0);
        }
        (
            self.next
                .len()
                .min(NEXT_MAX)
                .min(((col_w + 2) / (tw + 2)) as usize),
            tw,
            th,
        )
    }

    /// The stage's lines for the row, none when it does not fit.
    pub fn up_next(&mut self, col_w: i64, free: i64) -> Vec<StageLine> {
        if self.next.is_empty() {
            return self.up_songs(col_w, free);
        }
        let (n, tw, th) = self.next_fit(col_w, free);
        if n == 0 {
            return Vec::new();
        }
        let rows = self.next_rows(col_w, n, tw, th);
        let mut out = vec![
            StageLine::default(),
            StageLine {
                text: self.next_draw.label.clone(),
                width: self.next_draw.label_w,
                ..Default::default()
            },
        ];
        for (i, r) in rows.into_iter().enumerate() {
            let hit = (i == 0).then_some(Hit::UpNext { n, tw, th });
            out.push(StageLine {
                text: r,
                width: col_w as usize,
                hit,
                ..Default::default()
            });
        }
        out
    }

    /// The row of covers, col_w wide: drawn again only when the covers, the
    /// size or the style change, or one was still missing.
    fn next_rows(&mut self, col_w: i64, n: usize, tw: i64, th: i64) -> Vec<L> {
        let style = self.opts.cover(self.has_kitty);
        let d = &self.next_draw;
        if d.done
            && d.generation == self.next_gen
            && d.w == col_w
            && d.tw == tw
            && d.th == th
            && d.n == n
            && d.style == style
        {
            let keys = d.kitty.clone();
            let mut evicted = false;
            for k in keys {
                match self.kitty.get_mut(&k) {
                    Some(img) => {
                        self.kitty_clock += 1;
                        img.used = self.kitty_clock;
                    }
                    None => evicted = true,
                }
            }
            if !evicted {
                return self.next_draw.rows.clone();
            }
            self.next_draw.done = false; // evicted after all: draw it again
        }
        let first = self.next[0].clone();
        let label = trunc(
            &line(vec![
                sp("up next  ", DIM),
                sp(first.name.clone(), PLAIN),
                sp(format!("  {}", first.artist), DIM),
            ]),
            col_w.max(0) as usize,
        );
        let size = Size {
            width: tw as usize,
            height: th as usize,
        };
        let mut kitty_keys = Vec::new();
        let mut done = true;
        let mut pics: Vec<Vec<L>> = Vec::new();
        for i in 0..n {
            let url = self.next[i].url.clone();
            let mut got = None;
            if style == super::options::COVER_ORIGINAL
                && self.covers.contains_key(&url)
                && let Some(l) = self.kitty_lines(&url, size)
            {
                kitty_keys.push(super::cover::kitty_key(&url, size));
                got = Some(l);
            }
            if got.is_none() {
                got = self.thumbs.get(&self.thumb_key(&url, size)).cloned();
            }
            let lines = got.unwrap_or_else(|| {
                done = false;
                super::cover::placeholder(size, IC_ALBUM)
            });
            pics.push(lines);
        }
        let tail = (col_w - n as i64 * tw - (n as i64 - 1) * 2).max(0) as usize;
        let mut rows = Vec::with_capacity(th as usize);
        for y in 0..th as usize {
            let mut l = L::default();
            for (i, p) in pics.iter().enumerate() {
                if i > 0 {
                    l.spans.push(spaces(2));
                }
                l = cat(l, p.get(y).cloned().unwrap_or_default());
            }
            l.spans.push(spaces(tail));
            rows.push(l);
        }
        let label_w = w(&label);
        self.next_draw = NextStrip {
            generation: self.next_gen,
            w: col_w,
            tw,
            th,
            n,
            style,
            done,
            rows: rows.clone(),
            label,
            label_w,
            kitty: kitty_keys,
        };
        rows
    }

    /// An up-next cover whose loaded image is not drawn at the row's size
    /// yet, for drawing in the background.
    pub fn next_thumb_want(&self) -> super::model::ThumbReq {
        let d = &self.next_draw;
        if d.done || d.th == 0 {
            return Default::default();
        }
        let size = Size {
            width: d.tw as usize,
            height: d.th as usize,
        };
        for n in self.next.iter().take(d.n) {
            let key = self.thumb_key(&n.url, size);
            if self.covers.contains_key(&n.url) && !self.thumbs.contains_key(&key) {
                return super::model::ThumbReq {
                    key,
                    url: n.url.clone(),
                    size,
                };
            }
        }
        Default::default()
    }

    /// The stage's lines for the songs coming up, as many as fit under a
    /// gap and the label, the last saying how many more follow; none when
    /// fewer than two fit.
    fn up_songs(&mut self, col_w: i64, free: i64) -> Vec<StageLine> {
        let k = (free - 2).min(SONG_MAX);
        if self.songs.is_empty() || k < 2 {
            return Vec::new();
        }
        let mut n = (self.songs.len() as i64).min(k);
        if self.ahead > n && n == k {
            n -= 1; // room for the line saying more follow
        }
        let rows = self.song_rows(col_w, n);
        let mut out = vec![
            StageLine::default(),
            StageLine {
                text: one("up next", DIM),
                width: 7,
                ..Default::default()
            },
        ];
        for (i, r) in rows.into_iter().enumerate() {
            let hit = (i == 0).then_some(Hit::UpSongs { w: col_w, n });
            out.push(StageLine {
                text: r,
                width: col_w as usize,
                hit,
                ..Default::default()
            });
        }
        out
    }

    /// The list, col_w wide: drawn again only when the songs, the width or
    /// how many fit change. Like the list's rows, calmer.
    fn song_rows(&mut self, col_w: i64, n: i64) -> Vec<L> {
        let d = &self.song_draw;
        if d.generation == self.next_gen
            && d.w == col_w
            && d.n == n
            && let Some(r) = &d.rows
        {
            return r.clone();
        }
        let cw = col_w.max(0) as usize;
        let mut rows = Vec::new();
        for s in self.songs.iter().take(n as usize) {
            let mut text = line(vec![
                sp(format!("{:2}  ", s.num), DIM),
                sp(s.title.clone(), PLAIN),
            ]);
            if !s.artist.is_empty() {
                text.spans.push(sp(format!("  {}", s.artist), DIM));
            }
            let dur = if s.dur > 0.0 {
                clock(s.dur)
            } else {
                String::new()
            };
            let avail = col_w - 2 - dur.len() as i64;
            if dur.is_empty() || avail < 1 {
                rows.push(fit(&text, cw)); // no time, or too narrow for it
                continue;
            }
            rows.push(cat(
                fit(&text, avail as usize),
                line(vec![sp("  ", PLAIN), sp(dur, DIM)]),
            ));
        }
        let more = self.ahead - n;
        if more > 0 {
            rows.push(fit(&one(format!("    +{more} more"), DIM), cw));
        }
        self.song_draw = NextList {
            generation: self.next_gen,
            w: col_w,
            n,
            rows: Some(rows.clone()),
        };
        rows
    }

    /// The queue position of the cover or song at x, y.
    fn next_at(&self, x: i64, y: i64) -> Option<i64> {
        for (i, r) in self.geo.upnext.iter().enumerate() {
            if r.has(x, y) && i < self.next.len() {
                return Some(self.next[i].pos);
            }
        }
        let r = self.geo.upsongs;
        if r.has(x, y) && ((y - r.y0) as usize) < self.songs.len() {
            return Some(self.songs[(y - r.y0) as usize].pos);
        }
        None
    }

    /// Plays from the cover at x, y, if there is one.
    pub fn next_click(&mut self, x: i64, y: i64) -> bool {
        match self.next_at(x, y) {
            Some(pos) => {
                self.jump(pos);
                true
            }
            None => false,
        }
    }

    /// The song at queue position pos, when the queue ahead as last asked
    /// holds it.
    pub fn up_track(&self, pos: i64) -> Option<Track> {
        let i = pos - self.up_pos;
        (i >= 0)
            .then(|| self.up_tracks.get(i as usize).cloned())
            .flatten()
    }

    /// The queue position after the song playing, when the queue ahead
    /// holds it.
    pub fn after_playing(&self) -> Option<i64> {
        self.up_tracks
            .iter()
            .position(|t| t.id == self.state.player.id)
            .map(|i| self.up_pos + i as i64 + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: &str, album: &str, art: &str) -> Track {
        Track {
            id: id.into(),
            title: id.into(),
            artist: "A".into(),
            album: album.into(),
            artwork: art.into(),
            ..Default::default()
        }
    }

    /// Songs of one album share one cover; the playing one is not shown
    /// again; at most five.
    #[test]
    fn upcoming_dedupes() {
        let tracks = vec![
            t("1", "X", "x"),
            t("2", "X", "x"),
            t("3", "Y", "y"),
            t("4", "Y", "y2"),
            t("5", "", "z"),
            t("6", "W", ""),
        ];
        let up = upcoming(&tracks, 10, "stage");
        let names: Vec<&str> = up.iter().map(|u| u.name.as_str()).collect();
        assert_eq!(names, vec!["Y", "5"]);
        assert_eq!(up[0].pos, 12);
        let many: Vec<Track> = (0..20)
            .map(|i| t(&i.to_string(), &format!("al{i}"), &format!("art{i}")))
            .collect();
        assert_eq!(upcoming(&many, 0, "").len(), NEXT_MAX);
        let songs = songs_ahead(&tracks, 3);
        assert_eq!(songs.len(), 5);
        assert_eq!(
            (songs[0].num, songs[0].pos, songs[0].artist.as_str()),
            (1, 4, "")
        );
    }
}
