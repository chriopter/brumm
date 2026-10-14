//! The selected row shows as a card laid on the stage's cover: the playing
//! cover dims behind it, and the card sits in its lower right corner with
//! a margin of background around it and its name under it. Scrolling
//! through songs previews their covers the same way. Nothing playing, the
//! card is the stage. Once the list rests for CARD_FOR, the card goes and
//! the playing cover is back.

use std::time::Duration;

use super::model::{FRAME_EVERY, Model, ThumbReq};
use super::options::COVER_ORIGINAL;
use super::text::*;
use super::view::{IC_SONG, StageLine, kind_icon};
use crate::art::Size;
use crate::music::{KIND_ALBUM, KIND_ARTIST, KIND_PLAYLIST, KIND_STATION};

/// What a selected row shows on the stage.
#[derive(Clone, Debug)]
pub struct Card {
    pub art: String,
    pub kind: String,
    pub name: String,
    pub sub: String,
}

impl Card {
    /// The card's one line: its name, then artist or facts.
    fn caption(&self) -> L {
        let mut l = one(self.name.clone(), BOLD);
        if !self.sub.is_empty() {
            l.spans.push(sp(format!("  {}", self.sub), DIM));
        }
        l
    }
}

/// How long a card stays after the selection last moved.
const CARD_FOR: Duration = Duration::from_secs(4);

impl Model {
    /// How long the selected row's card has left on the stage.
    pub fn card_left(&self) -> Duration {
        let shown = FRAME_EVERY * (self.frame - self.cur().sel_at).max(0) as u32;
        CARD_FOR.saturating_sub(shown)
    }

    fn card_expired(&self) -> bool {
        (self.frame - self.cur().sel_at) as u128 * FRAME_EVERY.as_millis() >= CARD_FOR.as_millis()
    }

    /// The card of the selected row, if it has one worth showing: not the
    /// song playing, not a cover already on the stage, and only a while
    /// after the selection moved — or for good when nothing plays.
    fn selected_card(&self) -> Option<Card> {
        let v = self.cur();
        if self.opts.o.reduce_motion {
            return None;
        }
        let r = v.rows.get(v.sel)?;
        if !self.state.player.title.is_empty() && self.card_expired() {
            return None;
        }
        let c = if let Some(t) = &r.track {
            if t.id == self.state.player.id || t.artwork.is_empty() {
                return None;
            }
            Card {
                art: t.artwork.clone(),
                kind: "song".into(),
                name: t.title.clone(),
                sub: t.artist.clone(),
            }
        } else if let Some(it) = &r.item {
            if !matches!(
                it.kind.as_str(),
                KIND_ALBUM | KIND_PLAYLIST | KIND_ARTIST | KIND_STATION
            ) {
                return None;
            }
            let sub = if it.info.is_empty() {
                it.artist.clone()
            } else {
                it.info.clone()
            };
            Card {
                art: it.artwork.clone(),
                kind: it.kind.clone(),
                name: it.name.clone(),
                sub,
            }
        } else {
            return None;
        };
        if !c.art.is_empty() && c.art == self.cover_url && !self.state.player.title.is_empty() {
            return None; // the stage shows it already
        }
        Some(c)
    }

    /// A card's cover at size: through kitty graphics, from the drawn
    /// cache, or as a placeholder while it is being drawn.
    fn card_lines(&mut self, c: &Card, size: Size) -> Vec<L> {
        if self.opts.cover(self.has_kitty) == COVER_ORIGINAL
            && self.covers.contains_key(&c.art)
            && let Some(l) = self.kitty_lines(&c.art, size)
        {
            return l;
        }
        self.card_size = size;
        let key = self.thumb_key(&c.art, size);
        if let Some(l) = self.thumbs.get(&key) {
            return l.clone();
        }
        if !c.art.is_empty() {
            self.thumb_want = ThumbReq {
                key,
                url: c.art.clone(),
                size,
            };
        }
        let icon = if c.kind == "song" {
            IC_SONG
        } else {
            kind_icon(&c.kind)
        };
        super::cover::placeholder(size, icon)
    }

