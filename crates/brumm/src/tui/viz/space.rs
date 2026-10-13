//! stars and wire.

use std::sync::OnceLock;

use super::*;

// ─── stars ──────────────────────────────────────────────────────────────
// Warp starfield on a braille canvas, so stars glide a quarter cell at a
// time: they fly out of the center while the field slowly rolls, speed
// eased toward the loudness and kicked into hyperspace on every beat, when
// the streaks stretch long. Distance sets color (gray → blue → cyan →
// white); the nearest stars swell to fat dots.
#[derive(Clone, Copy)]
struct Star {
    x: f64,
    y: f64,
    z: f64,
}

#[derive(Default)]
pub(crate) struct Stars {
    st: Vec<Star>,
    speed: f64,
    rol: f64,
}

const STAR_PAL: [u8; 5] = [B_WHITE, B_CYAN, CYAN, BLUE, GRAY];

impl Visualizer {
    pub(crate) fn draw_stars(&mut self, dt: f64) {
        let (w, h) = (self.w, self.h);
        let (big_w, big_h) = ((w * 2) as f64, (h * 4) as f64);
        let (cx, cy) = (big_w / 2.0, big_h / 2.0);
        let s_scale = big_h / 2.0; // dots per unit at z = 1; braille dots are square
        let r = (cx / s_scale).hypot(1.0) * 1.05;
        let max_n = ((big_w * big_h) as usize / 50).clamp(80, 3000);
        while self.stars.st.len() < max_n {
            let st = Star {
                x: (self.rf() * 2.0 - 1.0) * r,
                y: (self.rf() * 2.0 - 1.0) * r,
                z: 0.05 + self.rf() * 0.95,
            };
            self.stars.st.push(st);
        }
        self.stars.st.truncate(max_n);
        let (live, level, beat) = (self.au.live, self.au.level, self.au.beat);
        let target = 0.06 + live * (0.25 + 1.6 * level) + 3.5 * beat;
        self.stars.speed += (target - self.stars.speed) * (dt * 5.0).min(1.0);
        self.stars.rol += dt * (0.03 + 0.3 * level);
        let (cs, sn) = (self.stars.rol.cos(), self.stars.rol.sin());
        let speed = self.stars.speed;
        let tail = speed * 0.06;

        self.dot_reset();
        for i in 0..self.stars.st.len() {
            let mut st = self.stars.st[i];
            st.z -= speed * dt;
            if st.z < 0.03 {
                st = Star {
                    x: (self.rf() * 2.0 - 1.0) * r,
                    y: (self.rf() * 2.0 - 1.0) * r,
                    z: 0.6 + self.rf() * 0.4,
                };
            }
            let (px, py) = (st.x * cs - st.y * sn, st.x * sn + st.y * cs);
            let (hx, hy) = (cx + px / st.z * s_scale, cy + py / st.z * s_scale);
            if hx < -1.0 || hy < -1.0 || hx >= big_w + 1.0 || hy >= big_h + 1.0 {
                st.z = 0.0; // respawn next frame
                self.stars.st[i] = st;
                continue;
            }
            self.stars.st[i] = st;
            let rk: u8 = if st.z > 0.75 {
                4
            } else if st.z > 0.5 {
                3
            } else if st.z > 0.3 {
                2
            } else if st.z > 0.15 {
                1
            } else {
                0
            };
            let tz = st.z + tail;
            self.dot_line(cx + px / tz * s_scale, cy + py / tz * s_scale, hx, hy, rk);
            if st.z < 0.2 {
                let (x, y) = (hx as i64, hy as i64);
                self.dot(x + 1, y, rk);
                self.dot(x, y + 1, rk);
                self.dot(x + 1, y + 1, rk);
            }
        }
        self.dot_flush(&STAR_PAL);
    }
}

// ─── wire ───────────────────────────────────────────────────────────────
// Vector-display 3D: a wireframe spins in perspective on a braille
// phosphor that fades in a twenty-fifth of a second, so fast turns smear
// into motion blur. Three shapes take turns (every 16 kicks or 24
// seconds): an icosahedron whose vertices spike out with their spectrum
// band around a counter-rotating inner one, a tesseract turning through
// the fourth dimension, and a torus whose tube swells with the spectrum
// around its ring. Near edges burn white, far ones fade to blue; the bass
// pumps the size, every kick throws in a spin that eases off, every fourth
// recolors the beam.
const TORUS_M: usize = 20; // segments around the ring
const TORUS_S: usize = 8; // around the tube
const TORUS_N: usize = TORUS_M * TORUS_S;

pub(crate) struct Wire {
    /// Phosphor, one value per dot.
    c: Vec<f32>,
    /// Cell rows with anything lit.
    rows: Vec<bool>,
    ax: f64,
    ay: f64,
    az: f64,
    aw: f64,
    spin: f64,
    k: i32,
    /// Projected vertices: dots, dots, depth 0 near…1 far.
    px: [f64; TORUS_N],
    py: [f64; TORUS_N],
    pz: [f64; TORUS_N],
}

