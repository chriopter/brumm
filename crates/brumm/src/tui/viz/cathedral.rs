//! cathedral.

use std::sync::OnceLock;

use super::*;

// ─── cathedral ──────────────────────────────────────────────────────────
// The window's cathedral (gui/qml/viz/Cathedral.qml) in half blocks: the
// same nave, sun and windows, traced per pixel, the camera drifting as
// there but framed tighter, so the glass gets as many pixels as it can.
// The window stands big in the end wall, set back in three moulded
// orders under the bay's great arch; two piers stand as dark columns at
// the sides, outlined. A pixel of glass takes the piece that covers most
// of it, so lead finer than a pixel never punches a hole. Every point is lit by
// tracing it back along the sunlight to the wall: the glass it meets
// there is the light it gets, so the window lies in color on the floor in
// front of it, dithered as if blurred. The air is marched in a few steps,
// which gives the shafts, each pixel in the color most of the glass on
// its way has. Dust drifts in four sheets and shows only where the sun
// catches it. The sun crosses, clouds pass. Each piece of glass flickers
// with its part of the spectrum, a kick swells the sun, the highs make
// the dust glitter, and every 24th kick brings another church's window,
// piece by piece from the sill up: Chartres, Sainte-Chapelle, an oculus.

/// The room, as in the window's shader: the glass's size and sill, the
/// piers (±PX, from PZ0 back every PDZ, radius PR), the vault.
const WW: f64 = 5.0;
const WH: f64 = 5.0 * H;
const SILL: f64 = 1.4;
const PX: f64 = 3.9;
const PZ0: f64 = -1.4;
const PDZ: f64 = 3.4;
const PR: f64 = 0.42;
const VAULT: f64 = 12.6;

/// The window flat, x −0.5 … 0.5, y 0 … H up from the sill, laid out once
/// per design into a map of MAP_W × MAP_H cells.
const H: f64 = 1.55;
// Leave stone between the complete oculus and the pointed outer arch.
const OCULUS_Y: f64 = 0.93;
const OCULUS_RADIUS: f64 = 0.42;
const MAP_W: usize = 128;
const MAP_H: usize = 198;
/// The pieces are cut coarser than the window's shader cuts them, so that
/// one still shows in a terminal's pixels.
const COARSE: f64 = 0.25;
/// The rose's pieces are mirrored in every petal: cut finer, or one piece
/// would fill a petal's medallion in all twelve.
const ROSE: f64 = 2.0;

/// Map cells: 0 the wall, TRACERY its stone inside the window, LEAD, or a
/// glass color from GLASS (1-based).
const TRACERY: u8 = 254;
const LEAD: u8 = 255;
const B_BLUE: u8 = 94;

/// The old glass's colors as the theme has them: dim and lit, and how much
/// light each lets through. In order: blue, deep, sky, ruby, wine, gold,
/// green, white, purple.
const GLASS: [(u8, u8, f32); 9] = [
    (BLUE, B_BLUE, 0.7),
    (BLUE, BLUE, 0.35),
    (B_BLUE, B_CYAN, 0.9),
    (RED, B_RED, 0.8),
    (RED, RED, 0.45),
    (YELLOW, B_YELLOW, 1.0),
    (GREEN, B_GREEN, 0.6),
    (WHITE, B_WHITE, 1.0),
    (MAGENTA, B_MAGENTA, 0.55),
];
const G_BLUE: u8 = 1;
const G_DEEP: u8 = 2;
const G_SKY: u8 = 3;
const G_RUBY: u8 = 4;
const G_WINE: u8 = 5;
const G_GOLD: u8 = 6;
const G_GREEN: u8 = 7;
const G_WHITE: u8 = 8;
const G_PURPLE: u8 = 9;

const DUST: usize = 64;
const STEPS: usize = 10;

#[derive(Default)]
pub(crate) struct Cathedral {
    /// Seconds it has run, slower while paused; the camera's drift.
    tick: f64,
    pan: f64,
    swell: f64,
    /// How much the spectrum lights the pieces.
    glow: f64,
    k: i32,
    design: usize,
    old: usize,
    /// 0 → 1 as a new window comes in.
    mixing: f64,
    /// Per mote: x, y, z in the room (one of four sheets), its own pace.
    dust: Vec<[f64; 4]>,
}

