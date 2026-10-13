//! synth and matrix.

use super::*;

// ─── synth ──────────────────────────────────────────────────────────────
// Synthwave: a striped sun sinks into the horizon, its bands sliding down,
// behind a range of mountains raised by the spectrum (bass at the flanks,
// the valley kept open for the sun); stars twinkle with the highs. Below,
// a braille grid races toward the viewer at the loudness's pace while the
// camera sways, lines glowing magenta near and blue far. The sun and
// mountains are half blocks; a kick swells the sun and flashes the grid.
#[derive(Default)]
pub(crate) struct Synth {
    speed: f64,
    run: f64,
    stripe: f64,
    /// The beat, eased in.
    flare: f64,
    /// Per column, the mountains' own jaggedness.
    noise: Vec<f64>,
}

const SUN_PAL: [u8; 6] = [B_YELLOW, B_YELLOW, YELLOW, B_RED, RED, MAGENTA];

impl Visualizer {
    pub(crate) fn draw_synth(&mut self, dt: f64) {
        let (w, h) = (self.w, self.h);
        if self.synth.noise.len() != w {
            let (p1, p2, p3) = (self.rf() * 6.0, self.rf() * 6.0, self.rf() * 6.0);
            self.synth.noise = (0..w)
                .map(|x| {
                    let f = x as f64;
                    0.5 + 0.22 * (f * 0.13 + p1).sin()
                        + 0.16 * (f * 0.37 + p2).sin()
                        + 0.1 * (f * 0.91 + p3).sin()
                })
                .collect();
        }
        let (beat, live, level, bass, high, t) = (
            self.au.beat,
            self.au.live,
            self.au.level,
            self.au.bass,
            self.au.high,
            self.au.t,
        );
        let s = &mut self.synth;
        s.flare += (beat - s.flare) * (dt * 12.0).min(1.0);
        let target = 0.3 + live * (0.4 + 2.6 * level) + 1.2 * s.flare;
        s.speed += (target - s.speed) * (dt * 3.0).min(1.0);
        s.run += dt * s.speed;
        s.stripe = (s.stripe + dt * (0.2 + 0.5 * level)) % 1.0;
        let (flare, stripe, run) = (s.flare, s.stripe, s.run);

        let hz = (h * 11 / 20).max(1); // sky rows 0..hz-1, the floor below
        let big_p = 2 * hz;
        self.pix_reset();
        let cx = w as f64 / 2.0;
        let fp = big_p as f64;

        // the sun
        let r = (fp * 0.62).min(w as f64 * 0.2) * (1.0 + 0.05 * bass + 0.07 * flare);
        let sy = fp - r * 0.35;
        let mut py = (sy - r).max(0.0) as usize;
        while py < big_p && py as f64 <= sy + r {
            let dy = py as f64 + 0.5 - sy;
            let hw = (r * r - dy * dy).max(0.0).sqrt();
            let u = ((dy + r) / (2.0 * r)).max(0.0); // 0 top … 1 bottom
            if u > 0.4 && ((u * 14.0 - stripe) % 1.0) < (u - 0.4) * 1.6 {
                py += 1;
                continue; // a gap in the stripes
            }
            let c = SUN_PAL[((u / 0.68 * 6.0) as usize).min(5)];
            let x0 = (cx - hw + 0.5).max(0.0) as usize;
            let x1 = ((cx + hw + 0.5) as usize).min(w);
            for x in x0..x1 {
                self.pix[py * w + x] = c;
            }
            py += 1;
        }
        // The mountains: two ranges, a far one in blue with the highs, then
        // the near one in cyan with the bass, hiding what's behind it.
        let mut tops = self.take_ints(w);
        for layer in 0..2 {
            for (x, top) in tops.iter_mut().enumerate().take(w) {
                let u = (x as f64 + 0.5 - cx).abs() / cx;
                let m = if layer == 0 {
                    let e = clamp((u - 0.2) / 0.6, 0.0, 1.0);
                    fp * 0.72
                        * e
                        * (0.3
                            + 0.4 * self.synth.noise[(x + w / 3) % w]
                            + 0.5 * at(&self.au.sm, 0.3 + 0.7 * u))
                } else {
                    let mut e = clamp((u - 0.12) / 0.75, 0.0, 1.0);
                    e = e * e * (3.0 - 2.0 * e);
                    fp * 0.55
                        * e
                        * (0.25
                            + 0.4 * self.synth.noise[x]
                            + 0.6 * at(&self.au.sm, (1.0 - u) * 0.9))
                };
                *top = big_p as i64 - 1 - m as i64;
            }
            let ridge = if layer == 1 { B_CYAN } else { BLUE };
            for x in 0..w {
                let top = tops[x];
                if top >= big_p as i64 - 1 {
                    continue;
                }
                let mut lo = top;
                if x > 0 {
                    lo = lo.max(tops[x - 1] - 1);
                }
                if x < w - 1 {
                    lo = lo.max(tops[x + 1] - 1);
                }
                for py in top.max(0)..big_p as i64 {
                    self.pix[py as usize * w + x] = if py <= lo { ridge } else { 0 };
                }
            }
        }
        self.give_ints(tops);
        self.pix_flush(0, hz);

        // stars on the empty sky
        for y in 0..hz.saturating_sub(1) {
            for x in 0..w {
                let i = y * w + x;
                if self.glyph[i] != ' ' || self.bg[i] != 0 {
                    continue;
                }
                let mut hsh = (x as u32).wrapping_mul(73856093) ^ (y as u32).wrapping_mul(19349663);
                hsh ^= hsh >> 13;
                hsh = hsh.wrapping_mul(0x5bd1e995);
                hsh ^= hsh >> 15;
                if !hsh.is_multiple_of(61) {
                    continue;
                }
                let tw = (t * (0.7 + (hsh >> 8 & 7) as f64 * 0.3) + (hsh >> 12 & 63) as f64).sin();
                (self.glyph[i], self.col[i]) = if tw > 0.75 - 0.8 * high {
                    ('+', B_WHITE)
                } else if tw > -0.3 {
                    ('·', WHITE)
                } else {
                    ('·', GRAY)
                };
            }
        }

        // the floor: braille grid in perspective
        self.dot_reset();
        let (big_w, big_h) = (2 * w as i64, 4 * h as i64);
        let hy = hz as i64 * 4;
        let d = (big_h - 1 - hy) as f64;
        if d < 2.0 {
            self.dot_flush(&[B_MAGENTA]);
            return;
        }
        let rank = |z: f64| -> u8 {
            if z < 1.8 {
                0
            } else if z < 4.5 {
                1
            } else {
                2
            }
        };
        for x in 0..big_w {
            self.dot(x, hy, 0);
        }
        const GAP: f64 = 0.6;
        let off = run % GAP;
        let mut last = big_h as f64;
        for n in 0..200 {
            let z = 1.0 + n as f64 * GAP - off;
            let y = hy as f64 + d / z;
            if y >= big_h as f64 {
                continue;
            }
            if last - y < 4.0 {
                break; // lines crowding into the horizon: stop
            }
            last = y;
            let r = rank(z);
            for x in 0..big_w {
                self.dot(x, y as i64, r);
            }
        }
        let cam = (t * 0.17).sin() * 0.7;
        let spread = big_w as f64 / 8.0;
        for j in -12..=12 {
            let big_x = (j as f64 - cam) * spread;
            // start where neighboring lines are apart, not in a knot
            let y0 = hy + 2.max((1.5 / spread * d) as i64);
            let (mut px, mut py) = (big_w as f64 / 2.0 + big_x * (y0 - hy) as f64 / d, y0 as f64);
            let mut y = y0 + 2;
            while y < big_h {
                let k = (y - hy) as f64 / d;
                let x = big_w as f64 / 2.0 + big_x * k;
                if (x < 0.0 && px < 0.0) || (x >= big_w as f64 && px >= big_w as f64) {
                    break;
                }
                self.dot_line(px, py, x, y as f64, rank(1.0 / k));
                px = x;
                py = y as f64;
                y += 2;
            }
        }
        let mut pal = [B_MAGENTA, MAGENTA, BLUE];
        if beat > 0.5 {
            pal[0] = B_WHITE;
        }
        self.dot_flush(&pal);
    }
}

