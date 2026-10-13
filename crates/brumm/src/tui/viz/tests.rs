use std::time::Duration;

use super::*;

fn data(kind: &str, n: usize, frame: usize) -> (Vec<f64>, Vec<f64>) {
    match kind {
        "nil" => return (Vec::new(), Vec::new()),
        "silent" => return (vec![0.0; n], vec![0.0; 256]),
        _ => {}
    }
    let mut r: u64 = (frame as u64).wrapping_mul(2654435761).wrapping_add(7);
    let mut next = || {
        r ^= r << 13;
        r ^= r >> 7;
        r ^= r << 17;
        (r >> 11) as f64 / (1u64 << 53) as f64
    };
    let beat = (-((frame % 12) as f64) * 0.25).exp(); // kick every 12 frames
    let mut spec = vec![0.0; n];
    for (i, s) in spec.iter_mut().enumerate() {
        // music-ish: smooth, rolling off toward the highs, bass kicks
        let u = i as f64 / n as f64;
        let mut x = (1.0 - u * 0.7)
            * (0.45
                + 0.25 * (frame as f64 * 0.21 + u * 9.0).sin()
                + 0.15 * (frame as f64 * 0.07 - u * 23.0).sin());
        if u < 0.2 {
            x += beat * 0.5 * (1.0 - u * 5.0);
        }
        *s = (x + 0.06 * next()).clamp(0.0, 1.0);
    }
    let mut wave = Vec::new();
    if kind == "random" {
        wave = (0..512)
            .map(|i| (i as f64 * 0.07 + frame as f64 * 0.3).sin() * 0.6 + (next() - 0.5) * 0.3)
            .collect();
    }
    (spec, wave) // "specOnly": no wave, scope must synthesize
}

/// A visualizer whose clock advances step per frame.
struct Clocked {
    v: Visualizer,
    t: Instant,
    step: Duration,
}

impl Clocked {
    fn new(step: Duration) -> Clocked {
        Clocked {
            v: Visualizer::default(),
            t: Instant::now(),
            step,
        }
    }

    #[allow(clippy::too_many_arguments)] // mirrors render_mix
    fn render(
        &mut self,
        from: Option<usize>,
        style: usize,
        f: f64,
        spec: &[f64],
        wave: &[f64],
        w: usize,
        h: usize,
        playing: bool,
    ) {
        self.t += self.step;
        self.v.clock = Some(self.t);
        self.v.render_mix(from, style, f, spec, wave, w, h, playing);
    }

    fn draw(
        &mut self,
        style: usize,
        spec: &[f64],
        wave: &[f64],
        w: usize,
        h: usize,
        playing: bool,
    ) {
        self.render(None, style, 1.0, spec, wave, w, h, playing);
    }
}

fn frame25() -> Clocked {
    Clocked::new(Duration::from_millis(40))
}

fn check_grid(v: &Visualizer, w: usize, h: usize, what: &str) {
    assert_eq!((v.w, v.h), (w, h), "{what}");
    assert_eq!(v.glyph.len(), w * h, "{what}");
    for (i, &c) in v.col.iter().chain(v.bg.iter()).enumerate() {
        assert!(
            c == 0 || (30..=37).contains(&c) || (90..=97).contains(&c),
            "{what}: cell {i} has color {c}, not one of the theme's 16"
        );
    }
    for &g in &v.glyph {
        assert_eq!(
            unicode_width::UnicodeWidthChar::width(g),
            Some(1),
            "{what}: glyph {g:?} is not one column"
        );
    }
}

/// Every style, at many sizes and inputs, paused and resumed: the grid is
/// exactly the size asked, one column per glyph, theme colors only.
#[test]
fn every_style_at_every_size() {
    let sizes = [
        (80, 24),
        (237, 68),
        (20, 6),
        (1, 1),
        (2, 1),
        (1, 5),
        (3, 2),
        (0, 3),
    ];
    for (style, name) in NAMES.iter().enumerate() {
        for &(w, h) in &sizes {
            for kind in ["nil", "silent", "random", "specOnly"] {
                let mut c = frame25();
                for f in 0..60 {
                    let (spec, wave) = data(kind, 48, f);
                    let playing = !(30..50).contains(&f); // pause, then resume
                    c.draw(style, &spec, &wave, w, h, playing);
                    check_grid(&c.v, w, h, &format!("{name} {w}x{h} {kind}"));
                }
            }
        }
    }
    assert_eq!(NAMES.len(), COUNT);
}

