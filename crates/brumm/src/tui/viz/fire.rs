//! fire and fireworks.

use std::sync::OnceLock;

use super::*;

// ─── fire ───────────────────────────────────────────────────────────────
// Flames in half blocks, two square pixels per cell. Rather than a
// cellular automaton (which at terminal resolution boils into speckle) the
// heat is a smooth field: each column burns as high as its part of the
// spectrum (bass in the middle), and two octaves of value noise scrolling
// upward at different speeds cut it into tongues that lick, split and
// break off, more turbulent toward the tips. Being a function of the
// clock, it moves exactly as smoothly at 120 fps as at 30. Loudness makes
// the flames reach higher and rise faster, a kick flares them and throws
// sparks; the heat maps through red, orange and yellow to a white-hot
// base, ordered-dithered only where one color meets the next.
#[derive(Clone, Copy)]
struct Spark {
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
    life: f64,
}

#[derive(Default)]
pub(crate) struct Fire {
    w: usize,
    h: usize,
    src: Vec<f64>,
    /// Flame height per column, in pixels.
    reach: Vec<f32>,
    /// Noise scroll, in texels.
    off1: f64,
    off2: f64,
    flare: f64,
    sparks: Vec<Spark>,
    k: i32,
}

const FIRE_PAL: [u8; 6] = [0, RED, B_RED, YELLOW, B_YELLOW, B_WHITE];

/// A 256×256 tile of smooth, periodic value noise (a 32×32 lattice,
/// smoothstep-interpolated), sampled bilinearly.
fn noise() -> &'static [f32] {
    static N: OnceLock<Vec<f32>> = OnceLock::new();
    N.get_or_init(|| {
        const L: usize = 32;
        const S: usize = 8; // lattice size, texels per lattice cell
        let mut lat = [0f32; L * L];
        let mut r: u64 = 0x2545F4914F6CDD1D;
        for v in lat.iter_mut() {
            r ^= r << 13;
            r ^= r >> 7;
            r ^= r << 17;
            *v = (r >> 40) as f32 / (1u64 << 24) as f32;
        }
        let mut out = vec![0f32; 256 * 256];
        for y in 0..256 {
            let (ly, mut fy) = (y / S, (y % S) as f32 / S as f32);
            fy = fy * fy * (3.0 - 2.0 * fy);
            for x in 0..256 {
                let (lx, mut fx) = (x / S, (x % S) as f32 / S as f32);
                fx = fx * fx * (3.0 - 2.0 * fx);
                let a = lat[ly * L + lx] + (lat[ly * L + (lx + 1) % L] - lat[ly * L + lx]) * fx;
                let b = lat[((ly + 1) % L) * L + lx]
                    + (lat[((ly + 1) % L) * L + (lx + 1) % L] - lat[((ly + 1) % L) * L + lx]) * fx;
                out[y * 256 + x] = a + (b - a) * fy;
            }
        }
        out
    })
}

/// The noise tile at texel (u, v), both ≥ 0, wrapping.
fn noise_at(n: &[f32], u: f32, v: f32) -> f32 {
    let (iu, iv) = (u as i32, v as i32);
    let (fu, fv) = (u - iu as f32, v - iv as f32);
    let (r0, r1) = (((iv & 255) << 8) as usize, (((iv + 1) & 255) << 8) as usize);
    let (x0, x1) = ((iu & 255) as usize, ((iu + 1) & 255) as usize);
    let a = n[r0 + x0] + (n[r0 + x1] - n[r0 + x0]) * fu;
    let b = n[r1 + x0] + (n[r1 + x1] - n[r1 + x0]) * fu;
    a + (b - a) * fv
}