impl Default for Wire {
    fn default() -> Self {
        Wire {
            c: Vec::new(),
            rows: Vec::new(),
            ax: 0.0,
            ay: 0.0,
            az: 0.0,
            aw: 0.0,
            spin: 0.0,
            k: 0,
            px: [0.0; TORUS_N],
            py: [0.0; TORUS_N],
            pz: [0.0; TORUS_N],
        }
    }
}

const WIRE_RAMP: [[u8; 8]; 3] = [
    [GRAY, BLUE, BLUE, CYAN, CYAN, B_CYAN, B_CYAN, B_WHITE],
    [
        GRAY, BLUE, MAGENTA, MAGENTA, B_MAGENTA, B_MAGENTA, B_WHITE, B_WHITE,
    ],
    [
        GRAY, GREEN, GREEN, B_GREEN, B_GREEN, B_YELLOW, B_YELLOW, B_WHITE,
    ],
];

struct Shapes {
    ico: [[f64; 3]; 12],
    ico_edges: Vec<(usize, usize)>,
    tes_edges: Vec<(usize, usize)>,
}

fn shapes() -> &'static Shapes {
    static S: OnceLock<Shapes> = OnceLock::new();
    S.get_or_init(|| {
        let phi = (1.0 + 5f64.sqrt()) / 2.0;
        let n = (1.0 + phi * phi).sqrt();
        let mut ico = [[0f64; 3]; 12];
        let mut k = 0;
        for s1 in [-1.0, 1.0] {
            for s2 in [-1.0, 1.0] {
                ico[k] = [0.0, s1 / n, s2 * phi / n];
                ico[k + 1] = [s1 / n, s2 * phi / n, 0.0];
                ico[k + 2] = [s2 * phi / n, 0.0, s1 / n];
                k += 3;
            }
        }
        let mut ico_edges = Vec::new();
        for i in 0..12 {
            for j in i + 1..12 {
                let d: f64 = (0..3).map(|c| (ico[i][c] - ico[j][c]).powi(2)).sum();
                if (d.sqrt() - 2.0 / n).abs() < 1e-6 {
                    ico_edges.push((i, j));
                }
            }
        }
        let mut tes_edges = Vec::new();
        for i in 0..16usize {
            for b in 0..4 {
                let j = i ^ (1 << b);
                if j > i {
                    tes_edges.push((i, j));
                }
            }
        }
        Shapes {
            ico,
            ico_edges,
            tes_edges,
        }
    })
}

