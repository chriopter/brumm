//! The listener every visualizer style hears through, in the terminal and
//! in the window alike: the spectrum blended with an idle breath while
//! paused, a smoothed copy, falling peaks, band energies and a kick
//! detector.

/// Peaks hold this long after being pushed up, then fall with gravity.
const PEAK_HOLD: f64 = 0.35;
/// Screen heights per second².
pub const GRAVITY: f64 = 1.8;

#[derive(Clone, Debug, Default)]
pub struct Listener {
    /// Animation seconds.
    pub t: f64,
    /// 1 playing … 0 paused, eased so a pause fades out.
    pub live: f64,
    /// The input (clamped) blended with the idle breath.
    pub spec: Vec<f64>,
    /// `spec` with a fast attack and an eased release.
    pub sm: Vec<f64>,
    /// Peak-hold markers over `spec`.
    pub peak: Vec<f64>,
    vel: Vec<f64>,
    hold: Vec<f64>,
    /// Smoothed energies of the low eighth, the mids, the highs, all.
    pub bass: f64,
    pub mid: f64,
    pub high: f64,
    pub level: f64,
    /// 1 on a kick, decaying over ~0.4 s.
    pub beat: f64,
    /// Kicks so far, for events like palette changes.
    pub kicks: i32,
    /// Seconds since the last kick.
    pub since: f64,
    avg: f64,
    prev: f64,
}

impl Listener {
    /// Moves the listener on by `dt` seconds with the spectrum `input`
    /// (0..1 per band; 48 bands are assumed when it is empty). A step is
    /// at most 0.1 s; a negative or not-a-number one is no step.
    pub fn listen(&mut self, input: &[f64], dt: f64, playing: bool) {
        let dt = if dt.is_nan() { 0.0 } else { dt.clamp(0.0, 0.1) };
        self.t += dt;
        let live = if playing { 1.0 } else { 0.0 };
        self.live += (live - self.live) * (dt * 4.0).min(1.0);
        let n = if input.is_empty() { 48 } else { input.len() };
        if self.spec.len() != n {
            self.spec = vec![0.0; n];
            self.sm = vec![0.0; n];
            self.peak = vec![0.0; n];
            self.vel = vec![0.0; n];
            self.hold = vec![0.0; n];
        }
        // Paused: a low rolling hill that swells and settles over six
        // seconds, so the screen breathes instead of freezing.
        let breath = 0.5 - 0.5 * (self.t * 2.0 * std::f64::consts::PI / 6.0).cos();
        let (up, down) = ((dt * 30.0).min(1.0), (dt * 9.0).min(1.0));
        let nl = (n / 8).max(1);
        let (mut bass, mut mid, mut high, mut level) = (0.0, 0.0, 0.0, 0.0);
        for i in 0..n {
            let mut x = match input.get(i) {
                Some(&v) if v > 0.0 => v.min(1.0), // also drops NaN
                _ => 0.0,
            };
            if self.live < 0.999 {
                let u = (i as f64 + 0.5) / n as f64;
                let idle = (0.03 + 0.09 * breath)
                    * (1.0 - 0.6 * u)
                    * (0.7 + 0.3 * (u * 9.0 - self.t * 0.7).sin());
                x = idle + (x - idle) * self.live;
            }
            self.spec[i] = x;
            let k = if x > self.sm[i] { up } else { down };
            self.sm[i] += (x - self.sm[i]) * k;
            if self.hold[i] > 0.0 {
                self.hold[i] -= dt;
            } else {
                self.vel[i] += GRAVITY * dt;
                self.peak[i] -= self.vel[i] * dt;
            }
            if x >= self.peak[i] {
                self.peak[i] = x;
                self.vel[i] = 0.0;
                self.hold[i] = PEAK_HOLD;
            }
            level += x;
            if i < nl {
                bass += x;
            } else if i < n / 2 {
                mid += x;
            } else {
                high += x;
            }
        }
        bass /= nl as f64;
        mid /= (n / 2).saturating_sub(nl).max(1) as f64;
        high /= (n - n / 2).max(1) as f64;
        level /= n as f64;
        let e = (dt * 12.0).min(1.0);
        self.bass += (bass - self.bass) * e;
        self.mid += (mid - self.mid) * e;
        self.high += (high - self.high) * e;
        self.level += (level - self.level) * e;

        // A kick is the bass jumping well above its recent average while
        // rising, at most five per second.
        self.since += dt;
        if dt > 0.0 {
            if playing && bass > self.avg * 1.3 + 0.03 && bass > self.prev && self.since > 0.2 {
                self.beat = 1.0;
                self.kicks += 1;
                self.since = 0.0;
            }
            self.avg += (bass - self.avg) * (dt * 2.5).min(1.0);
            self.prev = bass;
        }
        self.beat *= (-dt * 6.0).exp();
    }
}