// ─── matrix ─────────────────────────────────────────────────────────────
// Digital rain: glyphs (half-width katakana, digits and marks: the symbol
// set of the matrix effect in TerminalTextEffects by ChrisBuilds, MIT, as
// ported to Rust by omacom/ttfx, MIT) fall in columns, each head white,
// its trail fading bright green → green → gray as the rain moves on. The
// spectrum decides where it rains (bass in the middle), the highs how hard
// and how fast the glyphs mutate, the loudness how fast it falls; a kick
// sends a squall down from the top and flashes the trails.
#[derive(Clone, Copy)]
struct Drop {
    x: usize,
    y: f32,
    sp: f32,
}

#[derive(Default)]
pub(crate) struct Matrix {
    w: usize,
    h: usize,
    g: Vec<char>,
    b: Vec<f32>,
    drops: Vec<Drop>,
    cap: usize,
    col: Vec<f64>,
    flow: f64,
    flash: f64,
    k: i32,
}

const MATRIX_GLYPHS: &str = "ｦｱｳｴｵｶｷｹｺｻｼｽｾｿﾀﾂﾃﾅﾆﾇﾈﾊﾋﾎﾏﾐﾑﾒﾓﾔﾕﾗﾘﾜ2598Z*):.\"=+-¦|_";

