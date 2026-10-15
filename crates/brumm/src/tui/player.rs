//! The now-playing block of the stage: the cover on top, then a block
//! anchored to the panel's bottom border:
//!
//! ```text
//! Title                                    3/10
//! Artist · Album
//!
//! ━━━━━━━━━━━━━━━━━━━━╸━━━━━━━━━━━━━━━━━   meter, half-cell precise
//! 2:44 / 5:08                         -2:24
//!
//! 󰒝  󰑖        󰒮   ▐ 󰏤 ▌   󰒭         󰕾 100
//! ```
//!
//! Modes on the left, transport in the middle, volume on the right: three
//! groups, the play button the one filled element.

use super::keys::Badge;
use super::model::Model;
use super::text::*;
use super::view::{
    Hit, IC_MUTED, IC_NEXT, IC_PAUSE, IC_PLAY, IC_PREV, IC_QUEUE, IC_REPEAT, IC_REPEAT_ONE,
    IC_SHUFFLE, IC_VOLUME, StageLine, clock,
};
use crate::art::Size;

/// Lines besides the cover: title, meta, gap above it; meter, times, gap,
/// transport below (the stage adds the gap between cover and meter).
pub const STAGE_BELOW: i64 = 2 + 1 + 2 + 1 + 1;

impl Model {
    /// The stage in two parts: the cover (top-anchored) and the block under
    /// it (bottom-anchored); bottom has STAGE_BELOW lines.
    pub fn now_playing(&mut self, col_w: i64, cover_h: i64) -> (Vec<StageLine>, Vec<StageLine>) {
        let mut top = Vec::new();
        let mut bottom = Vec::new();
        if cover_h >= 6 {
            let cover_w = col_w.min((cover_h as f64 * self.cell_aspect).round() as i64);
            for l in self.stage_cover(Size {
                width: cover_w as usize,
                height: cover_h as usize,
            }) {
                top.push(StageLine {
                    text: l,
                    center: true,
                    hit: None,
                    width: cover_w as usize,
                });
            }
        }
        let st = self.state.player.clone();
        let (mut title, mut artist, mut album) =
            (st.title.clone(), st.artist.clone(), st.album.clone());
        let mut tag = L::default();
        if let Some(p) = &st.preview {
            title.clone_from(&p.title);
            artist.clone_from(&p.artist);
            album.clear();
            tag = one(" PREVIEW ", PILL);
        } else if st.length > 1 && st.index >= 0 {
            tag = one(format!("{}/{}", st.index + 1, st.length), DIM);
        }
        let mut heading = one(title.clone(), BOLD);
        if st.preview.is_none() && self.rating.get(&st.id).copied().unwrap_or(0) != 0 {
            heading.spans.push(sp("  ", PLAIN));
            heading = cat(heading, self.rating_mark(&st.id));
        }
        bottom.push(StageLine::of(spread(heading, tag, col_w.max(0) as usize)));
        let has_album_text = !album.is_empty() && album != title;
        let mut meta = plain(artist.clone());
        if has_album_text {
            meta.spans.push(sp(format!("  ·  {album}"), DIM));
        }
        let aw = (sw(&artist) as i64).min(col_w);
        let has_album = has_album_text && aw + 5 < col_w;
        let hit = st.preview.is_none().then(|| Hit::Meta {
            aw,
            album: has_album.then(|| col_w.min(aw + 5 + sw(&album) as i64)),
        });
        bottom.push(StageLine {
            text: trunc(&meta, col_w.max(0) as usize),
            hit,
            ..Default::default()
        });
        // The gap under the names carries a and A while keytips show, when
        // they act on this song (the selected row is not another song).
        if self.show_tips() && st.preview.is_none() && self.tips_on_playing() {
            let mut bs = vec![Badge {
                col: 0,
                key: "A".into(),
            }];
            if has_album {
                bs.push(Badge {
                    col: aw + 5,
                    key: "a".into(),
                });
            }
            bottom.push(StageLine::of(self.badges(col_w, bs)));
        } else {
            bottom.push(StageLine::default());
        }
        if st.preview.is_some() {
            // The clip's own position is not reported: a sweeping meter says
            // "a clip is playing", the line under it what comes back after.
            bottom.push(StageLine::of(self.sweep(col_w)));
            let back = if st.title.is_empty() {
                L::default()
            } else {
                line(vec![sp("then ", DIM), sp(st.title.clone(), PLAIN)])
            };
            bottom.push(StageLine::of(spread(
                one("30 s clip", DIM),
                back,
                col_w.max(0) as usize,
            )));
        } else {
            bottom.push(StageLine {
                text: self.meter(col_w),
                hit: Some(Hit::Bar { w: col_w }),
                ..Default::default()
            });
            bottom.push(StageLine::of(self.times(col_w)));
        }
        if self.show_tips() {
            bottom.push(StageLine::of(self.control_tips(col_w)));
        } else {
            bottom.push(StageLine::default());
        }
        bottom.push(self.controls(col_w));
        (top, bottom)
    }

