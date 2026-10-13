// Adapted from vibez (https://github.com/simonepelosi/vibez), MIT License,
// Copyright (c) 2025 Simone Pelosi.

//! The cover as Atkinson-dithered pixel art in sextants.
//!
//! Every cell is a sextant (U+1FB00 block) holding a 2×3 grid of solid
//! sub-pixels, so the effective resolution is 2×width by 3×height — at the
//! usual ~2:1 cell aspect those sub-pixels are close to square. The cover
//! is first reduced to a small k-means palette and then Atkinson-dithered,
//! which diffuses only 6/8 of the error: flat areas stay clean and the
//! result reads as deliberate pixel art rather than noise. A cell can show
//! only two colours, so each one is fitted to the best fg/bg pair among
//! its sub-pixels.

use image::RgbaImage;

use super::{Cell, Picture, Size, sample_grid, u8c};

/// The palette size the cover is reduced to before dithering. 16 keeps
/// small accents (a green cap, a red scarf) while still producing a
/// visible dither texture.
const DITHER_COLORS: usize = 16;

/// How many pixels the k-means palette is fitted on at most: an evenly
/// strided subsample yields the same 16 colours at a fraction of the cost.
const PALETTE_SAMPLES: usize = 4096;

type Rgb = [f64; 3];

/// A "redmean" weighted RGB distance: nearly as cheap as plain Euclidean
/// but much closer to perceived difference.
fn dist2(a: Rgb, b: Rgb) -> f64 {
    let rm = (a[0] + b[0]) / 2.0;
    let (dr, dg, db) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    (2.0 + rm / 256.0) * dr * dr + 4.0 * dg * dg + (2.0 + (255.0 - rm) / 256.0) * db * db
}

fn nearest(p: Rgb, pal: &[Rgb]) -> usize {
    let (mut bi, mut bd) = (0, f64::MAX);
    for (j, &c) in pal.iter().enumerate() {
        let d = dist2(p, c);
        if d < bd {
            bi = j;
            bd = d;
        }
    }
    bi
}

/// k representative colours by k-means. Seeding is maximin (start at the
/// colour nearest the mean, then repeatedly add the pixel farthest from
/// every seed), deterministic and fair to small accent colours.
fn palette(all: &[Rgb], k: usize) -> Vec<Rgb> {
    let step = all.len() / PALETTE_SAMPLES;
    let px: Vec<Rgb> = if step > 1 {
        all.iter().step_by(step).copied().collect()
    } else {
        all.to_vec()
    };
    let mut mean = [0f64; 3];
    for p in &px {
        for c in 0..3 {
            mean[c] += p[c] / px.len() as f64;
        }
    }
    let mut cent = vec![px[nearest(mean, &px)]];
    let mut min_d: Vec<f64> = px.iter().map(|&p| dist2(p, cent[0])).collect();
    while cent.len() < k {
        let mut far = 0;
        for i in 0..px.len() {
            if min_d[i] > min_d[far] {
                far = i;
            }
        }
        if min_d[far] == 0.0 {
            break; // fewer distinct colours than k
        }
        cent.push(px[far]);
        for (i, &p) in px.iter().enumerate() {
            min_d[i] = min_d[i].min(dist2(p, px[far]));
        }
    }
    let mut sums = vec![[0f64; 3]; cent.len()];
    let mut cnt = vec![0f64; cent.len()];
    // Lloyd iterations until no centroid moves by more than about one step.
    for _ in 0..10 {
        sums.iter_mut().for_each(|s| *s = [0.0; 3]);
        cnt.iter_mut().for_each(|c| *c = 0.0);
        for &p in &px {
            let j = nearest(p, &cent);
            for c in 0..3 {
                sums[j][c] += p[c];
            }
            cnt[j] += 1.0;
        }
        let mut moved = false;
        for j in 0..cent.len() {
            if cnt[j] == 0.0 {
                continue;
            }
            let next = [
                sums[j][0] / cnt[j],
                sums[j][1] / cnt[j],
                sums[j][2] / cnt[j],
            ];
            moved = moved || dist2(next, cent[j]) > 9.0;
            cent[j] = next;
        }
        if !moved {
            break;
        }
    }
    cent
}

/// Maps every pixel to the palette, spreading 1/8 of the error to each of
/// six neighbours. Dropping the remaining 2/8 keeps flat areas free of the
/// stray speckles Floyd–Steinberg produces.
fn atkinson(px: &[Rgb], gw: usize, gh: usize, pal: &[Rgb]) -> Vec<Rgb> {
    let mut work = px.to_vec();
    let mut out = vec![[0f64; 3]; px.len()];
    const TAPS: [(isize, usize); 6] = [(1, 0), (2, 0), (-1, 1), (0, 1), (1, 1), (0, 2)];
    for y in 0..gh {
        for x in 0..gw {
            let i = y * gw + x;
            let q = pal[nearest(work[i], pal)];
            out[i] = q;
            let w = work[i];
            for (dx, dy) in TAPS {
                let nx = x as isize + dx;
                let ny = y + dy;
                if nx < 0 || nx >= gw as isize || ny >= gh {
                    continue;
                }
                let j = ny * gw + nx as usize;
                for c in 0..3 {
                    work[j][c] += (w[c] - q[c]) / 8.0;
                }
            }
        }
    }
    out
}

