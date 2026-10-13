//! lava.

use super::*;

// ─── lava ───────────────────────────────────────────────────────────────
// Lava lamp: seven metaballs drift on slow Lissajous paths through a
// half-block canvas (square pixels, two per cell); where their fields add
// up past one they merge into a blob, shaded from a dithered glow at the
// rim to a hot core. Each ball is as big as its part of the spectrum (the
// bass the biggest), a kick swells them all at once, the loudness hurries
// the drift, and every eighth kick pours a new color into the lamp,
// dissolving over a second and a half.
const BALLS: usize = 7;

#[derive(Default)]
pub(crate) struct Lava {
    ph: f64,
    speed: f64,
    pulse: f64,
    /// pulse, eased in so a kick swells rather than jumps.
    swell: f64,
    k: i32,
    /// The palette, and the one it is dissolving from.
    pal: usize,
    old: usize,
    /// 0 → 1 over the dissolve.
    mix: f64,
    /// Squared distances: ball × column, ball × pixel row.
    dx2: Vec<f32>,
    dy2: Vec<f32>,
}

const LAVA_PAL: [[u8; 5]; 4] = [
    [BLUE, MAGENTA, B_MAGENTA, B_RED, B_YELLOW],
    [RED, RED, B_RED, YELLOW, B_YELLOW],
    [BLUE, BLUE, CYAN, B_CYAN, B_WHITE],
    [GREEN, GREEN, B_GREEN, B_YELLOW, B_WHITE],
];

/// Per ball: x and y frequency, x and y phase.
const BALL_PATH: [[f64; 4]; BALLS] = [
    [0.31, 0.43, 0.0, 1.7],
    [0.23, 0.37, 2.1, 0.4],
    [0.41, 0.29, 4.0, 2.9],
    [0.19, 0.53, 1.1, 5.2],
    [0.37, 0.21, 5.5, 3.3],
    [0.27, 0.47, 3.2, 0.9],
    [0.45, 0.33, 0.7, 4.4],
];

impl Visualizer {
    pub(crate) fn draw_lava(&mut self, dt: f64) {
        let (w, h) = (self.w, self.h);
        let big_p = 2 * h;
        let s = &mut self.lava;
        if s.dx2.len() != BALLS * w || s.dy2.len() != BALLS * big_p {
            s.dx2 = vec![0.0; BALLS * w];
            s.dy2 = vec![0.0; BALLS * big_p];
        }
        let a = &self.au;
        let target = 0.25 + a.live * (0.2 + 1.2 * a.level);
        s.speed += (target - s.speed) * (dt * 1.5).min(1.0);
        s.ph += dt * s.speed;
        if a.kicks != s.k {
            s.k = a.kicks;
            s.pulse = (s.pulse + 0.22).min(0.4);
        }
        s.pulse *= (-dt * 4.0).exp();
        s.swell += (s.pulse - s.swell) * (dt * 12.0).min(1.0);
        let p = (a.kicks / 8).rem_euclid(LAVA_PAL.len() as i32) as usize;
        if p != s.pal {
            s.old = s.pal;
            s.pal = p;
            s.mix = 0.0;
        }
        s.mix = (s.mix + dt / 1.5).min(1.0);

        let (fw, fp) = (w as f64, big_p as f64);
        let base = fw.min(fp) * 0.12;
        let mut r2 = [0f32; BALLS];
        for (k, bp) in BALL_PATH.iter().enumerate() {
            let bx = fw / 2.0 + fw * 0.32 * (s.ph * bp[0] * 2.0 + bp[2]).sin();
            let by = fp / 2.0 + fp * 0.34 * (s.ph * bp[1] * 2.0 + bp[3]).sin();
            let r = base * (0.6 + 1.1 * at(&a.sm, k as f64 / (BALLS - 1) as f64) + s.swell);
            r2[k] = (r * r) as f32;
            for x in 0..w {
                let d = x as f64 + 0.5 - bx;
                s.dx2[k * w + x] = (d * d) as f32 + 1.0;
            }
            for y in 0..big_p {
                let d = y as f64 + 0.5 - by;
                s.dy2[k * big_p + y] = (d * d) as f32;
            }
        }
        let (pal, old) = (&LAVA_PAL[s.pal], &LAVA_PAL[s.old]);
        let mix = s.mix as f32;
        let n = w * h * 2;
        self.pix.clear();
        self.pix.resize(n, 0);
        let s = &self.lava;
        for y in 0..big_p {
            let mut dy = [0f32; BALLS];
            let mut most = 0f32; // the field can't beat this anywhere on the row
            for k in 0..BALLS {
                dy[k] = s.dy2[k * big_p + y];
                most += r2[k] / (dy[k] + 1.0);
            }
            if most < 0.6 {
                continue;
            }
            let by = &BAYER[y & 3];
            for x in 0..w {
                let mut f = 0f32;
                for k in 0..BALLS {
                    f += r2[k] / (s.dx2[k * w + x] + dy[k]);
                }
                if f < 0.6 {
                    continue;
                }
                let d = by[x & 3];
                let p = if d >= mix { old } else { pal }; // a new color pours in dot by dot
                let out = &mut self.pix[y * w + x];
                if f < 1.0 {
                    // the glow around a blob, thinning outward
                    if (f - 0.6) * 2.5 > d {
                        *out = p[0];
                    }
                    continue;
                }
                // depth into the blob: 0 at its skin, toward 1 deep inside
                let t = 1.0 - 1.0 / f;
                *out = p[1 + ((t * 4.2 + d - 0.5) as i64).clamp(0, 3) as usize];
            }
        }
        self.pix_flush(0, h);
    }
}