    /// The progress rule: played in the accent, the rest dim, with a
    /// half-cell step (╸ ╺) so it moves every second on long songs too.
    pub fn meter(&self, wd: i64) -> L {
        let wd = wd.max(0);
        let (pos, dur) = (self.position(), self.state.player.dur);
        let halves = if dur > 0.0 {
            ((2 * wd) as f64 * pos / dur) as i64
        } else {
            0
        }
        .min(2 * wd);
        let (full, half) = (halves / 2, halves % 2 == 1);
        let mut l = one("━".repeat(full as usize), self.acc.here);
        let mut rest = wd - full;
        if half {
            l.spans.push(sp("╸", self.acc.here));
            rest -= 1;
            l.spans.push(sp("━".repeat(rest.max(0) as usize), DIM));
        } else if rest > 0 {
            l.spans
                .push(sp(format!("╺{}", "━".repeat((rest - 1) as usize)), DIM));
        }
        l
    }

    /// The meter while a preview clip plays: a short accent segment
    /// travelling along a dim rule.
    fn sweep(&self, wd: i64) -> L {
        let wd = wd.max(1);
        let seg = (wd / 8).max(3);
        let p = self.frame.rem_euclid(wd + seg);
        let (lo, hi) = ((p - seg).max(0), p.min(wd));
        line(vec![
            sp("━".repeat(lo as usize), DIM),
            sp("━".repeat((hi - lo).max(0) as usize), self.acc.here),
            sp("━".repeat((wd - hi).max(0) as usize), DIM),
        ])
    }

    /// Under the meter: elapsed and length on the left, what is left on
    /// the right, and a quiet "paused" in the middle when paused.
    pub fn times(&self, wd: i64) -> L {
        let st = &self.state.player;
        let pos = self.position();
        let wdu = wd.max(0) as usize;
        let left = if st.playing {
            line(vec![
                sp(clock(pos), PLAIN),
                sp(format!(" / {}", clock(st.dur)), DIM),
            ])
        } else {
            one(format!("{} / {}", clock(pos), clock(st.dur)), DIM)
        };
        let right = one(format!("-{}", clock((st.dur - pos).max(0.0))), DIM);
        if st.playing {
            return spread(left, right, wdu);
        }
        let mid_w = sw(IC_PAUSE) + sw(" paused");
        let (lw, rw) = (w(&left), w(&right));
        let at = wdu.saturating_sub(mid_w) / 2;
        if at >= lw + 2 && at + mid_w + rw + 2 <= wdu {
            let mut l = cat(left, plain(" ".repeat(at - lw)));
            l.spans.push(sp(IC_PAUSE, HERE));
            l.spans.push(sp(" paused", DIM));
            l.spans.push(spaces(wdu - at - mid_w - rw));
            return cat(l, right);
        }
        spread(
            cat(line(vec![sp(IC_PAUSE, HERE), sp(" ", PLAIN)]), left),
            right,
            wdu,
        )
    }

    /// The one-line meter with times at its ends, for the mini player.
    pub fn progress(&self, wd: i64) -> L {
        let (left, right) = (
            format!("{}  ", clock(self.position())),
            format!("  {}", clock(self.state.player.dur)),
        );
        let bar = (wd - sw(&left) as i64 - sw(&right) as i64).max(0);
        cat(cat(one(left, DIM), self.meter(bar)), one(right, DIM))
    }