/// Peak-hold markers that hold briefly, then fall with gravity.
#[derive(Clone, Debug, Default)]
pub struct Peaks {
    val: Vec<f64>,
    vel: Vec<f64>,
    hold: Vec<f64>,
}

impl Peaks {
    pub fn update(&mut self, b: &[f64], dt: f64, hold: f64) -> &[f64] {
        if self.val.len() != b.len() {
            self.val = vec![0.0; b.len()];
            self.vel = vec![0.0; b.len()];
            self.hold = vec![0.0; b.len()];
        }
        for (i, &x) in b.iter().enumerate() {
            if self.hold[i] > 0.0 {
                self.hold[i] -= dt;
            } else {
                self.vel[i] += GRAVITY * dt;
                self.val[i] -= self.vel[i] * dt;
            }
            if x >= self.val[i] {
                self.val[i] = x;
                self.vel[i] = 0.0;
                self.hold[i] = hold;
            }
        }
        &self.val
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_breathes_and_kicks_need_playback() {
        let mut l = Listener::default();
        let quiet = vec![0.0; 48];
        for _ in 0..200 {
            l.listen(&quiet, 0.04, false);
        }
        assert!(
            l.spec.iter().all(|&x| x > 0.0 && x < 0.2),
            "idle breath: {:?}",
            &l.spec[..4]
        );
        assert_eq!(l.kicks, 0);
        // A kick every half second, playing.
        let mut loud = vec![0.0; 48];
        for step in 0..100 {
            let on = step % 12 < 2;
            for (i, v) in loud.iter_mut().enumerate() {
                *v = if on && i < 6 { 1.0 } else { 0.05 };
            }
            l.listen(&loud, 0.04, true);
        }
        assert!(l.kicks >= 5, "kicks: {}", l.kicks);
        assert!(l.live > 0.9);
    }

    #[test]
    fn nan_and_overload_are_clamped() {
        let mut l = Listener::default();
        l.listen(&[f64::NAN, 5.0, -1.0], 0.04, true);
        assert!(
            l.spec
                .iter()
                .all(|x| x.is_finite() && *x <= 1.0 && *x >= 0.0)
        );
        let t = l.t;
        for dt in [f64::NAN, -3.0, f64::NEG_INFINITY] {
            l.listen(&[0.5; 3], dt, true);
        }
        l.listen(&[0.5; 3], f64::INFINITY, true);
        assert!(
            (l.t - t - 0.1).abs() < 1e-12,
            "bad steps add nothing, a huge one at most 0.1 s"
        );
        assert!(
            [l.live, l.bass, l.level, l.beat]
                .iter()
                .all(|x| x.is_finite())
        );
        assert!(l.sm.iter().chain(&l.peak).all(|x| x.is_finite()));
    }

    #[test]
    fn peaks_hold_then_fall() {
        let mut p = Peaks::default();
        p.update(&[1.0], 0.01, 0.5);
        assert_eq!(p.update(&[0.0], 0.1, 0.5)[0], 1.0);
        for _ in 0..30 {
            p.update(&[0.0], 0.05, 0.5);
        }
        assert!(p.update(&[0.0], 0.05, 0.5)[0] < 0.5);
    }
}
