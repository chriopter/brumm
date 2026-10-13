//! scope and ridge.

use super::*;

#[derive(Default)]
pub(crate) struct Scope {
    wv: Vec<f64>,
    /// Phosphor, one value per dot.
    ph: Vec<f32>,
    gain: f64,
}

#[derive(Default)]
pub(crate) struct Ridge {
    /// History rows, p points each.
    rg: Vec<f32>,
    live: Vec<f32>,
    noise: Vec<f32>,
    hz: Vec<i64>,
    r: usize,
    p: usize,
    head: usize,
}

// ─── scope ──────────────────────────────────────────────────────────────
// Oscilloscope: the waveform traced as a continuous braille line into a
// phosphor canvas that fades over a third of a second, so the trace leaves
// green afterglow over a dotted graticule. A trigger on a rising zero
// crossing holds periodic sounds still; kicks thicken the beam to white.
// Without wave data (or paused) it synthesizes a calm signal from the
// spectrum.
impl Visualizer {
    pub(crate) fn draw_scope(&mut self, wave: &[f64], dt: f64) {
        let (w, h) = (self.w, self.h);
        let (big_w, big_h) = (w * 2, h * 4);
        if self.scope.ph.len() != big_w * big_h {
            self.scope.ph = vec![0.0; big_w * big_h];
        }
        let decay = (-dt * 7.0).exp() as f32;
        for x in self.scope.ph.iter_mut() {
            *x *= decay;
        }
        let mut s = std::mem::take(&mut self.scope.wv);
        s.clear();
        s.resize(big_w, 0.0);
        let a = &self.au;

        let peak = wave.iter().fold(0f64, |m, x| m.max(x.abs()));
        let m = wave.len();
        if m >= 16 && a.live > 0.5 && peak > 0.005 {
            // Show two thirds of the buffer, starting at the first rising
            // zero crossing after the signal dipped (hysteresis against noise).
            let l = m * 2 / 3;
            let (mut start, mut armed) = ((m - l) / 2, false);
            for j in 1..=m - l {
                if wave[j] < -0.1 * peak {
                    armed = true;
                } else if armed && wave[j - 1] < 0.0 && wave[j] >= 0.0 {
                    start = j;
                    break;
                }
            }
            for (x, v) in s.iter_mut().enumerate() {
                let p = start as f64 + x as f64 * (l - 1) as f64 / (big_w.max(2) - 1) as f64;
                let j = (p as usize).min(m - 1);
                let mut y = wave[j];
                if j + 1 < m {
                    y += (wave[j + 1] - y) * (p - j as f64);
                }
                *v = y;
            }
        } else {
            // A handful of partials, amplitude from band groups.
            const PARTS: usize = 7;
            let mut amp = [0f64; PARTS];
            let nb = a.spec.len();
            let mut norm = 0.0;
            for (k, am) in amp.iter_mut().enumerate() {
                let (lo, hi) = (
                    k * nb / PARTS,
                    ((k + 1) * nb / PARTS).max(k * nb / PARTS + 1),
                );
                *am = a.spec[lo.min(nb)..hi.min(nb)].iter().sum::<f64>() / (hi - lo) as f64;
                norm += *am;
            }
            let t = a.t * 1.5;
            for (x, v) in s.iter_mut().enumerate() {
                let u = x as f64 / big_w as f64 * 2.0 * PI;
                let mut y = 0.0;
                for (k, &am) in amp.iter().enumerate() {
                    if am > 0.0 {
                        let f = (1u64 << k) as f64;
                        y += am * (u * f * 1.5 + t * (k + 1) as f64).sin();
                    }
                }
                if norm > 0.0 {
                    y *= norm.min(1.0) / norm;
                }
                *v = y;
            }
        }
        // Auto-gain so quiet passages still fill the screen; paused, the
        // trace shrinks to a gentle ripple.
        let pk = s.iter().fold(0f64, |m, x| m.max(x.abs()));
        let mut target = if pk > 0.02 { (0.85 / pk).min(4.0) } else { 1.0 };
        target *= 0.3 + 0.7 * a.live;
        if self.scope.gain == 0.0 {
            self.scope.gain = target;
        }
        self.scope.gain += (target - self.scope.gain) * (dt * 4.0).min(1.0);

        let mid = (big_h as f64 - 1.0) / 2.0;
        let amp = mid * self.scope.gain;
        let thick = (0.6 * a.beat) as f32;
        let beat = a.beat;
        let ph = &mut self.scope.ph;
        let mut py: i64 = -1;
        for (x, &v) in s.iter().enumerate() {
            let y = ((mid - v * amp + 0.5) as i64).clamp(0, big_h as i64 - 1);
            let (mut y0, mut y1) = (y, y);
            if py >= 0 {
                if py < y {
                    y0 = py + 1;
                } else if py > y {
                    y1 = py - 1;
                }
            }
            for yy in y0..=y1 {
                ph[yy as usize * big_w + x] = 1.0;
            }
            if thick > 0.2 {
                if y0 > 0 {
                    let i = (y0 - 1) as usize * big_w + x;
                    ph[i] = ph[i].max(thick);
                }
                if y1 < big_h as i64 - 1 {
                    let i = (y1 + 1) as usize * big_w + x;
                    ph[i] = ph[i].max(thick);
                }
            }
            py = y;
        }
        self.scope.wv = s;

        let mut ramp = [GRAY, GRAY, GREEN, GREEN, GREEN, B_GREEN, B_GREEN, B_GREEN];
        if beat > 0.5 {
            ramp[7] = B_WHITE;
        }
        let ph = std::mem::take(&mut self.scope.ph);
        self.glow(&ph, None, 0.1, &ramp);
        self.scope.ph = ph;

        let mid_row = h / 2;
        let gx = (w / 12).max(4);
        let gy = (h / 6).max(2) as i64;
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if self.glyph[i] != ' ' {
                    continue;
                }
                if y == mid_row && x % 2 == 0 {
                    (self.glyph[i], self.col[i]) = ('·', GRAY);
                } else if x % gx == 0 && (y as i64 - mid_row as i64) % gy == 0 {
                    (self.glyph[i], self.col[i]) = ('+', GRAY);
                }
            }
        }
    }
}