impl Visualizer {
    pub(crate) fn draw_fire(&mut self, dt: f64) {
        let n = noise();
        let (w, h) = (self.w, self.h);
        let big_p = 2 * h;
        if self.fire.w != w || self.fire.h != h {
            self.fire.w = w;
            self.fire.h = h;
            self.fire.reach = vec![0.0; w];
            self.fire.sparks.clear();
        }
        let mut src = std::mem::take(&mut self.fire.src);
        mirror(&mut src, &self.au.sm, w);
        let (beat, level, live, kicks) = (self.au.beat, self.au.level, self.au.live, self.au.kicks);
        let f = &mut self.fire;
        f.flare += (beat - f.flare) * (dt * 14.0).min(1.0);
        let fp = big_p as f64;
        let reach = fp * (0.28 + 0.5 * (level * 1.8).min(1.0) + 0.3 * f.flare);
        let mut top = 0f32; // the highest reach: nothing burns above 1.5× it
        for (x, &e) in src.iter().enumerate() {
            f.reach[x] = (reach * (0.45 + 0.75 * e.sqrt())).max(2.0) as f32;
            top = top.max(f.reach[x]);
        }
        f.src = src;
        // texels per pixel of the coarse and fine octaves, stretched tall
        const F1X: f32 = 2.2;
        const F1Y: f64 = 0.8;
        const F2X: f32 = 4.6;
        const F2Y: f64 = 1.9;
        let rise = fp * (0.7 + 0.6 * level + 0.5 * f.flare); // pixels a second
        f.off1 = (f.off1 + dt * rise * F1Y) % 256.0;
        f.off2 = (f.off2 + dt * rise * F2Y * 1.3) % 256.0;
        let (o1, o2) = (f.off1 as f32, f.off2 as f32);

        self.pix_reset();
        for y in 0..big_p {
            let d = (big_p - 1 - y) as f32; // height above the bottom
            if d > top * 1.5 {
                continue;
            }
            let by = &BAYER[y & 3];
            let v1 = y as f32 * F1Y as f32 + o1;
            let v2 = y as f32 * F2Y as f32 + o2 + 97.0;
            for x in 0..w {
                let rel = d / self.fire.reach[x]; // 0 at the base, 1 at the flame's reach
                if rel > 1.5 {
                    continue;
                }
                let nz = 0.6 * noise_at(n, x as f32 * F1X, v1)
                    + 0.4 * noise_at(n, x as f32 * F2X + 61.0, v2);
                // calm at the base, torn into tongues toward the tips
                let mut heat = (1.0 - rel) * 1.05 + (nz - 0.5) * (0.6 + 2.8 * (rel * rel).min(1.0));
                heat = heat.min(1.25 - rel); // tips cool to red, however they tear
                let l = (heat * 5.2 + (by[x & 3] - 0.5) * 0.7) as i32;
                if l > 0 {
                    self.pix[y * w + x] = FIRE_PAL[(l as usize).min(5)];
                }
            }
        }

        // Sparks: a burst per kick, a trickle with loudness, a few hundred
        // at most.
        let (fw, fh) = (w as f64, big_p as f64);
        let mut emit = 0usize;
        if kicks != self.fire.k {
            self.fire.k = kicks;
            emit = 6 + w / 10;
        }
        let rate = level * live * fw * 0.5 * dt;
        if self.rf() < rate - rate.floor() {
            emit += rate as usize + 1;
        } else {
            emit += rate as usize;
        }
        while emit > 0 && self.fire.sparks.len() < 400 {
            let s = Spark {
                x: fw / 2.0 + (self.rf() - 0.5) * fw * 0.6,
                y: fh * (1.0 - 0.3 * self.rf()),
                vx: (self.rf() - 0.5) * fh * 0.3,
                vy: -(0.5 + 0.6 * self.rf()) * fh,
                life: 0.5 + 0.9 * self.rf(),
            };
            self.fire.sparks.push(s);
            emit -= 1;
        }
        let mut i = 0;
        while i < self.fire.sparks.len() {
            let jitter = (self.rf() - 0.5) * fh * 1.2 * dt;
            let p = &mut self.fire.sparks[i];
            p.life -= dt;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.vy += fh * 0.35 * dt; // air slows them
            p.vx += jitter;
            if p.life <= 0.0 || p.y < 0.0 || p.x < 0.0 || p.x >= fw || p.y >= fh {
                self.fire.sparks.swap_remove(i);
                continue;
            }
            let c = if p.life > 0.9 {
                B_WHITE
            } else if p.life > 0.5 {
                B_YELLOW
            } else if p.life > 0.25 {
                YELLOW
            } else {
                RED
            };
            let j = p.y as usize * w + p.x as usize;
            if self.pix[j] == 0 || self.pix[j] == RED {
                self.pix[j] = c;
            }
            i += 1;
        }
        self.pix_flush(0, h);
    }
}

