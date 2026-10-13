//! plasma and milkdrop.

use super::*;

// ─── plasma ─────────────────────────────────────────────────────────────
// Demoscene plasma: four interfering sine fields (columns, rows, diagonal,
// radial) index a copper ring of theme colors — dark, blue, cyan, white,
// gold, red, magenta and back — and each cell blends the two colors it
// falls between: ░▒▓ in the one over the other as background, ordered-
// dithered, so ten colors give a smooth gradient; solid cells are plain
// spaces on a background. The color pair only changes at a band edge,
// which keeps the traffic to a few switches a line. Everything moves at
// eased, continuous speeds: loudness drives the flow, the bass deepens
// the radial rings, the mids turn the colors, and every kick sends a
// ripple out from the center instead of jolting the whole screen.
#[derive(Default)]
pub(crate) struct Plasma {
    t: f64,
    speed: f64,
    hue: f64,
    bass: f64,
    /// Start times of the kick ripples.
    rip: [f64; 4],
    k: i32,
    /// Distance from the center, in rows.
    dist: Vec<f32>,
    w: usize,
    h: usize,
    sx: Vec<f32>,
    sy: Vec<f32>,
}

const PLASMA_RING: [u8; 10] = [
    0, BLUE, CYAN, B_CYAN, B_WHITE, B_YELLOW, YELLOW, RED, MAGENTA, BLUE,
];

/// By quarters.
const SHADE_RAMP: [char; 5] = [' ', '░', '▒', '▓', '█'];

impl Visualizer {
    pub(crate) fn draw_plasma(&mut self, dt: f64) {
        let (w, h) = (self.w, self.h);
        let s = &mut self.plasma;
        if s.w != w || s.h != h {
            s.w = w;
            s.h = h;
            s.dist = vec![0.0; w * h];
            s.sx = vec![0.0; w];
            s.sy = vec![0.0; h];
            let (cx, cy) = ((w as f64 - 1.0) / 2.0, (h as f64 - 1.0) / 2.0);
            for y in 0..h {
                for x in 0..w {
                    let (dx, dy) = ((x as f64 - cx) * 0.5, y as f64 - cy);
                    s.dist[y * w + x] = (dx * dx + dy * dy).sqrt() as f32;
                }
            }
        }
        let a = &self.au;
        let target = 0.3 + a.live * 1.4 * a.level;
        s.speed += (target - s.speed) * (dt * 1.2).min(1.0);
        s.t = (s.t + dt * s.speed) % (2.0 * PI * 1000.0);
        s.hue = (s.hue + dt * (0.25 + 0.9 * a.mid)) % PLASMA_RING.len() as f64;
        s.bass += (a.bass - s.bass) * (dt * 5.0).min(1.0);
        if a.kicks != s.k {
            s.k = a.kicks;
            s.rip.copy_within(0..3, 1);
            s.rip[0] = a.t;
        }
        let t = s.t as f32;
        for x in 0..w {
            s.sx[x] = vsin(x as f32 * 0.08 + t);
        }
        for y in 0..h {
            s.sy[y] = vsin(y as f32 * 0.19 - t * 1.3);
        }
        // Ripples: a ring 8 rows wide running out at 30 rows a second, fading.
        let (mut rr, mut ra) = ([0f32; 4], [0f32; 4]);
        let mut nr = 0;
        for &t0 in &s.rip {
            let age = a.t - t0;
            if t0 > 0.0 && age < 1.6 {
                rr[nr] = (age * 30.0) as f32;
                ra[nr] = (0.9 * (1.0 - age / 1.6)) as f32;
                nr += 1;
            }
        }
        let amp = (0.7 + 0.9 * s.bass) as f32;
        let hue = s.hue as f32;
        // Palette steps per unit of the field: 13/8 in integer arithmetic,
        // as the plasma was tuned.
        const K: f32 = (13 / 8) as f32;
        let n = PLASMA_RING.len() as f32;
        for y in 0..h {
            let sy = s.sy[y];
            let ro = y * w;
            let fy = y as f32 * 0.11;
            let by = &BAYER[y & 3];
            for x in 0..w {
                let d = s.dist[ro + x];
                let mut p = s.sx[x]
                    + sy
                    + vsin(x as f32 * 0.045 + fy + t * 0.7)
                    + amp * vsin(d * 0.33 - t * 1.7);
                for k in 0..nr {
                    let mut e = d - rr[k];
                    if e > -4.0 && e < 4.0 {
                        e *= 0.25;
                        p += ra[k] * (1.0 - e * e) * (1.0 - e * e); // a smooth bump
                    }
                }
                let pos = (p + 5.0) * K + hue; // p ≥ -4.6: never negative
                let i = pos as usize;
                let q = ((pos - i as f32) * 4.0 + by[x & 3]) as usize;
                let i = i % PLASMA_RING.len();
                let j = if (i + 1) as f32 >= n { 0 } else { i + 1 };
                let (ca, cb) = (PLASMA_RING[i], PLASMA_RING[j]);
                let c = ro + x;
                if (ca == 0 && q == 0) || (cb == 0 && q == 4) {
                    // the terminal's own background
                } else if q == 0 {
                    // solid: a space on the color is the cheapest cell
                    (self.glyph[c], self.col[c], self.bg[c]) = (' ', cb, ca);
                } else if q == 4 {
                    (self.glyph[c], self.col[c], self.bg[c]) = (' ', cb, cb);
                } else if ca == 0 {
                    (self.glyph[c], self.col[c]) = (SHADE_RAMP[q], cb);
                } else if cb == 0 {
                    (self.glyph[c], self.col[c]) = (SHADE_RAMP[4 - q], ca);
                } else {
                    (self.glyph[c], self.col[c], self.bg[c]) = (SHADE_RAMP[q], cb, ca);
                }
            }
        }
    }
}

