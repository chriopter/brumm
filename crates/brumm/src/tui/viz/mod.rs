//! The fullscreen visualizer: thirteen Winamp / demoscene styles drawn with
//! nothing but the terminal's 16 ANSI colors (foreground and background).
//! Everything renders into a reusable cell grid (glyph + fg + bg per cell)
//! that is copied into the screen's buffer, whose diff sends only what
//! changed. A shared listener smooths the spectrum, detects kicks and
//! breathes gently while paused, so every style reacts to the same beat.
//! Motion follows the clock: extra frames must not speed anything up, and
//! a slow frame must not slow the picture down.
//!
//! What a frame costs the terminal matters as much as drawing it: solid
//! areas are spaces on a background color, and shading changes the glyph
//! within one color pair rather than the color.

mod cathedral;
mod fire;
mod lava;
mod lines;
mod retro;
mod space;
mod warp;

use std::f64::consts::PI;
use std::sync::OnceLock;
use std::time::Instant;

use brumm_shared::listen::{Listener, Peaks};
use ratatui::buffer::Buffer;
use ratatui::style::Color;

/// The styles, best first: digits 1–9, 0 pick the first ten, the list (v)
/// shows all of them.
pub const NAMES: [&str; 13] = [
    "milkdrop",  // spectrum + waveform rings in zoom/twist feedback
    "cathedral", // stained-glass window, sun shafts and dust, slow pan
    "synth",     // synthwave: striped sun, spectrum mountains, racing grid
    "stars",     // braille warp starfield, kicks jump to hyperspace
    "wire",      // 3D wireframes (icosahedron, tesseract, torus) in braille
    "fireworks", // shells launched by kicks, bursting in color
    "lava",      // metaballs in half blocks, sized by the bands
    "scope",     // triggered braille oscilloscope with phosphor persistence
    "plasma",    // sine plasma blending theme colors with shade glyphs
    "fire",      // half-block demoscene fire, flames lick with the bass
    "matrix",    // digital rain shaped by the spectrum, driven by the highs
    "ridge",     // Unknown Pleasures: spectrum history as receding ridgelines
    "led",       // hi-fi graphic EQ with LED segments and peak hold
];

const VZ_MILK: usize = 0;
const VZ_CATHEDRAL: usize = 1;
const VZ_SYNTH: usize = 2;
const VZ_STARS: usize = 3;
const VZ_WIRE: usize = 4;
const VZ_FIREWORKS: usize = 5;
const VZ_LAVA: usize = 6;
const VZ_SCOPE: usize = 7;
const VZ_PLASMA: usize = 8;
const VZ_FIRE: usize = 9;
const VZ_MATRIX: usize = 10;
const VZ_RIDGE: usize = 11;
const VZ_LED: usize = 12;
const COUNT: usize = 13;

// ANSI SGR foreground codes (the theme's 16 colors). 0 means "no color";
// a background uses the same code.
pub(crate) const RED: u8 = 31;
pub(crate) const GREEN: u8 = 32;
pub(crate) const YELLOW: u8 = 33;
pub(crate) const BLUE: u8 = 34;
pub(crate) const MAGENTA: u8 = 35;
pub(crate) const CYAN: u8 = 36;
pub(crate) const WHITE: u8 = 37;
pub(crate) const GRAY: u8 = 90;
pub(crate) const B_RED: u8 = 91;
pub(crate) const B_GREEN: u8 = 92;
pub(crate) const B_YELLOW: u8 = 93;
pub(crate) const B_MAGENTA: u8 = 95;
pub(crate) const B_CYAN: u8 = 96;
pub(crate) const B_WHITE: u8 = 97;

/// The theme color of an SGR foreground code.
pub fn color(c: u8) -> Color {
    match c {
        30 => Color::Black,
        31 => Color::Red,
        32 => Color::Green,
        33 => Color::Yellow,
        34 => Color::Blue,
        35 => Color::Magenta,
        36 => Color::Cyan,
        37 => Color::Gray,
        90 => Color::DarkGray,
        91 => Color::LightRed,
        92 => Color::LightGreen,
        93 => Color::LightYellow,
        94 => Color::LightBlue,
        95 => Color::LightMagenta,
        96 => Color::LightCyan,
        97 => Color::White,
        _ => Color::Reset,
    }
}