// ─── fireworks ──────────────────────────────────────────────────────────
// Shells launched on kicks (and steadily with the mids) climb from the
// city on a spark trail and burst at their peak: peonies, rings, willows
// that droop in gold, two-color crossettes. The bass sets how big a burst
// is, the loudness how far it throws, the highs make dying stars glitter.
// Sparks fall under gravity and drag on a braille canvas that fades in a
// sixth of a second, so every star draws a short trail; a burst's core
// flashes white. Paused, a small shell goes up now and then.
#[derive(Clone, Copy, Default)]
struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,
    span: f32,
    hue: u8,
    kind: u8,
}

const FW_STAR: u8 = 0;
const FW_WILLOW: u8 = 1;
const FW_ROCKET: u8 = 2;
const FW_TRAIL: u8 = 3;

/// Particles at most; the vector never grows past it.
const FW_CAP: usize = 4000;

#[derive(Default)]
pub(crate) struct Fireworks {
    /// Brightness per dot.
    c: Vec<f32>,
    /// Color per dot.
    hue: Vec<u8>,
    /// Cell rows with anything lit.
    rows: Vec<bool>,
    p: Vec<Particle>,
    k: i32,
    acc: f64,
    /// Building height per column, in cells.
    city: Vec<i8>,
    w: usize,
}

impl Fireworks {
    fn add(&mut self, p: Particle) {
        if self.p.len() < FW_CAP {
            self.p.push(p);
        }
    }
}

const FW_HUES: [u8; 6] = [RED, GREEN, YELLOW, BLUE, MAGENTA, CYAN];