// ─── milkdrop ───────────────────────────────────────────────────────────
// MilkDrop-style feedback on a braille canvas: a rotating ring whose radius
// is the spectrum (mirrored so it's symmetric) and an inner ring bent by
// the waveform are drawn every frame, and 30 times a second the canvas is
// zoomed out, twisted and faded, so echoes spiral outward. A kick punches
// the zoom and fires a shockwave ring; every eighth kick reverses the
// twist and every fourth changes the colors.
#[derive(Default)]
pub(crate) struct Milk {
    mk: Vec<f32>,
    tmp: Vec<f32>,
    map: [Vec<i32>; 4],
    w: usize,
    h: usize,
    rot: f64,
    k: i32,
}

const MILK_PAL: [u8; 6] = [MAGENTA, CYAN, BLUE, GREEN, YELLOW, RED];

impl Visualizer {
    pub(crate) fn draw_milk(&mut self, style: usize, wave: &[f64], dt: f64) {
        let (big_w, big_h) = (self.w * 2, self.h * 4);
        let n_dots = big_w * big_h;
        let (cx, cy) = ((big_w as f64 - 1.0) / 2.0, (big_h as f64 - 1.0) / 2.0);
        if self.milk.w != big_w || self.milk.h != big_h {
            self.milk.w = big_w;
            self.milk.h = big_h;
            self.milk.mk = vec![0.0; n_dots];
            self.milk.tmp = vec![0.0; n_dots];
            // Where each dot pulls its value from: calm / punch zoom, each
            // twisting either way. Built once per size.
            for k in 0..4 {
                let zoom = if k & 1 != 0 { 0.87 } else { 0.95 };
                let rot = if k & 2 != 0 { -0.025 } else { 0.025 };
                let (cs, sn) = (f64::cos(rot) * zoom, f64::sin(rot) * zoom);
                let mut m = vec![-1i32; n_dots];
                for y in 0..big_h {
                    for x in 0..big_w {
                        let (dx, dy) = (x as f64 - cx, y as f64 - cy);
                        let sx = (cx + dx * cs - dy * sn).round() as i64;
                        let sy = (cy + dx * sn + dy * cs).round() as i64;
                        if sx >= 0 && sy >= 0 && sx < big_w as i64 && sy < big_h as i64 {
                            m[y * big_w + x] = (sy as usize * big_w + sx as usize) as i32;
                        }
                    }
                }
                self.milk.map[k] = m;
            }
        }
        let (kicks, beat, level, bass, live, t) = (
            self.au.kicks,
            self.au.beat,
            self.au.level,
            self.au.bass,
            self.au.live,
            self.au.t,
        );
        let flip = ((kicks / 8) & 1) as usize;
        for _ in 0..self.ticks(style, dt, 30.0, 3) {
            let k = flip * 2 + usize::from(beat > 0.5);
            const DECAY: f32 = 0.82;
            let m = &mut self.milk;
            for (i, &j) in m.map[k].iter().enumerate() {
                m.tmp[i] = if j < 0 { 0.0 } else { m.mk[j as usize] * DECAY };
            }
            std::mem::swap(&mut m.mk, &mut m.tmp);
        }
        let dir = if flip != 0 { -1.0 } else { 1.0 };
        self.milk.rot += dir * dt * (0.25 + 1.5 * level);

        let mut bands = std::mem::take(&mut self.bands);
        resample(&mut bands, &self.au.sm, 48);
        let rmin = (big_w as f64).min(big_h as f64);
        let r0 = rmin * (0.15 + 0.08 * bass + 0.04 * beat);
        let big_r = rmin * 0.3;
        let mk = &mut self.milk.mk;
        let mut plot = |x: f64, y: f64, val: f32| {
            let (xi, yi) = (x as i64, y as i64);
            if xi >= 0 && yi >= 0 && xi < big_w as i64 && yi < big_h as i64 {
                let i = yi as usize * big_w + xi as usize;
                mk[i] = mk[i].max(val);
            }
        };
        let pts = 64.max((2.0 * PI * (r0 + big_r) * 1.6) as usize);
        // wave ring: normalized so a quiet track still bends it
        let wp = wave.iter().fold(0f64, |m, x| m.max(x.abs()));
        let use_wave = wave.len() >= 8 && wp > 0.005 && live > 0.5;
        let rw = r0 * 0.55;
        let rot = self.milk.rot;
        for k in 0..=pts {
            let u = k as f64 / pts as f64;
            let fold = 1.0 - (2.0 * u - 1.0).abs(); // 0→1→0 around the ring
            let b = at(&bands, fold);
            let ang = u * 2.0 * PI + rot;
            let r = r0 + b * big_r;
            plot(cx + ang.cos() * r, cy + ang.sin() * r, 1.0);

            let d = if use_wave {
                wave[((fold * (wave.len() - 1) as f64) as usize).min(wave.len() - 1)] / wp * 0.35
            } else {
                0.12 * (u * 12.0 * PI + t * 2.0).sin() * (0.3 + 2.0 * level)
            };
            let a2 = -u * 2.0 * PI - rot * 1.7;
            let r2 = rw * (1.0 + d);
            plot(cx + a2.cos() * r2, cy + a2.sin() * r2, 0.7);
        }
        if kicks != self.milk.k {
            let r = r0 + big_r * 1.05;
            for k in 0..pts {
                let ang = k as f64 / pts as f64 * 2.0 * PI;
                plot(cx + ang.cos() * r, cy + ang.sin() * r, 1.0);
            }
            self.milk.k = kicks;
        }
        self.bands = bands;
        let hi = (t / 15.0) as usize + (kicks / 4).max(0) as usize;
        let hue_a = MILK_PAL[hi % 6];
        let hue_b = MILK_PAL[(hi + 2) % 6];
        let ramp = [BLUE, BLUE, hue_b, hue_b, hue_a, hue_a, hue_a, bright(hue_a)];
        let mk = std::mem::take(&mut self.milk.mk);
        self.glow(&mk, None, 0.2, &ramp);
        self.milk.mk = mk;
    }
}