/// The fg/bg pair among the cell's sub-pixel colours that represents all
/// of them with the least error, and the bitmask of sub-pixels in fg.
fn fit_pair(sub: &[Rgb; 6]) -> (Rgb, Rgb, u8) {
    let mut cand: Vec<Rgb> = Vec::with_capacity(6);
    for &s in sub {
        if !cand.contains(&s) {
            cand.push(s);
        }
    }
    if cand.len() == 1 {
        return (cand[0], cand[0], 0);
    }
    let (mut fg, mut bg, mut mask, mut best) = (cand[0], cand[0], 0u8, f64::MAX);
    for i in 0..cand.len() {
        for j in i + 1..cand.len() {
            let (mut e, mut m) = (0.0, 0u8);
            for (k, &s) in sub.iter().enumerate() {
                let (df, db) = (dist2(s, cand[i]), dist2(s, cand[j]));
                if df < db {
                    e += df;
                    m |= 1 << k;
                } else {
                    e += db;
                }
            }
            if e < best {
                best = e;
                fg = cand[i];
                bg = cand[j];
                mask = m;
            }
        }
    }
    (fg, bg, mask)
}

/// The glyph whose filled sub-cells match mask, bit i set for sub-cell i
/// in row-major order. The U+1FB00 block leaves out the four patterns that
/// exist elsewhere.
pub(crate) fn sextant(mask: u8) -> char {
    match mask {
        0 => ' ',
        0b010101 => '▌',
        0b101010 => '▐',
        0b111111 => '█',
        _ => {
            let mut r = 0x1FB00 + mask as u32 - 1;
            if mask > 0b010101 {
                r -= 1;
            }
            if mask > 0b101010 {
                r -= 1;
            }
            char::from_u32(r).unwrap_or(' ')
        }
    }
}

/// img as exactly size.height rows of size.width cells of pixel art.
pub fn render_dithered(img: &RgbaImage, size: Size) -> Picture {
    if size.width == 0 || size.height == 0 || img.width() == 0 || img.height() == 0 {
        return Vec::new();
    }
    let (gw, gh) = (size.width * 2, size.height * 3);
    let px = sample_grid(img, gw, gh);
    let out = atkinson(&px, gw, gh, &palette(&px, DITHER_COLORS));
    let mut rows = Vec::with_capacity(size.height);
    for row in 0..size.height {
        let mut cells = Vec::with_capacity(size.width);
        for col in 0..size.width {
            let mut sub = [[0f64; 3]; 6];
            for (i, s) in sub.iter_mut().enumerate() {
                *s = out[(row * 3 + i / 2) * gw + col * 2 + i % 2];
            }
            let (fg, bg, mask) = fit_pair(&sub);
            cells.push(Cell {
                ch: sextant(mask),
                fg: u8c(fg),
                bg: u8c(bg),
            });
        }
        rows.push(cells);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sextants_cover_every_mask() {
        let mut seen = std::collections::HashSet::new();
        for m in 0..64u8 {
            seen.insert(sextant(m));
        }
        assert_eq!(seen.len(), 64, "every pattern has its own glyph");
        assert_eq!(sextant(1), '\u{1FB00}');
        assert_eq!(sextant(0b111110), '\u{1FB3B}');
    }

    #[test]
    fn renders_exactly_the_size_asked() {
        let img = RgbaImage::from_fn(60, 60, |x, y| {
            image::Rgba([
                (x * 4) as u8,
                (y * 4) as u8,
                if (x / 10 + y / 10) % 2 == 0 { 255 } else { 0 },
                255,
            ])
        });
        let p = render_dithered(
            &img,
            Size {
                width: 20,
                height: 10,
            },
        );
        assert_eq!(p.len(), 10);
        assert!(p.iter().all(|r| r.len() == 20));
        let colors: std::collections::HashSet<[u8; 3]> =
            p.iter().flatten().flat_map(|c| [c.fg, c.bg]).collect();
        assert!(
            colors.len() <= DITHER_COLORS,
            "{} colours for a palette of {DITHER_COLORS}",
            colors.len()
        );
        assert!(
            render_dithered(
                &img,
                Size {
                    width: 0,
                    height: 3
                }
            )
            .is_empty()
        );
    }
}