impl Visualizer {
    pub(crate) fn draw_fireworks(&mut self, dt: f64) {
        let (w, h) = (self.w, self.h);
        let (big_w, big_h) = (w * 2, h * 4);
        if self.fw.c.len() != big_w * big_h || self.fw.rows.len() != h {
            self.fw.c = vec![0.0; big_w * big_h];
            self.fw.hue = vec![0; big_w * big_h];
            self.fw.rows = vec![false; h];
            self.fw.p = Vec::with_capacity(FW_CAP);
        }
        if self.fw.w != w {
            self.fw.w = w;
            self.fw.city = vec![0; w];
            let mut x = 0;
            while x < w {
                let bw = 3 + (self.rnd() % 6) as usize;
                let bh = (1 + self.rnd() % (h / 7).max(1) as u64) as i8;
                for i in x..(x + bw).min(w) {
                    self.fw.city[i] = bh;
                }
                x += bw;
            }
        }
        let (kicks, bass, mid, live, high) = (
            self.au.kicks,
            self.au.bass,
            self.au.mid,
            self.au.live,
            self.au.high,
        );
        decay_rows(
            &mut self.fw.c,
            big_w,
            &mut self.fw.rows,
            (-dt * 7.0).exp() as f32,
        );

        let (fw, fh) = (big_w as f32, big_h as f32);
        let g = fh * 0.5;
        let scale = clamp(((big_w * big_h) as f64).sqrt() / 220.0, 0.2, 1.4) as f32;
        let launch = |v: &mut Visualizer, small: bool| {
            let mut apex = fh * (0.35 + 0.4 * v.rf()) as f32;
            if small {
                apex *= 0.7;
            }
            let x = fw * (0.12 + 0.76 * v.rf()) as f32;
            let vx = (fw / 2.0 - x) * (0.05 * v.rf()) as f32;
            let hue = FW_HUES[(v.rnd() % 6) as usize];
            v.fw.add(Particle {
                x,
                y: fh - 1.0,
                vx,
                vy: -(2.0 * g * apex).sqrt(),
                life: 9.0,
                span: 9.0,
                hue,
                kind: FW_ROCKET,
            });
        };
        if kicks != self.fw.k {
            self.fw.k = kicks;
            launch(self, false);
            if bass > 0.45 {
                launch(self, false);
            }
        }
        self.fw.acc += dt * (0.3 + 1.6 * mid * live + 0.3 * (1.0 - live));
        while self.fw.acc >= 1.0 {
            launch(self, live < 0.5);
            self.fw.acc -= 1.0;
        }

        let fdt = dt as f32;
        let drag = (-dt * 1.3).exp() as f32;
        let wdrag = (-dt * 3.0).exp() as f32;
        let glitter = high > 0.15;
        let n = self.fw.p.len();
        for i in 0..n {
            let mut p = self.fw.p[i];
            let (x0, y0) = (p.x, p.y);
            p.life -= fdt;
            match p.kind {
                FW_ROCKET => {
                    p.vy += g * fdt;
                    if self.rnd() & 1 == 0 {
                        let vx = (self.rf() as f32 - 0.5) * 6.0;
                        self.fw.add(Particle {
                            x: p.x,
                            y: p.y,
                            vx,
                            vy: 4.0,
                            life: 0.35,
                            span: 0.35,
                            hue: YELLOW,
                            kind: FW_TRAIL,
                        });
                    }
                    if p.vy > -fh * 0.04 {
                        // the peak: burst
                        p.life = 0.0;
                        self.fw_burst(p.x, p.y, p.hue, scale);
                    }
                }
                FW_WILLOW => {
                    p.vx *= wdrag;
                    p.vy = p.vy * wdrag + g * 0.35 * fdt;
                }
                _ => {
                    p.vx *= drag;
                    p.vy = p.vy * drag + g * 0.45 * fdt;
                }
            }
            p.x += p.vx * fdt;
            p.y += p.vy * fdt;
            self.fw.p[i] = p;
            if p.life <= 0.0 || p.y >= fh || p.x < -fw || p.x > 2.0 * fw {
                continue;
            }
            let mut b = 0.2 + 0.7 * p.life / p.span;
            if p.kind == FW_ROCKET {
                b = 0.7;
            } else if glitter && p.life < p.span * 0.5 && self.rnd().is_multiple_of(3) {
                b = 1.0;
            }
            plot_hue(
                &mut self.fw.c,
                &mut self.fw.hue,
                &mut self.fw.rows,
                big_w,
                big_h,
                x0,
                y0,
                p.x,
                p.y,
                b,
                p.hue,
            );
        }
        // drop the dead
        self.fw
            .p
            .retain(|p| p.life > 0.0 && p.y < fh && p.x >= -fw && p.x <= 2.0 * fw);

        // Cells: the brightest dot names the color; fresh bursts flash white.
        for cy in 0..h {
            if !self.fw.rows[cy] {
                continue;
            }
            for cx in 0..w {
                let (mut bits, mut m, mut hu) = (0u8, 0f32, 0u8);
                let mut base = cy * 4 * big_w + cx * 2;
                for sy in 0..4 {
                    for (sx, bit) in BR_BIT.iter().enumerate() {
                        let b = self.fw.c[base + sx];
                        if b > 0.1 {
                            bits |= bit[sy];
                            if b > m {
                                m = b;
                                hu = self.fw.hue[base + sx];
                            }
                        }
                    }
                    base += big_w;
                }
                if bits == 0 {
                    continue;
                }
                let c = if m > 0.95 {
                    B_WHITE
                } else if m > 0.45 {
                    bright(hu)
                } else {
                    hu
                };
                let i = cy * w + cx;
                self.glyph[i] = char::from_u32(0x2800 + bits as u32).unwrap_or(' ');
                self.col[i] = c;
            }
        }
        // the skyline in front, a few windows lit
        for x in 0..w {
            for k in 0..self.fw.city[x] as usize {
                if k >= h {
                    break;
                }
                let y = h - 1 - k;
                let i = y * w + x;
                (self.glyph[i], self.col[i], self.bg[i]) = (' ', 0, GRAY);
                let hsh = ((x * 7919 + y * 104729) as u32).wrapping_mul(2654435761);
                if hsh >> 27 == 0 {
                    (self.glyph[i], self.col[i]) = ('▪', YELLOW);
                }
            }
        }
    }

