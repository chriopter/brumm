//! Covers on the screen: drawn as pixel art or smooth half blocks, or
//! shown through kitty graphics as the real image.

use std::sync::Arc;

use image::RgbaImage;
use ratatui::style::{Color, Style};

use super::model::{Cmd, KittyReq, Model, Msg};
use super::options::COVER_PIXEL;
use super::text::*;
use crate::art::{self, Picture, Size};

/// Turns drawn cells into lines, one span per run of equal colors.
pub fn picture_lines(p: &Picture) -> Vec<L> {
    p.iter()
        .map(|row| {
            let mut spans = Vec::new();
            let mut text = String::new();
            let mut cur: Option<([u8; 3], [u8; 3])> = None;
            for c in row {
                let pair = (c.fg, c.bg);
                if cur != Some(pair) {
                    if let Some((f, b)) = cur {
                        spans.push(sp(
                            std::mem::take(&mut text),
                            Style::new()
                                .fg(Color::Rgb(f[0], f[1], f[2]))
                                .bg(Color::Rgb(b[0], b[1], b[2])),
                        ));
                    }
                    cur = Some(pair);
                }
                text.push(c.ch);
            }
            if let Some((f, b)) = cur {
                spans.push(sp(
                    text,
                    Style::new()
                        .fg(Color::Rgb(f[0], f[1], f[2]))
                        .bg(Color::Rgb(b[0], b[1], b[2])),
                ));
            }
            line(spans)
        })
        .collect()
}

/// img at size in a cover style; original is drawn by kitty_lines, and
/// smooth stands in for it until the image is there.
pub fn draw_cover(style: &str, img: &RgbaImage, size: Size) -> Vec<L> {
    let p = if style == COVER_PIXEL {
        art::render_dithered(img, size)
    } else {
        art::render_smooth(img, size)
    };
    picture_lines(&p)
}

/// Fills a cover's footprint while it loads.
pub fn placeholder(size: Size, icon: &str) -> Vec<L> {
    (0..size.height)
        .map(|i| {
            let l = if !icon.is_empty() && i == size.height / 2 && size.width > 4 {
                format!(
                    "{} {icon} {}",
                    "░".repeat(size.width / 2 - 1),
                    "░".repeat(size.width - size.width / 2 - 2)
                )
            } else {
                "░".repeat(size.width)
            };
            one(l, DIM)
        })
        .collect()
}

/// A cover shown through kitty graphics: sent to the terminal once per
/// address and size, then drawn as placeholder cells.
#[derive(Clone, Debug)]
pub struct KittyImage {
    pub id: u8,
    /// Sent; until then the cover draws smooth.
    pub ready: bool,
    /// For evicting the least recently drawn.
    pub used: u64,
}

/// Terminals keep images until told otherwise; brumm keeps at most this many.
const KITTY_KEEP: usize = 24;

pub fn kitty_key(url: &str, size: Size) -> String {
    format!("{url}@{}x{}", size.width, size.height)
}

impl Model {
    /// A cover through kitty graphics, or None while it is not in the
    /// terminal yet (it is then requested).
    pub fn kitty_lines(&mut self, url: &str, size: Size) -> Option<Vec<L>> {
        let key = kitty_key(url, size);
        if let Some(k) = self.kitty.get_mut(&key) {
            self.kitty_clock += 1;
            k.used = self.kitty_clock;
            if k.ready {
                let style = Style::new().fg(Color::Indexed(k.id));
                return Some(
                    art::kitty_lines(size)
                        .into_iter()
                        .map(|row| one(row.concat(), style))
                        .collect(),
                );
            }
            return None;
        }
        if !self.kitty_want.iter().any(|w| w.key == key) {
            self.kitty_want.push(KittyReq {
                key,
                url: url.to_string(),
                size,
            });
        }
        None
    }

    /// Uploads the first wanted cover off the event loop, one at a time,
    /// freeing the least recently drawn one when too many are kept.
    pub fn kitty_send(&mut self) {
        if self.kitty_busy || self.kitty_want.is_empty() {
            return;
        }
        let w = self.kitty_want.remove(0);
        if self.kitty.contains_key(&w.key) {
            return self.kitty_send();
        }
        while self.kitty.len() >= KITTY_KEEP {
            let oldest = self
                .kitty
                .iter()
                .min_by_key(|(_, v)| v.used)
                .map(|(k, _)| k.clone())
                .expect("not empty");
            let img = self.kitty.remove(&oldest).expect("there");
            self.raw(art::kitty_delete(img.id));
        }
        let used: std::collections::HashSet<u8> = self.kitty.values().map(|v| v.id).collect();
        let mut id = 0u8;
        for i in 0..240 {
            // ids 16–255, see art::kitty_send
            let c = 16 + ((self.kitty_next + i) % 240) as u8;
            if !used.contains(&c) {
                id = c;
                break;
            }
        }
        self.kitty_next = (id as usize).saturating_sub(16) + 1;
        self.kitty_clock += 1;
        self.kitty.insert(
            w.key.clone(),
            KittyImage {
                id,
                ready: false,
                used: self.kitty_clock,
            },
        );
        self.kitty_busy = true;
        let fallback: Option<Arc<RgbaImage>> = self.covers.get(&w.url).cloned();
        self.cmds.push(Cmd::Blocking(Box::new(move || {
            let dir = crate::config::cache_dir().join("covers");
            let img = art::original(&dir, &w.url).ok().map(Arc::new).or(fallback);
            let seq = img.and_then(|img| art::kitty_send(id, &img, w.size));
            Some(Msg::Kitty { key: w.key, seq })
        })));
    }

    /// Marks a cover as shown and passes its upload to the terminal.
    pub fn kitty_sent(&mut self, key: String, seq: Option<String>) {
        self.kitty_busy = false;
        let Some(k) = self.kitty.get_mut(&key) else {
            return self.kitty_send();
        };
        match seq {
            None => k.ready = false, // no image: stay smooth, and do not ask again
            Some(s) => {
                k.ready = true;
                self.raw(s);
            }
        }
        self.kitty_send();
    }

    /// Frees every image brumm sent, for when it quits.
    pub fn kitty_cleanup(&self) -> String {
        self.kitty
            .values()
            .map(|k| art::kitty_delete(k.id))
            .collect()
    }
}