struct Window {
    /// Per map cell: what is there (see TRACERY).
    col: Vec<u8>,
    /// Per map cell: which of 16 parts of the spectrum the piece hears.
    band: Vec<u8>,
}

fn hash(a: i64, b: i64, salt: u64) -> u64 {
    let mut h = (a as u64).wrapping_mul(0x9E3779B97F4A7C15)
        ^ (b as u64).wrapping_mul(0xC2B2AE3D27D4EB4F)
        ^ salt.wrapping_mul(0x165667B19E3779F9);
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58476D1CE4E5B9);
    h ^ (h >> 29)
}

fn smooth(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn unit(h: u64) -> f64 {
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// Inside a pointed (equilateral) arch: half width w, springing at y =
/// spring, standing on y = foot; > 0 inside.
fn arch(x: f64, y: f64, w: f64, foot: f64, spring: f64) -> f64 {
    let body = if y < spring {
        w - x.abs()
    } else {
        2.0 * w - (x.abs() + w).hypot(y - spring)
    };
    body.min(y - foot)
}

/// The pieces: cells round jittered points, s per unit. How far the
/// nearest lead is (in units), and the piece's own random number.
fn pieces(x: f64, y: f64, s: f64, seed: (f64, f64)) -> (f64, f64) {
    let (qx, qy) = ((x + seed.0) * s, (y + seed.1) * s);
    let (cx, cy) = (qx.floor() as i64, qy.floor() as i64);
    let (mut d1, mut d2, mut id) = (f64::MAX, f64::MAX, 0);
    for j in -1..=1 {
        for i in -1..=1 {
            let h = hash(cx + i, cy + j, 1);
            let ox = (cx + i) as f64 + 0.15 + 0.7 * unit(h) - qx;
            let oy = (cy + j) as f64 + 0.15 + 0.7 * unit(h.rotate_right(23)) - qy;
            let d = ox * ox + oy * oy;
            if d < d1 {
                (d2, d1, id) = (d1, d, h);
            } else if d < d2 {
                d2 = d;
            }
        }
    }
    ((d2.sqrt() - d1.sqrt()) * 0.5 / s, unit(id.rotate_right(41)))
}

/// One region's glass at (x, y): kind picks the colors as the window's
/// shader does (ground, figures, beads, rings, petals), s the pieces' size.
fn glass(x: f64, y: f64, seed: (f64, f64), s: f64, kind: u8) -> (u8, u8) {
    let s = s * COARSE;
    let (edge, r) = pieces(x, y, s, seed);
    if edge < 0.07 / s {
        return (LEAD, 0);
    }
    let c = match kind {
        0 => match r {
            r if r < 0.50 => G_BLUE,
            r if r < 0.85 => G_DEEP,
            r if r < 0.92 => G_SKY,
            r if r < 0.97 => G_RUBY,
            _ => G_GOLD,
        },
        1 => match r {
            r if r < 0.32 => G_RUBY,
            r if r < 0.47 => G_GREEN,
            r if r < 0.62 => G_GOLD,
            r if r < 0.75 => G_WHITE,
            r if r < 0.87 => G_PURPLE,
            _ => G_SKY,
        },
        2 => match r {
            r if r < 0.5 => G_RUBY,
            r if r < 0.8 => G_GOLD,
            _ => G_WHITE,
        },
        3 => {
            if r < 0.6 {
                G_GOLD
            } else {
                G_WHITE
            }
        }
        _ => match r {
            r if r < 0.4 => G_WINE,
            r if r < 0.7 => G_RUBY,
            _ => G_PURPLE,
        },
    };
    (c, ((r * 7.13).fract() * 16.0) as u8)
}

/// A pointed light of medallions: (x, y) from its foot, w its half width.
fn lancet(x: f64, y: f64, w: f64, tall: f64, seed: (f64, f64)) -> (u8, u8) {
    let mut edge = w - x.abs();
    if y > tall {
        edge = edge.min(2.0 * w - (x.abs() + w).hypot(y - tall));
    }
    if edge < 0.032 {
        // the border of beads along its edge
        return glass(x, y, (seed.0 + 3.0, seed.1 + 3.0), 70.0, 2);
    }
    let step = w * 1.9;
    let row = (y / step).floor();
    let bar = ((y / step + 0.5).fract() - 0.5).abs() * step;
    let d = x.hypot(y - (row + 0.5) * step);
    let rad = w * 0.72;
    if y < tall && d < rad {
        if d > rad - 0.03 {
            return glass(x, y, (seed.0 + 7.0, seed.1 + 7.0), 90.0, 3);
        }
        return glass(x, y, (seed.0 + row, seed.1 + row), 60.0, 1);
    }
    if y > tall {
        let hd = x.hypot(y - tall - w * 0.55);
        if hd < w * 0.42 {
            let kind = if hd < w * 0.2 { 3 } else { 4 };
            return glass(x, y, (seed.0 + 9.0, seed.1 + 9.0), 60.0, kind);
        }
    }
    if bar < 0.008 {
        return (LEAD, 0); // an iron saddle bar
    }
    glass(x, y, seed, 46.0, 0)
}

/// A rose of twelve petals round (dx, dy), radius rr; None off it.
fn rose(x: f64, y: f64, dx: f64, dy: f64, rr: f64, seed: (f64, f64)) -> Option<(u8, u8)> {
    let (vx, vy) = ((x - dx) / rr, (y - dy) / rr);
    let r = vx.hypot(vy);
    if r > 1.0 {
        return None;
    }
    let sec = 2.0 * PI / 12.0;
    let a = vy.atan2(vx) + PI / 2.0;
    let fa = (a.rem_euclid(sec) - sec * 0.5).abs();
    let (fx, fy) = (r * fa.cos(), r * fa.sin());
    // the stone: rim, the ring round the eye, spokes, the outer cusps
    let oc = (fx - 0.82).hypot(fy);
    let stone = r > 0.955
        || (r - 0.27).abs() < 0.03
        || (r > 0.27 && (sec * 0.5 - fa).sin() * r < 0.022)
        || (r - 0.70).abs() < 0.022
        || (r > 0.70 && (oc - 0.115).abs() < 0.022);
    if stone {
        return Some((TRACERY, 0));
    }
    let (px, py) = (fx * rr, fy * rr);
    Some(if r < 0.27 {
        if (r - 0.1).abs() < 0.02 {
            (LEAD, 0)
        } else {
            glass(
                px,
                py,
                (seed.0 + 2.0, seed.1 + 2.0),
                60.0,
                if r < 0.1 { 4 } else { 3 },
            )
        }
    } else if r < 0.70 {
        let pd = (fx - 0.49).hypot(fy);
        if (pd - 0.16).abs() < 0.02 {
            (LEAD, 0)
        } else if pd < 0.16 {
            glass(px, py, (seed.0 + 5.0, seed.1 + 5.0), 70.0 * ROSE, 1)
        } else {
            glass(px, py, (seed.0 + 4.0, seed.1 + 4.0), 55.0 * ROSE, 0)
        }
    } else if oc < 0.115 {
        glass(px, py, (seed.0 + 6.0, seed.1 + 6.0), 80.0 * ROSE, 4)
    } else {
        glass(px, py, (seed.0 + 8.0, seed.1 + 8.0), 70.0 * ROSE, 0)
    })
}

/// A whole window at (x, y): Chartres, Sainte-Chapelle or an oculus.
fn window_at(x: f64, y: f64, design: usize) -> (u8, u8) {
    if arch(x, y, 0.5, 0.0, H - 0.866) < 0.0 {
        return (0, 0);
    }
    let rose_or = |r: Option<(u8, u8)>| r.unwrap_or((TRACERY, 0));
    match design {
        0 => {
            let side = if x < 0.0 { -1.0 } else { 1.0 };
            let (qx, qy) = (x - side * 0.215, y - 0.05);
            if arch(qx, qy, 0.165, 0.0, 0.62) > 0.0 {
                return lancet(qx, qy, 0.165, 0.62, (side * 3.0, 1.0));
            }
            rose_or(rose(x, y, 0.0, 1.17, 0.27, (11.0, 2.0)))
        }
        1 => {
            let i = ((x + 0.5) / 0.333).floor().clamp(0.0, 2.0);
            let (qx, qy) = (x - (-0.333 + i * 0.333), y - 0.04);
            let tall = if i == 1.0 { 0.80 } else { 0.68 };
            if arch(qx, qy, 0.135, 0.0, tall) > 0.0 {
                return lancet(qx, qy, 0.135, tall, (i * 5.0, 7.0));
            }
            rose(x, y, -0.17, 1.235, 0.105, (1.0, 9.0))
                .or_else(|| rose(x, y, 0.17, 1.235, 0.105, (2.0, 9.0)))
                .or_else(|| rose(x, y, 0.0, 1.40, 0.075, (3.0, 9.0)))
                .unwrap_or((TRACERY, 0))
        }
        _ => {
            if let Some(g) = rose(x, y, 0.0, OCULUS_Y, OCULUS_RADIUS, (21.0, 5.0)) {
                return g;
            }
            let i = ((x + 0.5) / 0.2).floor().clamp(0.0, 4.0);
            let (qx, qy) = (x - (-0.4 + i * 0.2), y - 0.04);
            if arch(qx, qy, 0.072, 0.0, 0.26) > 0.0 {
                return lancet(qx, qy, 0.072, 0.26, (i * 2.0, 13.0));
            }
            (TRACERY, 0)
        }
    }
}

/// The three windows, and the order a new one comes in by (0..1 per cell:
/// from the sill up, a little at random).
fn windows() -> &'static ([Window; 3], Vec<f32>) {
    static W: OnceLock<([Window; 3], Vec<f32>)> = OnceLock::new();
    W.get_or_init(|| {
        let one = |d: usize| {
            let mut col = vec![0; MAP_W * MAP_H];
            let mut band = vec![0; MAP_W * MAP_H];
            for j in 0..MAP_H {
                let y = (j as f64 + 0.5) * H / MAP_H as f64;
                for i in 0..MAP_W {
                    let x = (i as f64 + 0.5) / MAP_W as f64 - 0.5;
                    (col[j * MAP_W + i], band[j * MAP_W + i]) = window_at(x, y, d);
                }
            }
            Window { col, band }
        };
        let mut order = vec![0f32; MAP_W * MAP_H];
        for j in 0..MAP_H {
            let y = (j as f64 + 0.5) * H / MAP_H as f64;
            for i in 0..MAP_W {
                let x = (i as f64 + 0.5) / MAP_W as f64 - 0.5;
                let h = hash(((x + 0.5) * 40.0) as i64, (y * 40.0) as i64, 9);
                order[j * MAP_W + i] = (unit(h) * 0.35 + y / H * 0.65) as f32;
            }
        }
        ([one(0), one(1), one(2)], order)
    })
}

type V3 = [f64; 3];

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: V3) -> V3 {
    mul(a, 1.0 / dot(a, a).sqrt().max(1e-9))
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// The frame's room: camera, sun, and which window shows.
struct Room<'a> {
    now: &'a Window,
    old: &'a Window,
    order: &'a [f32],
    mixing: f32,
    ro: V3,
    fw: V3,
    rt: V3,
    up: V3,
    focal: f64,
    roll: (f64, f64),
    l: V3,
}