// ─── ridge ──────────────────────────────────────────────────────────────
// Unknown Pleasures: the spectrum (bass in the middle, flat noisy edges) is
// frozen into a ridgeline ten times a second and recedes toward a horizon
// in perspective, each line hiding what lies behind it (floating horizon).
// The front line is live; lines fade white → cyan → blue → gray with
// distance.
const RIDGE_PAL: [u8; 6] = [B_WHITE, WHITE, B_CYAN, CYAN, BLUE, GRAY];

impl Visualizer {
    pub(crate) fn draw_ridge(&mut self, style: usize, dt: f64) {
        let (w, h) = (self.w, self.h);
        let (big_w, big_h) = (w * 2, h * 4);
        let r = (big_h / 7).clamp(3, 40);
        let p = (big_w / 4).clamp(16, 128);
        if self.ridge.r != r || self.ridge.p != p {
            self.ridge.rg = vec![0.0; r * p];
            self.ridge.live = vec![0.0; p];
            self.ridge.noise = vec![0.0; p];
            self.ridge.r = r;
            self.ridge.p = p;
            self.ridge.head = 0;
        }
        let mut pushes = self.ticks(style, dt, 10.0, 2);
        if pushes > 0 {
            for i in 0..p {
                self.ridge.noise[i] = self.rf() as f32;
            }
        }
        for i in 0..p {
            let u = i as f64 / (p - 1) as f64;
            let d = (u - 0.5).abs() * 2.0;
            // flat edges, the music in the middle
            let e = ((d - 0.5) / 0.45).clamp(0.0, 1.0);
            let env = 1.0 - e * e * (3.0 - 2.0 * e);
            let n = self.ridge.noise[i] as f64;
            self.ridge.live[i] =
                (env * at(&self.au.sm, d / 0.95) * (0.8 + 0.4 * n) + 0.015 * n) as f32;
        }
        while pushes > 0 {
            self.ridge.head = (self.ridge.head + 1) % r;
            let hd = self.ridge.head;
            let live = self.ridge.live.clone();
            self.ridge.rg[hd * p..(hd + 1) * p].copy_from_slice(&live);
            pushes -= 1;
        }

        self.dot_reset();
        let mut hz = std::mem::take(&mut self.ridge.hz);
        hz.clear();
        hz.resize(big_w, big_h as i64);
        let f = self.acc[style]; // how far the history has slid toward the next slot
        let kp = 3.0 / r as f64;
        let (hy, yb) = (big_h as f64 * 0.12, (big_h - 1) as f64);
        let cx = big_w as f64 / 2.0;
        let beat = self.au.beat;
        let rg = std::mem::take(&mut self.ridge.rg);
        let live = std::mem::take(&mut self.ridge.live);
        let mut line = |v: &mut Visualizer, row: &[f32], d: f64, rank: u8, boost: f64| {
            let s = 1.0 / (1.0 + d * kp);
            let base = hy + (yb - hy) * s;
            let a = big_h as f64 * 0.42 * s * boost;
            let half = cx * (0.3 + 0.68 * s);
            let x0 = cx - half;
            let mut py: i64 = -1;
            let lo_x = (x0 as i64).max(0);
            let hi_x = ((cx + half) as i64).min(big_w as i64 - 1);
            for xd in lo_x..=hi_x {
                let pp = ((xd as f64 - x0) / (2.0 * half)).clamp(0.0, 1.0) * (p - 1) as f64;
                let j = pp as usize;
                let mut val = row[j] as f64;
                if j + 1 < p {
                    val += (row[j + 1] as f64 - val) * (pp - j as f64);
                }
                let y = (base - a * val) as i64;
                let (mut lo, mut hi) = (y, y);
                if py >= 0 {
                    if py < y {
                        lo = py + 1;
                    } else if py > y {
                        hi = py - 1;
                    }
                }
                let xu = xd as usize;
                let mut yy = lo.max(0);
                while yy <= hi && yy < hz[xu] {
                    v.dot(xd, yy, rank);
                    yy += 1;
                }
                hz[xu] = hz[xu].min(y);
                py = y;
            }
        };
        line(self, &live, 0.0, 0, 1.0 + 0.25 * beat);
        for j in 0..r {
            let k = ((self.ridge.head as i64 - j as i64) % r as i64 + r as i64) as usize % r;
            line(
                self,
                &rg[k * p..(k + 1) * p],
                j as f64 + f,
                (1 + j * 5 / r) as u8,
                1.0,
            );
        }
        self.ridge.rg = rg;
        self.ridge.live = live;
        self.ridge.hz = hz;
        self.dot_flush(&RIDGE_PAL);
    }
}