    pub fn thumb_key(&self, art: &str, size: Size) -> String {
        format!(
            "{}:{art}@{}x{}",
            self.opts.cover(self.has_kitty),
            size.width,
            size.height
        )
    }

    /// A card worth drawing ahead: the rows around the selection, nearest
    /// first, whose cover is loaded and card not drawn yet; else an
    /// up-next cover.
    pub fn next_thumb(&self) -> ThumbReq {
        let (v, size) = (self.cur(), self.card_size);
        if size.width == 0 || self.opts.o.reduce_motion {
            return self.next_thumb_want();
        }
        for d in [1i64, -1, 2, -2, 3, 4, 5, -3] {
            let i = v.sel as i64 + d;
            let Some(r) = (i >= 0).then(|| v.rows.get(i as usize)).flatten() else {
                continue;
            };
            let url = r
                .track
                .as_ref()
                .map(|t| t.artwork.clone())
                .or_else(|| r.item.as_ref().map(|i| i.artwork.clone()))
                .unwrap_or_default();
            if url.is_empty() || !self.covers.contains_key(&url) {
                continue;
            }
            let key = self.thumb_key(&url, size);
            if !self.thumbs.contains_key(&key) {
                return ThumbReq { key, url, size };
            }
        }
        self.next_thumb_want() // then the covers coming up
    }

    /// The stage's cover at size, with the selected card on it.
    pub fn stage_cover(&mut self, size: Size) -> Vec<L> {
        let c = self.selected_card();
        let w2 = (size.width as f64 * 0.62).round() as usize;
        let h2 = (size.height as f64 * 0.62).round() as usize;
        let Some(c) = c.filter(|_| h2 >= 4 && size.height - h2 >= 3) else {
            return self.cover_lines(size, false);
        };
        self.card_shown = true;
        let base = self.cover_lines(size, true);
        let over = self.card_lines(
            &c,
            Size {
                width: w2,
                height: h2,
            },
        );
        // The card; one cell of margin left, above and right.
        let (x0, y0) = (size.width - 1 - w2, size.height - 1 - h2);
        base.iter()
            .enumerate()
            .map(|(y, l)| {
                let mid = if y + 1 < y0 {
                    return l.clone();
                } else if y + 1 == y0 {
                    plain(" ".repeat(w2 + 2))
                } else if y < y0 + h2 {
                    cat(
                        cat(plain(" "), over.get(y - y0).cloned().unwrap_or_default()),
                        plain(" "),
                    )
                } else {
                    cat(cat(plain(" "), fit(&c.caption(), w2)), plain(" "))
                };
                cat(cut(l, 0, x0.saturating_sub(1)), mid)
            })
            .collect()
    }

    /// The stage when nothing plays: the selected card, full size.
    pub fn idle_card(&mut self, col_w: i64, cover_h: i64) -> Option<Vec<StageLine>> {
        let c = self.selected_card()?;
        if cover_h < 6 {
            return None;
        }
        let w0 = col_w.min((cover_h as f64 * self.cell_aspect).round() as i64) as usize;
        let mut out: Vec<StageLine> = self
            .card_lines(
                &c,
                Size {
                    width: w0,
                    height: cover_h as usize,
                },
            )
            .into_iter()
            .map(|l| StageLine {
                text: l,
                center: true,
                hit: None,
                width: w0,
            })
            .collect();
        out.push(StageLine::default());
        out.push(StageLine {
            text: one(c.name.clone(), BOLD),
            center: true,
            ..Default::default()
        });
        if !c.sub.is_empty() {
            out.push(StageLine {
                text: one(c.sub.clone(), DIM),
                center: true,
                ..Default::default()
            });
        }
        Some(out)
    }
}