impl Visualizer {
    pub(crate) fn draw_wire(&mut self, dt: f64) {
        let (w, h) = (self.w, self.h);
        let (big_w, big_h) = (w * 2, h * 4);
        let s = &mut self.wire;
        if s.c.len() != big_w * big_h || s.rows.len() != h {
            s.c = vec![0.0; big_w * big_h];
            s.rows = vec![false; h];
        }
        decay_rows(&mut s.c, big_w, &mut s.rows, (-dt * 25.0).exp() as f32);
        let a = &self.au;
        if a.kicks != s.k {
            s.k = a.kicks;
            s.spin = (s.spin + 0.9).min(2.0);
        }
        s.spin *= (-dt * 3.0).exp();
        let sp = 0.3 + a.live * 0.6 * a.level + s.spin;
        s.ay += dt * sp;
        s.ax += dt * sp * 0.61;
        s.az += dt * sp * 0.23;
        s.aw += dt * (0.3 + 0.6 * a.mid + 0.4 * s.spin);

        // rotation matrix Rz·Rx·Ry
        let (cx, sx) = (s.ax.cos(), s.ax.sin());
        let (cy, sy) = (s.ay.cos(), s.ay.sin());
        let (cz, sz) = (s.az.cos(), s.az.sin());
        let m = [
            [cz * cy - sz * sx * sy, -sz * cx, cz * sy + sz * sx * cy],
            [sz * cy + cz * sx * sy, cz * cx, sz * sy - cz * sx * cy],
            [-cx * sy, sx, cx * cy],
        ];
        let (ox, oy) = (big_w as f64 / 2.0, big_h as f64 / 2.0);
        let scale = (big_w as f64).min(big_h as f64) * 0.3 * (1.0 + 0.16 * a.bass + 0.1 * a.beat);
        const DIST: f64 = 3.2;
        let put = |s: &mut Wire, i: usize, x: f64, y: f64, z: f64, inv: bool| {
            let (bx, by, bz) = if inv {
                // the transpose: the opposite turn
                (
                    m[0][0] * x + m[1][0] * y + m[2][0] * z,
                    m[0][1] * x + m[1][1] * y + m[2][1] * z,
                    m[0][2] * x + m[1][2] * y + m[2][2] * z,
                )
            } else {
                (
                    m[0][0] * x + m[0][1] * y + m[0][2] * z,
                    m[1][0] * x + m[1][1] * y + m[1][2] * z,
                    m[2][0] * x + m[2][1] * y + m[2][2] * z,
                )
            };
            let f = DIST / (bz + DIST + 0.001) * scale;
            s.px[i] = ox + bx * f;
            s.py[i] = oy + by * f;
            s.pz[i] = clamp((bz + 1.4) / 2.8, 0.0, 1.0);
        };
        let edge = |s: &mut Wire, i: usize, j: usize, bright: f32| {
            let b0 = bright * (1.0 - 0.72 * s.pz[i]) as f32;
            let b1 = bright * (1.0 - 0.72 * s.pz[j]) as f32;
            let (x0, y0, x1, y1) = (s.px[i], s.py[i], s.px[j], s.py[j]);
            plot(&mut s.c, &mut s.rows, big_w, big_h, x0, y0, x1, y1, b0, b1);
        };
        let vert = |s: &mut Wire, i: usize, b: f32| {
            let (x, y) = (s.px[i] as i64, s.py[i] as i64);
            for d in 0..4 {
                let (xx, yy) = (x + (d & 1), y + (d >> 1));
                if xx >= 0 && yy >= 0 && xx < big_w as i64 && yy < big_h as i64 {
                    let j = yy as usize * big_w + xx as usize;
                    s.c[j] = s.c[j].max(b * (1.0 - 0.6 * s.pz[i]) as f32);
                    s.rows[yy as usize >> 2] = true;
                }
            }
        };
        let spark = (0.55 + 0.45 * (a.high * 2.5).min(1.0)) as f32;
        let sh = shapes();
        match ((a.t / 24.0) as i64 + (a.kicks / 16) as i64).rem_euclid(3) {
            0 => {
                // icosahedron with spectrum spikes around a smaller, opposite one
                for (i, p) in sh.ico.iter().enumerate() {
                    let r = 1.0 + 0.4 * at(&a.sm, i as f64 / 11.0);
                    put(s, i, p[0] * r, p[1] * r, p[2] * r, false);
                    put(s, 12 + i, p[0] * 0.42, p[1] * 0.42, p[2] * 0.42, true);
                }
                for &(i, j) in &sh.ico_edges {
                    edge(s, i, j, 1.0);
                    edge(s, 12 + i, 12 + j, 0.75);
                }
                for i in 0..12 {
                    vert(s, i, spark);
                }
            }
            1 => {
                // tesseract: turn in the xw and yw planes, project 4D → 3D
                let (c1, s1) = (s.aw.cos(), s.aw.sin());
                let (c2, s2) = ((s.aw * 0.7).cos(), (s.aw * 0.7).sin());
                let inner = 1.0 + 0.35 * a.bass;
                for i in 0..16 {
                    let mut p = [-1f64; 4];
                    for (b, v) in p.iter_mut().enumerate() {
                        if i >> b & 1 != 0 {
                            *v = 1.0;
                        }
                    }
                    p[3] *= inner;
                    let (x, ww) = (p[0] * c1 - p[3] * s1, p[0] * s1 + p[3] * c1);
                    let (y, ww2) = (p[1] * c2 - ww * s2, p[1] * s2 + ww * c2);
                    let k = 0.55 * 2.4 / (2.4 - ww2 * 0.6);
                    put(s, i, x * k, y * k, p[2] * k, false);
                }
                for &(i, j) in &sh.tes_edges {
                    edge(s, i, j, 1.0);
                }
                for i in 0..16 {
                    vert(s, i, spark);
                }
            }
            _ => {
                // torus, the tube swelling with the spectrum around the ring
                for i in 0..TORUS_M {
                    let u = i as f64 / TORUS_M as f64 * 2.0 * PI;
                    let f = (i as f64 / TORUS_M as f64 * 2.0 - 1.0).abs(); // mirrored: bass at one side
                    let tube = 0.3 * (1.0 + 1.1 * at(&a.sm, 1.0 - f));
                    let (cu, su) = (u.cos(), u.sin());
                    for j in 0..TORUS_S {
                        let t = j as f64 / TORUS_S as f64 * 2.0 * PI + s.aw;
                        let r = 0.82 + tube * t.cos();
                        put(s, i * TORUS_S + j, r * cu, r * su, tube * t.sin(), false);
                    }
                }
                for i in 0..TORUS_M {
                    for j in 0..TORUS_S {
                        let k = i * TORUS_S + j;
                        edge(s, k, i * TORUS_S + (j + 1) % TORUS_S, 0.8);
                        edge(s, k, ((i + 1) % TORUS_M) * TORUS_S + j, 1.0);
                    }
                }
            }
        }
        let ramp = WIRE_RAMP[(a.kicks / 4).rem_euclid(3) as usize];
        let (c, rows) = (std::mem::take(&mut s.c), std::mem::take(&mut s.rows));
        self.glow(&c, Some(&rows), 0.12, &ramp);
        self.wire.c = c;
        self.wire.rows = rows;
    }
}