fn matrix_glyphs() -> &'static [char] {
    static G: std::sync::OnceLock<Vec<char>> = std::sync::OnceLock::new();
    G.get_or_init(|| MATRIX_GLYPHS.chars().collect())
}

impl Visualizer {
    fn glyph_rand(&mut self) -> char {
        let g = matrix_glyphs();
        g[(self.rnd() % g.len() as u64) as usize]
    }

    fn add_drop(&mut self, x: usize, y: f32) {
        if self.mx.drops.len() < self.mx.cap {
            let sp = (0.6 + 0.8 * self.rf()) as f32;
            self.mx.drops.push(Drop { x, y, sp });
        }
    }

    pub(crate) fn draw_matrix(&mut self, dt: f64) {
        let (w, h) = (self.w, self.h);
        if self.mx.w != w || self.mx.h != h {
            self.mx.w = w;
            self.mx.h = h;
            self.mx.g = (0..w * h).map(|_| ' ').collect();
            for i in 0..w * h {
                self.mx.g[i] = self.glyph_rand();
            }
            self.mx.b = vec![0.0; w * h];
            self.mx.cap = 2 * w + 64;
            self.mx.drops = Vec::with_capacity(self.mx.cap);
        }
        let mut col = std::mem::take(&mut self.mx.col);
        mirror(&mut col, &self.au.sm, w);
        let (live, level, high, kicks) = (self.au.live, self.au.level, self.au.high, self.au.kicks);
        let target = h as f64 * (0.3 + live * (0.2 + 1.2 * level));
        self.mx.flow += (target - self.mx.flow) * (dt * 2.0).min(1.0);
        if kicks != self.mx.k {
            self.mx.k = kicks;
            self.mx.flash = 1.0;
            for _ in 0..2 + w / 24 {
                let x = (self.rnd() % w as u64) as usize;
                let y = -(self.rf() * 3.0) as f32;
                self.add_drop(x, y);
            }
        }
        self.mx.flash *= (-dt * 5.0).exp();
        for (x, &e) in col.iter().enumerate() {
            let rate = (0.03 + 0.4 * e * e + 0.2 * high) * dt; // drops a second in this column
            if self.rf() < rate {
                let y = -(self.rf() * 2.0) as f32;
                self.add_drop(x, y);
            }
        }
        self.mx.col = col;
        // trails a third of the screen (at least 8 rows) long at any speed
        let trail = h as f64 * 0.35 + 8.0;
        let k = (-dt * 2.8 * self.mx.flow / trail).exp() as f32;
        decay(&mut self.mx.b, k);
        // mutations
        let n = ((w * h) as f64 * dt * (0.02 + 0.3 * high)) as usize;
        for _ in 0..n {
            let i = (self.rnd() % (w * h) as u64) as usize;
            self.mx.g[i] = self.glyph_rand();
        }
        let fl = (self.mx.flow * dt) as f32;
        let mut drops = std::mem::take(&mut self.mx.drops);
        drops.retain_mut(|d| {
            let y0 = d.y;
            d.y += d.sp * fl;
            let mut r = y0.floor() as i64 + 1;
            while r <= d.y.floor() as i64 {
                if r >= 0 && (r as usize) < h {
                    let i = r as usize * w + d.x;
                    self.mx.b[i] = 1.0;
                    let g = self.glyph_rand();
                    self.mx.g[i] = g;
                }
                r += 1;
            }
            d.y < h as f32 + 1.0
        });
        self.mx.drops = drops;

        let boost = (0.35 * self.mx.flash) as f32;
        for i in 0..w * h {
            let b = self.mx.b[i];
            if b < 0.06 {
                continue;
            }
            let b = b + boost;
            let c = if b > 0.8 {
                B_GREEN
            } else if b > 0.3 {
                GREEN
            } else {
                GRAY
            };
            self.glyph[i] = self.mx.g[i];
            self.col[i] = c;
        }
        for d in &self.mx.drops {
            let r = d.y as i64;
            if r >= 0 && (r as usize) < h && d.y >= 0.0 {
                let i = r as usize * w + d.x;
                self.col[i] = B_WHITE;
                self.glyph[i] = self.mx.g[i];
            }
        }
    }
}