impl Room<'_> {
    /// What the end wall holds at (x, y), and its piece's band.
    fn wall(&self, x: f64, y: f64) -> (u8, u8) {
        let u = (x + WW * 0.5) / WW;
        let v = (y - SILL) / WH;
        if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
            return (0, 0);
        }
        let i = (v * MAP_H as f64) as usize * MAP_W + (u * MAP_W as f64) as usize;
        let w = if self.mixing < 1.0 && self.order[i] > self.mixing * 1.3 - 0.15 {
            self.old
        } else {
            self.now
        };
        (w.col[i], w.band[i])
    }

    /// What a pixel of the end wall shows round (x, y), size wide: lead
    /// finer than a pixel must not punch holes in the glass, so the pixel
    /// takes the piece most of it covers, and is stone only where the
    /// stone covers most of it.
    fn glass(&self, x: f64, y: f64, size: f64) -> (u8, u8) {
        let mut n = [0u8; 10];
        let mut band = [0u8; 10];
        let mut stone = 0;
        for j in 0..3 {
            for i in 0..3 {
                let (c, b) = self.wall(
                    x + (i as f64 - 1.0) * size / 3.0,
                    y + (j as f64 - 1.0) * size / 3.0,
                );
                if (1..=9).contains(&c) {
                    n[c as usize] += 1;
                    band[c as usize] = b;
                } else if c != LEAD {
                    stone += 1;
                }
            }
        }
        let best = (1..10).max_by_key(|&c| n[c]).unwrap_or(1);
        match (stone, n[best]) {
            (_, 0) => self.wall(x, y),
            (s, _) if s >= 5 => self.wall(x, y),
            _ => (best as u8, band[best]),
        }
    }

    /// The glass whose light reaches p, traced back along the sun, nudged
    /// sideways by blur (in wall units) for a dithered softness; and how
    /// far it came.
    fn light(&self, p: V3, blur: (f64, f64)) -> (u8, f64) {
        let s = p[2] / self.l[2];
        let q = sub(p, mul(self.l, s));
        let soft = 0.03 * s;
        match self.wall(q[0] + blur.0 * soft, q[1] + blur.1 * soft) {
            (c, _) if (1..=9).contains(&c) => (c, s),
            _ => (0, s),
        }
    }

    /// The end wall's stone at (x, y), dithered by (d, d2): ashlar, the
    /// window's reveal in three moulded orders lit by its glass, the bay's
    /// great arch, darker above it and toward the vault. tracery: the
    /// stone between the lights, haloed by them.
    fn stone(&self, x: f64, y: f64, tracery: bool, near: f64, sun: f64, (d, d2): (f64, f64)) -> u8 {
        // the glass's halo: a piece of it close by, if any
        let halo = |r: f64| match self.wall(x + (d - 0.5) * r, y + (d2 - 0.5) * r) {
            (c, _) if (1..=9).contains(&c) => GLASS[c as usize - 1].0,
            _ => 0,
        };
        if tracery {
            return 0;
        }
        let dark = smooth(14.0, 9.5, y);
        let out = -arch(x / WW, (y - SILL) / WW, 0.5, 0.0, H - 0.866);
        if out > 0.0 && out < 0.1 {
            let order = out / 0.033;
            let f = order.fract();
            if !(0.15..0.85).contains(&f) {
                return 0; // the groove between two orders
            }
            let deep = 1.0 - order.floor() / 3.0;
            if order < 1.0 {
                // the innermost order catches the glass's colors
                let c = halo(out * WW + 0.6);
                if c != 0 && d < 0.3 * sun {
                    return c;
                }
            }
            let roll = (f * PI).sin();
            return if d < (0.4 + 0.6 * deep * sun) * roll * roll * dark {
                GRAY
            } else {
                0
            };
        }
        let wa = arch(x, y, PX - PR - 0.05, 0.0, 6.6);
        if wa > -0.4 && wa < 0.05 {
            // the bay's arch, its mouldings ridge by ridge
            let m = 0.55 + 0.45 * (wa * 48.0).cos();
            return if m > 0.9 && d < dark { GRAY } else { 0 };
        }
        // the ashlar, only a hint of its courses near the window
        let mut lit = 0.12 * near * dark;
        if wa < -0.4 {
            lit *= 0.6; // above the arch, further from the light
        }
        if mortar(x, y) && d < lit { GRAY } else { 0 }
    }

    /// The pixel at screen point (sx, sy), on the window shader's scale.
    fn ray(&self, sx: f64, sy: f64) -> V3 {
        let (c, s) = self.roll;
        let (x, y) = (c * sx - s * sy, s * sx + c * sy);
        norm(add(
            add(mul(self.fw, self.focal), mul(self.rt, x)),
            mul(self.up, y),
        ))
    }

    /// Where a point of the room falls on the screen, if in front.
    fn project(&self, p: V3) -> Option<(f64, f64)> {
        let v = sub(p, self.ro);
        let cz = dot(v, self.fw);
        if cz <= 0.2 {
            return None;
        }
        let (x, y) = (
            dot(v, self.rt) / cz * self.focal,
            dot(v, self.up) / cz * self.focal,
        );
        let (c, s) = self.roll;
        Some((c * x + s * y, -s * x + c * y))
    }
}