    /// Turns a shell at (x, y) into stars.
    fn fw_burst(&mut self, x: f32, y: f32, hue: u8, scale: f32) {
        let big_h = (self.h * 4) as f32;
        let kind = self.rnd() % 4;
        let (bass, beat, level) = (self.au.bass, self.au.beat, self.au.level);
        let n = ((40.0 + 110.0 * bass + 50.0 * beat) as f32 * scale) as usize;
        let speed = big_h * (0.3 + 0.25 * level) as f32;
        let hue2 = FW_HUES[(self.rnd() % 6) as usize];
        for i in 0..n {
            // a shell bursts as a sphere: seen from here, stars crowd its rim
            let ang = i as f64 / n as f64 * 2.0 * PI + self.rf() * 0.2;
            let u = self.rf() * 2.0 - 1.0;
            let mut sp = speed * ((1.0 - u * u).sqrt() * (0.9 + 0.1 * self.rf())) as f32;
            let mut life = (0.9 + 0.8 * self.rf()) as f32;
            let mut p = Particle {
                x,
                y,
                hue,
                kind: FW_STAR,
                ..Default::default()
            };
            match kind {
                1 => sp = speed * (0.92 + 0.08 * self.rf()) as f32, // ring
                2 => {
                    // willow: slow, long, gold
                    sp *= 0.8;
                    life *= 1.6;
                    p.kind = FW_WILLOW;
                    p.hue = YELLOW;
                }
                3 if i & 1 == 0 => p.hue = hue2, // crossette: two colors
                _ => {}
            }
            p.vx = ang.cos() as f32 * sp;
            p.vy = ang.sin() as f32 * sp;
            p.life = life;
            p.span = life;
            self.fw.add(p);
        }
        // the flash
        let (big_w, big_h) = (self.w * 2, self.h * 4);
        plot_hue(
            &mut self.fw.c,
            &mut self.fw.hue,
            &mut self.fw.rows,
            big_w,
            big_h,
            x - 1.0,
            y,
            x + 1.0,
            y,
            1.0,
            WHITE,
        );
    }
}

/// Draws a line into a brightness + color canvas.
#[allow(clippy::too_many_arguments)]
fn plot_hue(
    c: &mut [f32],
    hue: &mut [u8],
    rows: &mut [bool],
    big_w: usize,
    big_h: usize,
    mut x0: f32,
    mut y0: f32,
    x1: f32,
    y1: f32,
    b: f32,
    hu: u8,
) {
    let (dx, dy) = (x1 - x0, y1 - y0);
    let n = (dx.abs().max(dy.abs()) as i64).clamp(1, 32);
    let (sx, sy) = (dx / n as f32, dy / n as f32);
    for _ in 0..=n {
        if x0 >= 0.0 && y0 >= 0.0 {
            let (xi, yi) = (x0 as usize, y0 as usize);
            if xi < big_w && yi < big_h {
                let j = yi * big_w + xi;
                if b >= c[j] {
                    c[j] = b;
                    hue[j] = hu;
                    rows[yi >> 2] = true;
                }
            }
        }
        x0 += sx;
        y0 += sy;
    }
}