    /// Shuffle and repeat on the left edge, previous / play / next
    /// centered, volume on the right edge. Each target is padded a cell so
    /// it is easy to hit.
    fn controls(&self, wd: i64) -> StageLine {
        let st = &self.state.player;
        let lit = |on: bool, s: &str| sp(s.to_string(), if on { PLAYS } else { DIM });
        let repeat_icon = if st.repeat == 1 {
            IC_REPEAT_ONE
        } else {
            IC_REPEAT
        };
        let modes = line(vec![
            lit(st.shuffle, IC_SHUFFLE),
            sp("  ", PLAIN),
            lit(st.repeat != 0, repeat_icon),
        ]);
        let play_icon = if st.playing { IC_PAUSE } else { IC_PLAY };
        // A key cap: half blocks round the reverse cell off into a square
        // button two cells wider than the icon.
        let play = line(vec![
            sp("▐", self.acc.plays),
            sp(format!(" {play_icon} "), self.acc.pill),
            sp("▌", self.acc.plays),
        ]);
        let volume = cat(
            line(vec![
                lit(self.section == self.sec("queue"), IC_QUEUE),
                sp("  ", PLAIN),
            ]),
            self.volume_label(),
        );
        let (start, gap, tw, vw, all) = control_layout(wd, w(&volume) as i64);
        let g = " ".repeat(gap as usize);
        let transport = cat(
            cat(
                cat(cat(plain(IC_PREV.to_string()), plain(g.clone())), play),
                plain(g),
            ),
            plain(IC_NEXT.to_string()),
        );
        let text = if all {
            let m = w(&modes) as i64;
            let mut l = cat(modes, plain(" ".repeat((start - m).max(0) as usize)));
            l = cat(l, transport);
            l = cat(l, plain(" ".repeat((wd - start - tw - vw).max(0) as usize)));
            cat(l, volume)
        } else {
            // Too narrow for three groups: transport alone, centered.
            cat(plain(" ".repeat(start.max(0) as usize)), transport)
        };
        StageLine {
            text,
            hit: Some(Hit::Controls {
                w: wd,
                start,
                gap,
                tw,
                vw: all.then_some(vw),
            }),
            ..Default::default()
        }
    }

    pub fn volume_label(&self) -> L {
        let vol = (self.state.player.volume * 100.0).round() as i64;
        if vol > 0 {
            return line(vec![
                sp(IC_VOLUME, PLAIN),
                sp(" ", PLAIN),
                sp(format!("{vol:3}"), DIM),
            ]);
        }
        line(vec![sp(IC_MUTED, DIM), sp(" ", PLAIN), sp("mute", DIM)])
    }

    /// The line above the controls while keytips show: each key under what
    /// it presses.
    fn control_tips(&self, wd: i64) -> L {
        let (start, gap, tw, vw, all) = control_layout(wd, 3 + w(&self.volume_label()) as i64);
        let mut bs = vec![
            Badge {
                col: start,
                key: "p".into(),
            },
            Badge {
                col: start + 1 + gap,
                key: "space".into(),
            },
            Badge {
                col: start + tw - 1,
                key: "n".into(),
            },
        ];
        if all {
            bs.push(Badge {
                col: 0,
                key: "s".into(),
            });
            bs.push(Badge {
                col: 3,
                key: "r".into(),
            });
            bs.push(Badge {
                col: wd - vw,
                key: "8".into(),
            });
            bs.push(Badge {
                col: wd - vw + 3,
                key: "- + m".into(),
            });
        }
        self.badges(wd, bs)
    }

    /// Whether a and A would act on the playing song.
    pub fn tips_on_playing(&self) -> bool {
        let v = self.cur();
        v.rows
            .get(v.sel)
            .and_then(|r| r.track.as_ref())
            .is_none_or(|t| t.id == self.state.player.id)
    }
}

/// The controls in w cells: the transport's first column, the gap around
/// the play button, the transport's width, the volume's, and whether modes
/// and volume fit beside the transport.
pub fn control_layout(wd: i64, vw: i64) -> (i64, i64, i64, i64, bool) {
    let gap = (wd / 14).clamp(2, 5);
    let tw = 1 + gap + 5 + gap + 1;
    let start = ((wd - tw) / 2).max(0);
    (
        start,
        gap,
        tw,
        vw,
        start >= 4 + 2 && start + tw + 2 + vw <= wd,
    )
}