/// How far a ray goes to the nearest pier, and the normal's x and z there.
fn piers(ro: V3, rd: V3) -> (f64, f64, f64) {
    let mut best = (f64::MAX, 0.0, 0.0);
    let a = rd[0] * rd[0] + rd[2] * rd[2];
    if a < 1e-12 {
        return best;
    }
    for i in 0..4 {
        let cx = if i < 2 { -PX } else { PX };
        let cz = PZ0 - PDZ * (i % 2) as f64;
        let (ox, oz) = (ro[0] - cx, ro[2] - cz);
        let b = ox * rd[0] + oz * rd[2];
        let cc = ox * ox + oz * oz - PR * PR;
        let disc = b * b - a * cc;
        if disc > 0.0 {
            let t = (-b - disc.sqrt()) / a;
            if t > 0.0 && t < best.0 {
                best = (t, (ox + rd[0] * t) / PR, (oz + rd[2] * t) / PR);
            }
        }
    }
    best
}

/// The ashlar's mortar at (x, y) on a wall: true on a joint.
fn mortar(x: f64, y: f64) -> bool {
    let row = (y / 0.42).floor();
    let bx = x / 0.78 + if row as i64 % 2 == 0 { 0.0 } else { 0.5 };
    let (fx, fy) = (bx.rem_euclid(1.0), (y / 0.42).rem_euclid(1.0));
    !(0.05..0.95).contains(&fx) || !(0.1..0.9).contains(&fy)
}