/// Sine lookup table for the per-cell styles, interpolated so slow motion
/// advances evenly at any frame rate instead of in table steps.
const SIN_N: usize = 4096;

fn sin_table() -> &'static [f32; SIN_N + 1] {
    static T: OnceLock<[f32; SIN_N + 1]> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = [0f32; SIN_N + 1];
        for (i, v) in t.iter_mut().enumerate() {
            *v = (i as f64 * 2.0 * PI / SIN_N as f64).sin() as f32;
        }
        t
    })
}

pub(crate) fn vsin(x: f32) -> f32 {
    let t = sin_table();
    let f = x * (SIN_N as f64 / (2.0 * PI)) as f32;
    let mut fl = (f as i32) as f32;
    if fl > f {
        fl -= 1.0;
    }
    let i = ((fl as i32) & (SIN_N as i32 - 1)) as usize;
    t[i] + (t[i + 1] - t[i]) * (f - fl)
}

/// Braille bit for sub-pixel (sx, sy) in a cell.
pub(crate) const BR_BIT: [[u8; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];

/// 4×4 ordered-dither thresholds in (0, 1).
pub(crate) const BAYER: [[f32; 4]; 4] = [
    [0.5 / 16.0, 8.5 / 16.0, 2.5 / 16.0, 10.5 / 16.0],
    [12.5 / 16.0, 4.5 / 16.0, 14.5 / 16.0, 6.5 / 16.0],
    [3.5 / 16.0, 11.5 / 16.0, 1.5 / 16.0, 9.5 / 16.0],
    [15.5 / 16.0, 7.5 / 16.0, 13.5 / 16.0, 5.5 / 16.0],
];

/// Per-style animation state between frames (peaks, history, particles…).
#[derive(Default)]
pub struct Visualizer {
    pub(crate) w: usize,
    pub(crate) h: usize,
    pub(crate) glyph: Vec<char>,
    pub(crate) col: Vec<u8>,
    pub(crate) bg: Vec<u8>,

    // The outgoing style during a dissolve.
    mix_g: Vec<char>,
    mix_c: Vec<u8>,
    mix_b: Vec<u8>,

    /// Steps the clock in tests; the wall clock otherwise.
    pub(crate) clock: Option<Instant>,
    pub(crate) au: Listener,
    au_at: Option<Instant>,
    /// Last draw per style.
    at: [Option<Instant>; COUNT],
    /// Fixed-rate tick accumulators.
    pub(crate) acc: [f64; COUNT],
    rng: u64,
    pub(crate) bands: Vec<f64>,
    ints: Vec<i64>,
    /// Braille canvas: dot bits and a color rank per cell.
    dots: Vec<u8>,
    rank: Vec<u8>,
    /// Half-block canvas: w × 2h colors.
    pub(crate) pix: Vec<u8>,

    pub(crate) led_pk: Peaks,
    pub(crate) scope: lines::Scope,
    pub(crate) ridge: lines::Ridge,
    pub(crate) fire: fire::Fire,
    pub(crate) stars: space::Stars,
    pub(crate) plasma: warp::Plasma,
    pub(crate) milk: warp::Milk,
    pub(crate) synth: retro::Synth,
    pub(crate) wire: space::Wire,
    pub(crate) fw: fire::Fireworks,
    pub(crate) lava: lava::Lava,
    pub(crate) mx: retro::Matrix,
    pub(crate) cathedral: cathedral::Cathedral,
}

/// The seconds from last to t, moving last: 1/25 s the first time, at most
/// 0.1 s so a style coming back after a minute doesn't leap.
fn since(last: &mut Option<Instant>, t: Instant) -> f64 {
    let d = match last {
        Some(l) => t
            .saturating_duration_since(*l)
            .as_secs_f64()
            .clamp(0.0, 0.1),
        None => 0.04,
    };
    *last = Some(t);
    d
}

fn wrap_style(style: i64) -> usize {
    style.rem_euclid(COUNT as i64) as usize
}

impl Visualizer {
    pub(crate) fn rnd(&mut self) -> u64 {
        if self.rng == 0 {
            self.rng = 0x9E3779B97F4A7C15;
        }
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }

    pub(crate) fn rf(&mut self) -> f64 {
        (self.rnd() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Draws a dissolve from style from to style to, f (0..1) of the way
    /// through, in blocks of 6×2 cells that switch over in a fixed random
    /// order. It mixes the two grids, so a dissolve costs two draws. from
    /// None or f ≥ 1 draws to alone. The grid is then w × h cells.
    #[allow(clippy::too_many_arguments)] // the frame's inputs, each plain data
    pub fn render_mix(
        &mut self,
        from: Option<usize>,
        to: usize,
        f: f64,
        spec: &[f64],
        wave: &[f64],
        w: usize,
        h: usize,
        playing: bool,
    ) {
        if h == 0 || w == 0 {
            self.grid(w, h);
            return;
        }
        let t = self.clock.unwrap_or_else(Instant::now);
        let dt = since(&mut self.au_at, t);
        self.au.listen(spec, dt, playing);
        let to = wrap_style(to as i64);
        match from {
            Some(from) if f < 1.0 => {
                let from = wrap_style(from as i64);
                self.draw(from, wave, w, h, t);
                self.mix_g.clone_from(&self.glyph);
                self.mix_c.clone_from(&self.col);
                self.mix_b.clone_from(&self.bg);
                self.draw(to, wave, w, h, t);
                const BW: usize = 6; // block size in cells
                const BH: usize = 2;
                for y in 0..h {
                    let mut x0 = 0;
                    while x0 < w {
                        let mut hsh = ((x0 / BW) as u32).wrapping_mul(2654435761)
                            ^ ((y / BH) as u32).wrapping_mul(2246822519);
                        hsh ^= hsh >> 15;
                        if ((hsh % 1000) as f64 / 1000.0) >= f {
                            let (i, j) = (y * w + x0, y * w + (x0 + BW).min(w));
                            self.glyph[i..j].copy_from_slice(&self.mix_g[i..j]);
                            self.col[i..j].copy_from_slice(&self.mix_c[i..j]);
                            self.bg[i..j].copy_from_slice(&self.mix_b[i..j]);
                        }
                        x0 += BW;
                    }
                }
            }
            _ => self.draw(to, wave, w, h, t),
        }
    }

    fn draw(&mut self, style: usize, wave: &[f64], w: usize, h: usize, t: Instant) {
        let dt = since(&mut self.at[style], t);
        self.grid(w, h);
        match style {
            VZ_MILK => self.draw_milk(style, wave, dt),
            VZ_CATHEDRAL => self.draw_cathedral(dt),
            VZ_SYNTH => self.draw_synth(dt),
            VZ_STARS => self.draw_stars(dt),
            VZ_WIRE => self.draw_wire(dt),
            VZ_FIREWORKS => self.draw_fireworks(dt),
            VZ_LAVA => self.draw_lava(dt),
            VZ_SCOPE => self.draw_scope(wave, dt),
            VZ_PLASMA => self.draw_plasma(dt),
            VZ_FIRE => self.draw_fire(dt),
            VZ_MATRIX => self.draw_matrix(dt),
            VZ_RIDGE => self.draw_ridge(style, dt),
            VZ_LED => self.draw_led(dt),
            _ => {}
        }
    }

    /// Turns elapsed time into whole simulation steps at hz for the styles
    /// that evolve in discrete steps, at most most.
    pub(crate) fn ticks(&mut self, style: usize, dt: f64, hz: f64, most: usize) -> usize {
        self.acc[style] += dt * hz;
        let n = self.acc[style] as usize;
        self.acc[style] -= n as f64;
        n.min(most)
    }

    fn grid(&mut self, w: usize, h: usize) {
        let n = w * h;
        self.w = w;
        self.h = h;
        self.glyph.clear();
        self.glyph.resize(n, ' ');
        self.col.clear();
        self.col.resize(n, 0);
        self.bg.clear();
        self.bg.resize(n, 0);
    }

    /// Copies the grid into buf at (x, y).
    pub fn blit(&self, buf: &mut Buffer, x: u16, y: u16) {
        let area = buf.area;
        for row in 0..self.h {
            let yy = y as usize + row;
            if yy >= (area.y + area.height) as usize {
                break;
            }
            for c in 0..self.w {
                let xx = x as usize + c;
                if xx >= (area.x + area.width) as usize {
                    break;
                }
                let i = row * self.w + c;
                let cell = &mut buf[(xx as u16, yy as u16)];
                cell.set_char(self.glyph[i]);
                cell.set_fg(if self.glyph[i] == ' ' {
                    Color::Reset
                } else {
                    color(self.col[i])
                });
                cell.set_bg(color(self.bg[i]));
            }
        }
    }

    /// Styled rows for the visualizer embedded in the normal player.
    pub fn lines(&self) -> Vec<super::text::L> {
        use ratatui::{
            style::Style,
            text::{Line, Span},
        };
        (0..self.h)
            .map(|y| {
                Line::from(
                    (0..self.w)
                        .map(|x| {
                            let i = y * self.w + x;
                            Span::styled(
                                self.glyph[i].to_string(),
                                Style::default()
                                    .fg(color(self.col[i]))
                                    .bg(color(self.bg[i])),
                            )
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect()
    }

    /// The grid as text rows, each cell's glyph and colors, for tests.
    #[cfg(test)]
    pub fn rows(&self) -> Vec<String> {
        (0..self.h)
            .map(|y| {
                (y * self.w..(y + 1) * self.w)
                    .map(|i| format!("{}{}/{}", self.glyph[i], self.col[i], self.bg[i]))
                    .collect()
            })
            .collect()
    }

    // ─── braille canvases ───────────────────────────────────────────────
    // Line styles draw into 2×4 dots per cell. The ranked canvas colors a
    // cell by its most prominent dot (lowest rank); the float canvas by its
    // brightest.

    pub(crate) fn dot_reset(&mut self) {
        let n = self.w * self.h;
        self.dots.clear();
        self.dots.resize(n, 0);
        self.rank.clear();
        self.rank.resize(n, 255);
    }

    pub(crate) fn dot(&mut self, x: i64, y: i64, r: u8) {
        if x < 0 || y < 0 || x >= self.w as i64 * 2 || y >= self.h as i64 * 4 {
            return;
        }
        let i = (y >> 2) as usize * self.w + (x >> 1) as usize;
        self.dots[i] |= BR_BIT[(x & 1) as usize][(y & 3) as usize];
        if r < self.rank[i] {
            self.rank[i] = r;
        }
    }

    /// Plots a straight run of dots, at most 64 long.
    pub(crate) fn dot_line(&mut self, mut x0: f64, mut y0: f64, x1: f64, y1: f64, r: u8) {
        let (dx, dy) = (x1 - x0, y1 - y0);
        let n = (dx.abs().max(dy.abs()) as i64).min(64);
        if n <= 0 {
            self.dot(x1 as i64, y1 as i64, r);
            return;
        }
        let (sx, sy) = (dx / n as f64, dy / n as f64);
        for _ in 0..=n {
            self.dot(x0 as i64, y0 as i64, r);
            x0 += sx;
            y0 += sy;
        }
    }

    /// Writes the ranked canvas into the grid, rank r in pal[r].
    pub(crate) fn dot_flush(&mut self, pal: &[u8]) {
        for i in 0..self.dots.len() {
            let b = self.dots[i];
            if b == 0 {
                continue;
            }
            let r = (self.rank[i] as usize).min(pal.len() - 1);
            self.glyph[i] = char::from_u32(0x2800 + b as u32).unwrap_or(' ');
            self.col[i] = pal[r];
        }
    }

    /// Writes a float canvas (2w×4h dots, 0..1) into the grid: dots above
    /// lit are set and the cell takes ramp[brightest dot × 8]. rows, if
    /// given, says which cell rows hold anything at all.
    pub(crate) fn glow(&mut self, c: &[f32], rows: Option<&[bool]>, lit: f32, ramp: &[u8; 8]) {
        let (w, big_w) = (self.w, self.w * 2);
        for cy in 0..self.h {
            if rows.is_some_and(|r| !r[cy]) {
                continue;
            }
            for cx in 0..w {
                let mut bits = 0u8;
                let mut m = 0f32;
                let mut base = cy * 4 * big_w + cx * 2;
                for (&left, &right) in BR_BIT[0].iter().zip(&BR_BIT[1]) {
                    let (a, b) = (c[base], c[base + 1]);
                    base += big_w;
                    if a > lit {
                        bits |= left;
                        m = m.max(a);
                    }
                    if b > lit {
                        bits |= right;
                        m = m.max(b);
                    }
                }
                if bits == 0 {
                    continue;
                }
                let i = cy * w + cx;
                self.glyph[i] = char::from_u32(0x2800 + bits as u32).unwrap_or(' ');
                self.col[i] = ramp[((m * 8.0) as usize).min(7)];
            }
        }
    }

    // ─── half-block canvas ──────────────────────────────────────────────
    // Solid styles draw pixels of w × 2h (square, two per cell) in theme
    // colors. A cell becomes ▀ with the top pixel as foreground and the
    // bottom one as background, ▄ or ▀ alone over the terminal's
    // background, or a plain space on a background color when both halves
    // agree, which is the cheapest cell a terminal can be sent.

    pub(crate) fn pix_reset(&mut self) {
        let n = self.w * self.h * 2;
        self.pix.clear();
        self.pix.resize(n, 0);
    }

    /// Writes cell rows y0..y1-1 of the half-block canvas into the grid.
    pub(crate) fn pix_flush(&mut self, y0: usize, y1: usize) {
        let w = self.w;
        for y in y0..y1.min(self.h) {
            for x in 0..w {
                let (a, b) = (self.pix[2 * y * w + x], self.pix[2 * y * w + w + x]);
                let i = y * w + x;
                if a == b {
                    if a != 0 {
                        self.glyph[i] = ' ';
                        self.col[i] = 0;
                        self.bg[i] = a;
                    }
                } else if a == 0 {
                    (self.glyph[i], self.col[i], self.bg[i]) = ('▄', b, 0);
                } else if b == 0 {
                    (self.glyph[i], self.col[i], self.bg[i]) = ('▀', a, 0);
                } else {
                    (self.glyph[i], self.col[i], self.bg[i]) = ('▀', a, b);
                }
            }
        }
    }

    /// A scratch Vec<i64> of n.
    pub(crate) fn take_ints(&mut self, n: usize) -> Vec<i64> {
        let mut v = std::mem::take(&mut self.ints);
        v.clear();
        v.resize(n, 0);
        v
    }

    pub(crate) fn give_ints(&mut self, v: Vec<i64>) {
        self.ints = v;
    }

    // ─── led ────────────────────────────────────────────────────────────
    // Hi-fi graphic equalizer: wide columns of ▆ segments (gaps both ways),
    // green/yellow/red zones with the leading segment bright, unlit
    // segments glowing faintly, a white peak-hold LED. A kick lights whole
    // columns.
    fn draw_led(&mut self, dt: f64) {
        let (w, h) = (self.w as i64, self.h as i64);
        let mut nb = ((w + 1) / 6).clamp(3, 32);
        let mut bw = (w + 1) / nb - 1;
        if bw < 1 {
            bw = 1;
            nb = ((w + 1) / 2).max(1);
        }
        let off = (w - (nb * (bw + 1) - 1)) / 2;
        let mut bands = std::mem::take(&mut self.bands);
        resample(&mut bands, &self.au.sm, nb as usize);
        let pk = self.led_pk.update(&bands, dt, 0.6).to_vec();
        let all = self.au.beat > 0.45;
        for (i, &b) in bands.iter().enumerate() {
            let x0 = off + i as i64 * (bw + 1);
            let lit = (b * h as f64 + 0.5) as i64;
            let pr = if pk[i] > 0.02 {
                ((pk[i] * h as f64 + 0.5) as i64).min(h) - 1
            } else {
                -1
            };
            for r in 0..h {
                let f = (r as f64 + 0.5) / h as f64;
                let c = if r == pr && r >= lit - 1 {
                    B_WHITE
                } else if r < lit {
                    let c = if f < 0.6 {
                        GREEN
                    } else if f < 0.85 {
                        YELLOW
                    } else {
                        RED
                    };
                    if all || r == lit - 1 { bright(c) } else { c }
                } else {
                    GRAY
                };
                let ro = (h - 1 - r) * w + x0;
                for dx in 0..bw {
                    let i = (ro + dx) as usize;
                    if i < self.glyph.len() {
                        self.glyph[i] = '▆';
                        self.col[i] = c;
                    }
                }
            }
        }
        self.bands = bands;
    }
}

// ─── spectrum helpers ───────────────────────────────────────────────────

/// Samples src (0..1 values) at fraction f of its range, interpolated.
pub(crate) fn at(src: &[f64], f: f64) -> f64 {
    let m = src.len();
    if m == 0 {
        return 0.0;
    }
    let p = f.clamp(0.0, 1.0) * (m - 1) as f64;
    let j = p as usize;
    let mut x = src[j];
    if j + 1 < m {
        x += (src[j + 1] - x) * (p - j as f64);
    }
    x
}

/// Maps spec onto n values (linear interpolation, clamped 0..1).
pub(crate) fn resample(dst: &mut Vec<f64>, spec: &[f64], n: usize) {
    dst.clear();
    dst.resize(n, 0.0);
    let m = spec.len();
    if m == 0 {
        return;
    }
    for (i, d) in dst.iter_mut().enumerate() {
        let p = ((i as f64 + 0.5) * m as f64 / n as f64 - 0.5).max(0.0);
        let j = p as usize;
        let f = p - j as f64;
        let a = spec[j.min(m - 1)];
        let b = if j + 1 < m { spec[j + 1] } else { a };
        let x = a + (b - a) * f;
        *d = if x > 0.0 { x.min(1.0) } else { 0.0 }; // also catches NaN
    }
}

/// Maps spec onto n values with the bass in the middle and the highs
/// fanning out to both edges.
pub(crate) fn mirror(dst: &mut Vec<f64>, spec: &[f64], n: usize) {
    dst.clear();
    dst.resize(n, 0.0);
    let c = (n as f64 - 1.0) / 2.0;
    for (x, d) in dst.iter_mut().enumerate() {
        *d = at(spec, (x as f64 - c).abs() / (c + 0.5));
    }
}

pub(crate) fn bright(c: u8) -> u8 {
    if (30..=37).contains(&c) { c + 60 } else { c }
}

pub(crate) fn clamp(x: f64, lo: f64, hi: f64) -> f64 {
    x.max(lo).min(hi)
}

/// Multiplies a float canvas by k, skipping the work when it's dark.
pub(crate) fn decay(c: &mut [f32], k: f32) {
    for x in c.iter_mut() {
        if *x != 0.0 {
            *x *= k;
            if *x < 0.02 {
                *x = 0.0;
            }
        }
    }
}

/// decay for a sparse braille canvas big_w dots wide: it only visits the
/// cell rows marked in rows, and unmarks those gone dark.
pub(crate) fn decay_rows(c: &mut [f32], big_w: usize, rows: &mut [bool], k: f32) {
    for (r, on) in rows.iter_mut().enumerate() {
        if !*on {
            continue;
        }
        let mut any = false;
        for x in &mut c[r * 4 * big_w..(r + 1) * 4 * big_w] {
            if *x != 0.0 {
                *x *= k;
                if *x < 0.02 {
                    *x = 0.0;
                } else {
                    any = true;
                }
            }
        }
        *on = any;
    }
}

/// Draws a line into a float canvas of width big_w, brightness i0 → i1,
/// keeping the brighter value per dot and marking the cell rows it touches.
#[allow(clippy::too_many_arguments)]
pub(crate) fn plot(
    c: &mut [f32],
    rows: &mut [bool],
    big_w: usize,
    big_h: usize,
    mut x0: f64,
    mut y0: f64,
    x1: f64,
    y1: f64,
    mut i0: f32,
    i1: f32,
) {
    let (dx, dy) = (x1 - x0, y1 - y0);
    let n = dx.abs().max(dy.abs()) as i64;
    if n > 4 * (big_w + big_h) as i64 {
        return; // off in the weeds: a degenerate projection
    }
    let n = n.max(1);
    let (sx, sy) = (dx / n as f64, dy / n as f64);
    let di = (i1 - i0) / n as f32;
    for _ in 0..=n {
        if x0 >= 0.0 && y0 >= 0.0 {
            let (xi, yi) = (x0 as usize, y0 as usize);
            if xi < big_w && yi < big_h {
                let j = yi * big_w + xi;
                c[j] = c[j].max(i0);
                rows[yi >> 2] = true;
            }
        }
        x0 += sx;
        y0 += sy;
        i0 += di;
    }
}

#[cfg(test)]
mod tests;