/// A dissolve mixes two styles block by block: the old style at 0, the new
/// one at 1, both in between.
#[test]
fn dissolves() {
    let mut c = frame25();
    let sizes = [(80, 24), (1, 1), (7, 3), (200, 60)];
    for f in 0..80 {
        let (w, h) = sizes[(f / 20) % sizes.len()];
        let (spec, wave) = data("random", 48, f);
        c.render(
            Some(f % COUNT),
            (f * 5 + 3) % COUNT,
            (f % 20) as f64 / 19.0,
            &spec,
            &wave,
            w,
            h,
            true,
        );
        check_grid(&c.v, w, h, "mix");
    }
    let (spec, wave) = data("random", 48, 1);
    let count = |v: &Visualizer| v.glyph.iter().filter(|&&g| g == '▆').count();
    let mut b = frame25();
    b.draw(VZ_LED, &spec, &wave, 60, 20, true);
    let full = count(&b.v);
    let mut m = frame25();
    m.render(Some(VZ_SCOPE), VZ_LED, 0.0, &spec, &wave, 60, 20, true);
    assert_eq!(count(&m.v), 0, "mix at 0 shows the incoming LED style");
    let mut m = frame25();
    m.render(Some(VZ_SCOPE), VZ_LED, 1.0, &spec, &wave, 60, 20, true);
    assert_eq!(count(&m.v), full, "mix at 1 shows all of the LED style");
    let mut m = frame25();
    m.render(Some(VZ_SCOPE), VZ_LED, 0.5, &spec, &wave, 60, 20, true);
    let half = count(&m.v);
    assert!(
        half > 0 && half < full,
        "mix at 0.5 shows {half} of {full} LED segments"
    );
}

/// Drawn at 120 fps with the spectrum arriving at 30 and a kick every half
/// second, the full-screen styles move evenly: about the same number of
/// cells changing every frame, no screen-wide jolt on the beat.
#[test]
fn full_screen_styles_move_smoothly() {
    for style in [VZ_PLASMA, VZ_FIRE, VZ_LAVA] {
        let mut c = Clocked::new(Duration::from_secs(1) / 120);
        let (w, h) = (120, 40);
        let (mut prev, mut pc, mut pb) = (Vec::new(), Vec::new(), Vec::new());
        let mut ch = Vec::new();
        for f in 0..600 {
            let (spec, wave) = data("random", 48, f / 4);
            c.draw(style, &spec, &wave, w, h, true);
            if f > 60 {
                let n = (0..w * h)
                    .filter(|&j| {
                        prev[j] != c.v.glyph[j] || pc[j] != c.v.col[j] || pb[j] != c.v.bg[j]
                    })
                    .count();
                ch.push(n as f64);
            }
            prev.clone_from(&c.v.glyph);
            pc.clone_from(&c.v.col);
            pb.clone_from(&c.v.bg);
        }
        let m = ch.iter().sum::<f64>() / ch.len() as f64;
        let sq = ch.iter().map(|x| x * x).sum::<f64>() / ch.len() as f64;
        let mx = ch.iter().cloned().fold(0.0, f64::max);
        let cv = (sq - m * m).sqrt() / m;
        // lava is mostly still, so its kick swells stand out in cv; what no
        // style may do is repaint most of the screen at once
        assert!(
            !((cv > 0.35 && style != VZ_LAVA) || mx > 0.3 * (w * h) as f64),
            "{}: mean {m:.0} cv {cv:.2} max {mx:.0}: jerky",
            NAMES[style]
        );
    }
}

/// Resizing mid-run and switching styles back and forth keep the grid
/// well formed.
#[test]
fn resize_and_switch() {
    let mut c = frame25();
    let sizes = [(80, 24), (1, 1), (300, 90), (20, 6), (5, 40)];
    for f in 0..200 {
        let (w, h) = sizes[(f / 7) % sizes.len()];
        let (spec, wave) = data("random", 48, f);
        for style in [f % COUNT, (f / 3 + 4) % COUNT] {
            c.draw(style, &spec, &wave, w, h, f % 50 < 40);
            check_grid(&c.v, w, h, NAMES[style]);
        }
    }
}

/// Paused, every style keeps moving (breathing), not freezing.
#[test]
fn idle_breathes() {
    for (style, name) in NAMES.iter().enumerate() {
        let mut c = frame25();
        let mut first = Vec::new();
        let mut changed = false;
        for f in 0..80 {
            c.draw(style, &[], &[], 60, 20, false);
            let s = c.v.rows();
            if f == 20 {
                first = s;
            } else if f > 20 && s != first {
                changed = true;
            }
        }
        assert!(changed, "{name}: the paused screen is static");
    }
}

/// The kick detector catches the synthetic kick every 12 frames, and
/// nothing while paused.
#[test]
fn kicks() {
    let mut c = frame25();
    for f in 0..120 {
        let (spec, wave) = data("random", 48, f);
        c.draw(0, &spec, &wave, 40, 10, true);
    }
    let k = c.v.au.kicks;
    assert!(
        (6..=12).contains(&k),
        "{k} kicks in 120 frames, want about 10"
    );
    for f in 0..60 {
        let (spec, wave) = data("random", 48, f);
        c.draw(0, &spec, &wave, 40, 10, false);
    }
    assert_eq!(c.v.au.kicks, k, "kicks counted while paused");
}

/// The grid goes into the screen's buffer cell for cell.
#[test]
fn blit_fills_the_buffer() {
    use ratatui::layout::Rect;
    let mut c = frame25();
    let (spec, wave) = data("random", 48, 3);
    c.draw(VZ_LED, &spec, &wave, 30, 8, true);
    let mut buf = Buffer::empty(Rect::new(0, 0, 30, 9));
    c.v.blit(&mut buf, 0, 0);
    let row: String = (0..30).map(|x| buf[(x, 7)].symbol().to_string()).collect();
    assert!(row.contains('▆'), "{row:?}");
}