impl Visualizer {
    pub(crate) fn draw_cathedral(&mut self, dt: f64) {
        let (w, h) = (self.w, self.h);
        let big_p = 2 * h;
        let (maps, order) = windows();
        let a = &self.au;
        let s = &mut self.cathedral;
        s.tick += dt * (0.3 + 0.7 * a.live);
        s.pan += dt; // steady camera motion, independent of playback and beats
        if a.kicks != s.k {
            s.k = a.kicks;
            s.swell = (s.swell + 0.35).min(0.7);
        }
        s.swell *= (-dt * 2.5).exp();
        s.glow += (a.live * (0.5 + 0.8 * a.level) - s.glow) * (dt * 2.0).min(1.0);
        let d = (a.kicks / 24).rem_euclid(3) as usize;
        if d != s.design {
            s.old = s.design;
            s.design = d;
            s.mixing = 0.0;
        }
        s.mixing = (s.mixing + dt / 4.0).min(1.0);
        if s.dust.len() != DUST {
            s.dust = (0..DUST)
                .map(|i| {
                    let h = hash(i as i64, 3, 11);
                    [
                        unit(h) * 5.0 - 2.5,
                        unit(h.rotate_right(13)) * 9.0,
                        -0.6 - (i % 4) as f64 * 1.5, // four sheets, as in the window
                        unit(h.rotate_right(43)),
                    ]
                })
                .collect();
        }
        let (tick, t) = (s.tick, s.pan);
        for m in s.dust.iter_mut() {
            m[0] += dt * 0.04 * (tick * 0.11 + m[3] * 30.0).sin();
            m[1] = (m[1] + dt * (0.04 + 0.03 * m[3])).rem_euclid(9.0);
        }

        // The same slow, two-minute pan as the window's shader.
        let tau = std::f64::consts::TAU;
        let ro = [
            0.45 * (t * tau / 180.0).sin(),
            2.6 + 0.10 * (t * tau / 210.0 + 1.0).sin(),
            -13.0 + 0.35 * (t * tau / 240.0 + 2.0).sin(),
        ];
        let aim = [
            1.6 * (t * tau / 120.0).sin(),
            4.9 + 0.2 * (t * tau / 160.0).sin(),
            0.0,
        ];
        let fw = norm(sub(aim, ro));
        let rt = norm(cross([0.0, 1.0, 0.0], fw));
        let roll = 0.004 * (t * tau / 200.0).sin();
        let sunx = 0.38 + 0.3 * (tick * 0.021).sin();
        let cloud = (0.6 + 0.5 * (tick * 0.13).sin() * (tick * 0.071 + 1.3).sin()).clamp(0.0, 1.0);
        let sun = (0.62 + 0.38 * cloud) * (1.0 + s.swell);
        let room = Room {
            now: &maps[s.design],
            old: &maps[s.old],
            order,
            mixing: s.mixing as f32,
            ro,
            fw,
            rt,
            up: cross(fw, rt),
            focal: 1.45,
            roll: (roll.cos(), roll.sin()),
            l: norm([sunx, -1.35, -1.0]),
        };
        let mut band = [0f64; 16];
        for (i, b) in band.iter_mut().enumerate() {
            *b = at(&a.sm, 0.05 + 0.75 * i as f64 / 15.0);
        }
        let (glow, high, live) = (s.glow, a.high, a.live);

        let (fw_, fp) = (w as f64, big_p as f64);
        let aspect = fw_ / fp.max(1.0);
        self.pix.clear();
        self.pix.resize(w * big_p, 0);
        for py in 0..big_p {
            let by = &BAYER[py & 3];
            let by2 = &BAYER[(py + 2) & 3];
            let sy = 0.5 - (py as f64 + 0.5) / fp;
            for px in 0..w {
                let d = by[px & 3] as f64;
                let d2 = by2[(px + 1) & 3] as f64;
                let sx = ((px as f64 + 0.5) / fw_ - 0.5) * aspect;
                let rd = room.ray(sx, sy);
                // what the ray meets: the end wall, the floor, a pier
                let tw = if rd[2] > 0.0 {
                    -ro[2] / rd[2]
                } else {
                    f64::MAX
                };
                let tf = if rd[1] < 0.0 {
                    -ro[1] / rd[1]
                } else {
                    f64::MAX
                };
                let tv = if rd[1] > 0.0 {
                    (VAULT - ro[1]) / rd[1]
                } else {
                    f64::MAX
                };
                let (tp, nx, nz) = piers(ro, rd);
                let th = tw.min(tf).min(tp).min(tv);
                let mut out = 0u8;
                let mut glass_here = false;
                if th < f64::MAX {
                    let p = add(ro, mul(rd, th));
                    // the window's glow on the stone round it
                    let dw = p[0].hypot(p[1] - (SILL + WH * 0.5)).hypot(p[2]);
                    let near = sun / (1.0 + dw * dw * 0.1);
                    if th == tw {
                        match room.glass(p[0], p[1], th / (room.focal * fp)) {
                            (c, b) if (1..=9).contains(&c) => {
                                let (dim, lit, _) = GLASS[c as usize - 1];
                                let l = 0.45 * sun + 0.8 * glow * band[b as usize];
                                out = if l > d { lit } else { dim };
                                glass_here = true;
                            }
                            (c, _) => {
                                out = room.stone(p[0], p[1], c == TRACERY, near, sun, (d, d2))
                            }
                        }
                    } else if th == tf {
                        let (c, far) = room.light(p, (d - 0.5, d2 - 0.5));
                        let fade = (-far * 0.06).exp() * sun * 1.15;
                        if c != 0 && fade * GLASS[c as usize - 1].2 as f64 > d {
                            out = GLASS[c as usize - 1].0;
                        } else {
                            // marble in squares set on the diagonal
                            let (u, v) = (
                                (p[0] - p[2]) * std::f64::consts::FRAC_1_SQRT_2 / 0.9,
                                (p[0] + p[2]) * std::f64::consts::FRAC_1_SQRT_2 / 0.9,
                            );
                            let light = (u.floor() + v.floor()) as i64 % 2 == 0;
                            if light && d < 0.08 + 0.25 * near {
                                out = GRAY;
                            }
                        }
                    } else if th == tp {
                        // a pier: dark, its outline a thin line where the
                        // ray grazes it
                        let graze = (nx * rd[0] + nz * rd[2]).abs() / rd[0].hypot(rd[2]).max(1e-9);
                        let fade = smooth(14.0, 9.5, p[1]);
                        // the band of the column one pixel wide at each side
                        let r = PR * room.focal * fp / (th * dot(rd, fw)).max(1e-6);
                        let edge = (1.0 - (1.0 - 1.0 / r.max(1.0)).powi(2)).sqrt();
                        if graze < edge && d < 0.3 + fade {
                            out = GRAY;
                        }
                    }
                }
                // the shafts: march through the air, gathering what crosses it
                if !glass_here {
                    let far = th.min(22.0);
                    let step = far / STEPS as f64;
                    let mut sum = 0.0;
                    let mut by_color = [0f32; 9];
                    for i in 0..STEPS {
                        let p = add(ro, mul(rd, step * (i as f64 + d2)));
                        if p[2] > -0.02 {
                            break;
                        }
                        let (c, s) = room.light(p, (0.0, 0.0));
                        if c != 0 {
                            let dens = 0.8
                                + 0.3
                                    * vsin((p[0] * 1.3 + p[2] * 0.7 - tick * 0.1) as f32) as f64
                                    * vsin((p[1] * 1.1 + tick * 0.07) as f32) as f64;
                            let wt = GLASS[c as usize - 1].2 as f64 * (-s * 0.06).exp() * dens;
                            by_color[c as usize - 1] += 1.0; // its color by area, not brightness
                            sum += wt;
                        }
                    }
                    let air = sum * step * sun * (0.3 + 0.15 * self.au.bass);
                    if air > d {
                        // the color most of the light on the way has
                        let mut c = 0;
                        for k in 1..9 {
                            if by_color[k] > by_color[c] {
                                c = k;
                            }
                        }
                        out = GLASS[c].0;
                    }
                }
                self.pix[py * w + px] = out;
            }
        }

        // dust, seen only where the sun catches it
        let glitter = 1.5 + 6.0 * high * live;
        let pix_at = |x: f64, y: f64| -> Option<usize> {
            let px = ((x / aspect + 0.5) * fw_).floor();
            let py = ((0.5 - y) * fp).floor();
            (px >= 0.0 && py >= 0.0 && px < fw_ && py < fp).then(|| py as usize * w + px as usize)
        };
        for m in &self.cathedral.dust {
            let p = [m[0], m[1], m[2]];
            let (c, _) = room.light(p, (0.0, 0.0));
            if c == 0 {
                continue;
            }
            let tw = (tick * glitter + m[3] * 60.0).sin();
            if tw < 0.1 - 0.4 * high {
                continue;
            }
            if let Some(i) = room.project(p).and_then(|(x, y)| pix_at(x, y)) {
                self.pix[i] = if tw > 0.7 { B_WHITE } else { B_YELLOW };
            }
        }
        self.pix_flush(0, h);
    }
}

#[cfg(test)]
mod geometry_tests {
    use super::*;

    #[test]
    fn all_roses_fit_inside_the_pointed_arch() {
        for (cx, cy, radius) in [
            (0.0, 1.17, 0.27),
            (-0.17, 1.235, 0.105),
            (0.17, 1.235, 0.105),
            (0.0, 1.40, 0.075),
            (0.0, OCULUS_Y, OCULUS_RADIUS),
        ] {
            for step in 0..720 {
                let angle = 2.0 * PI * step as f64 / 720.0;
                let x = cx + radius * angle.cos();
                let y = cy + radius * angle.sin();
                assert!(
                    arch(x, y, 0.5, 0.0, H - 0.866) > 0.015,
                    "rose at ({cx}, {cy}) touches the arch at ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn oculus_clears_the_lancets_below_it() {
        let lancet_top = 0.04 + 0.26 + 3.0_f64.sqrt() * 0.072;
        assert!(OCULUS_Y - OCULUS_RADIUS > lancet_top + 0.02);
    }
}
