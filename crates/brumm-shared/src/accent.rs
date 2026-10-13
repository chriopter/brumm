//! The accent a cover lends to what plays: its dominant vivid color, made
//! readable on the background. The terminal player and the window pick
//! it the same way.

pub type Rgb = [u8; 3];

/// The cover's dominant vivid color, none for a gray cover. A sample of
/// pixels votes by hue, each weighed by its saturation; grays, near black
/// and near white do not vote. `rgba` is `height` rows of `stride` bytes,
/// four per pixel.
pub fn vivid(rgba: &[u8], width: usize, height: usize, stride: usize) -> Option<Rgb> {
    const BINS: usize = 24;
    let mut w = [0f64; BINS];
    let mut sum = [[0f64; 3]; BINS];
    let step = (width.max(height) / 48).max(1);
    let mut n = 0usize;
    for y in (0..height).step_by(step) {
        for x in (0..width).step_by(step) {
            let o = y * stride + x * 4;
            let Some(p) = rgba.get(o..o + 3) else {
                continue;
            };
            let (r, g, b) = (
                p[0] as f64 / 255.0,
                p[1] as f64 / 255.0,
                p[2] as f64 / 255.0,
            );
            let (h, s, v) = hsv(r, g, b);
            n += 1;
            if s < 0.25 || v < 0.2 {
                continue;
            }
            let i = (h * BINS as f64) as usize % BINS;
            let wt = s * s * v;
            w[i] += wt;
            sum[i][0] += r * wt;
            sum[i][1] += g * wt;
            sum[i][2] += b * wt;
        }
    }
    // The strongest hue with its neighbors, so a hue split over two bins
    // is not outvoted.
    let (mut best, mut score) = (0, 0.0);
    for i in 0..BINS {
        let s = w[(i + BINS - 1) % BINS] / 2.0 + w[i] + w[(i + 1) % BINS] / 2.0;
        if s > score {
            best = i;
            score = s;
        }
    }
    if n == 0 || score < 0.03 * n as f64 {
        return None; // a few specks of color: gray
    }
    let mut tw = 0.0;
    let mut rgb = [0f64; 3];
    for i in [(best + BINS - 1) % BINS, best, (best + 1) % BINS] {
        tw += w[i];
        for k in 0..3 {
            rgb[k] += sum[i][k];
        }
    }
    let (h, s, l) = hsl(rgb[0] / tw, rgb[1] / tw, rgb[2] / tw);
    Some(from_hsl(h, s.max(0.55), l)) // averaging dulls it: vivid again
}

/// Moves c's lightness until it stands out from bg as text does (4.5:1).
/// Without a known background it keeps to the middle, which reads on dark
/// and light alike.
pub fn readable(c: Rgb, bg: Option<Rgb>) -> Rgb {
    let (h, s, mut l) = hsl(
        c[0] as f64 / 255.0,
        c[1] as f64 / 255.0,
        c[2] as f64 / 255.0,
    );
    let Some(bg) = bg else {
        return from_hsl(h, s, l.clamp(0.45, 0.65));
    };
    // Lighter on a dark background, darker on a light one.
    let dir = if contrast([0, 0, 0], bg) > contrast([255, 255, 255], bg) {
        -0.02
    } else {
        0.02
    };
    let mut c = c;
    for _ in 0..50 {
        if contrast(c, bg) >= 4.5 || l <= 0.0 || l >= 1.0 {
            break;
        }
        l = (l + dir).clamp(0.0, 1.0);
        c = from_hsl(h, s, l);
    }
    c
}

/// The WCAG contrast ratio of two colors.
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

fn luminance(c: Rgb) -> f64 {
    let lin = |v: u8| {
        let f = v as f64 / 255.0;
        if f <= 0.03928 {
            f / 12.92
        } else {
            ((f + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

/// Hue 0–1, saturation and value 0–1.
fn hsv(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let (hi, lo) = (r.max(g).max(b), r.min(g).min(b));
    let s = if hi > 0.0 { (hi - lo) / hi } else { 0.0 };
    (hue(r, g, b, hi, lo), s, hi)
}

fn hsl(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let (hi, lo) = (r.max(g).max(b), r.min(g).min(b));
    let l = (hi + lo) / 2.0;
    let d = hi - lo;
    let s = if d > 0.0 {
        d / (1.0 - (2.0 * l - 1.0).abs())
    } else {
        0.0
    };
    (hue(r, g, b, hi, lo), s.min(1.0), l)
}

fn hue(r: f64, g: f64, b: f64, hi: f64, lo: f64) -> f64 {
    let d = hi - lo;
    if d == 0.0 {
        return 0.0;
    }
    let mut h = if hi == r {
        ((g - b) / d) % 6.0
    } else if hi == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    if h < 0.0 {
        h += 6.0;
    }
    h / 6.0
}

fn from_hsl(h: f64, s: f64, l: f64) -> Rgb {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
    let (r, g, b) = match (h * 6.0) as i64 % 6 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let o = l - c / 2.0;
    let u = |v: f64| ((v + o).clamp(0.0, 1.0) * 255.0).round() as u8;
    [u(r), u(g), u(b)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(f: impl Fn(usize, usize) -> Rgb) -> Vec<u8> {
        let mut out = Vec::new();
        for y in 0..64 {
            for x in 0..64 {
                let c = f(x, y);
                out.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        out
    }

    #[test]
    fn gray_has_none_and_red_is_red() {
        let gray = image(|x, _| [x as u8 * 3, x as u8 * 3, x as u8 * 3]);
        assert_eq!(vivid(&gray, 64, 64, 256), None);
        let red = image(|x, _| if x < 40 { [200, 30, 30] } else { [20, 20, 20] });
        let c = vivid(&red, 64, 64, 256).expect("a vivid red");
        assert!(c[0] > 150 && c[1] < 80 && c[2] < 80, "{c:?}");
    }

    #[test]
    fn readable_contrasts_with_the_background() {
        let dark_blue = [20, 20, 120];
        let on_black = readable(dark_blue, Some([0, 0, 0]));
        assert!(contrast(on_black, [0, 0, 0]) >= 4.5, "{on_black:?}");
        let on_white = readable([250, 250, 120], Some([255, 255, 255]));
        assert!(contrast(on_white, [255, 255, 255]) >= 4.5, "{on_white:?}");
        let unknown = readable([250, 250, 250], None);
        assert!(unknown[0] < 200);
    }
}
