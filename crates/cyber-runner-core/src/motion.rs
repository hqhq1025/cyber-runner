//! Time-based motion along a [`Track`].
//!
//! The old Python engine pre-sliced each segment into `int(len / (v*dt))`
//! steps and slept `dt` per step. Truncation made short segments run up to
//! ~2× too fast, and network latency added on top of the sleep made the
//! effective pace drift. Here position is a pure function of *distance
//! travelled*, and distance is integrated from the real elapsed time, so the
//! pace is exact no matter how often (or how irregularly) we tick.
//!
//! Every run gets a fresh random seed, so no two runs produce the same
//! trace. Human-like variation (smooth, deterministic for a given seed):
//! - per-run pace factor (± `run_pace_jitter`) and start offset along the
//!   track (0..`start_jitter_m`)
//! - per-lap pace factor (± `lap_pace_jitter`)
//! - slow in-lap speed wobble (± `speed_wobble`)
//! - sideways position = per-run lane (± `lane_m`) + slow drift
//!   (± `lateral_jitter_m`) + fine GPS-like wobble (± `fine_jitter_m`),
//!   all continuous across lap boundaries so there is never a jump.
//!
//! Distance accuracy: sideways movement makes the emitted path slightly
//! longer or shorter than the drawn line (inner/outer lane on a bend). We
//! integrate along the line at `v / stretch`, where `stretch` is the local
//! ratio of emitted path to line length, so the *emitted* trace — what the
//! campus app measures — moves at exactly the requested speed. The reported
//! distance is the length of the emitted trace itself.

use serde::{Deserialize, Serialize};

use crate::geo::LatLng;
use crate::track::Track;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Variation {
    /// Fractional per-run pace change, e.g. 0.015 = ±1.5 %.
    pub run_pace_jitter: f64,
    /// Fractional per-lap pace change, e.g. 0.04 = ±4 %.
    pub lap_pace_jitter: f64,
    /// Fractional smooth in-lap speed wobble.
    pub speed_wobble: f64,
    /// Per-run constant sideways offset ("which lane"), ± meters.
    pub lane_m: f64,
    /// Slow sideways drift from the lane, ± meters.
    pub lateral_jitter_m: f64,
    /// Fine, faster sideways wobble (GPS-like), ± meters.
    pub fine_jitter_m: f64,
    /// Random start offset along the track, 0..meters.
    pub start_jitter_m: f64,
}

impl Default for Variation {
    fn default() -> Self {
        Self {
            run_pace_jitter: 0.015,
            lap_pace_jitter: 0.04,
            speed_wobble: 0.03,
            lane_m: 1.5,
            lateral_jitter_m: 2.0,
            fine_jitter_m: 0.6,
            start_jitter_m: 8.0,
        }
    }
}

impl Variation {
    pub const NONE: Variation = Variation {
        run_pace_jitter: 0.0,
        lap_pace_jitter: 0.0,
        speed_wobble: 0.0,
        lane_m: 0.0,
        lateral_jitter_m: 0.0,
        fine_jitter_m: 0.0,
        start_jitter_m: 0.0,
    };

    /// "低" realism preset.
    pub const LOW: Variation = Variation {
        run_pace_jitter: 0.008,
        lap_pace_jitter: 0.02,
        speed_wobble: 0.015,
        lane_m: 1.0,
        lateral_jitter_m: 1.0,
        fine_jitter_m: 0.4,
        start_jitter_m: 5.0,
    };

    /// "高" realism preset.
    pub const HIGH: Variation = Variation {
        run_pace_jitter: 0.025,
        lap_pace_jitter: 0.07,
        speed_wobble: 0.05,
        lane_m: 2.5,
        lateral_jitter_m: 3.5,
        fine_jitter_m: 1.0,
        start_jitter_m: 15.0,
    };

    fn clamped(self) -> Self {
        let c = |v: f64, hi: f64| if v.is_finite() { v.clamp(0.0, hi) } else { 0.0 };
        Self {
            run_pace_jitter: c(self.run_pace_jitter, 0.1),
            lap_pace_jitter: c(self.lap_pace_jitter, 0.2),
            speed_wobble: c(self.speed_wobble, 0.2),
            lane_m: c(self.lane_m, 5.0),
            lateral_jitter_m: c(self.lateral_jitter_m, 10.0),
            fine_jitter_m: c(self.fine_jitter_m, 3.0),
            start_jitter_m: c(self.start_jitter_m, 30.0),
        }
    }
}

/// Speed limits accepted by the engine (m/s).
pub const MIN_SPEED_MPS: f64 = 0.5;
/// Hard cap: 3'05"/km. Campus-run apps typically void a run whose pace is
/// faster than 3'00"/km, so neither the chosen speed nor the realism
/// variation (per-run, per-lap, wobble) may ever exceed this.
pub const MAX_SPEED_MPS: f64 = 1000.0 / 185.0;

pub fn clamp_speed(v: f64) -> f64 {
    if v.is_finite() {
        v.clamp(MIN_SPEED_MPS, MAX_SPEED_MPS)
    } else {
        MIN_SPEED_MPS
    }
}

/// One simulated fix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fix {
    pub position: LatLng,
    /// Total distance of the emitted trace since start, meters.
    pub distance_m: f64,
    /// Completed laps.
    pub laps: u32,
    /// Progress in the current lap, 0..1.
    pub lap_progress: f64,
    /// Instantaneous speed of the emitted trace, m/s.
    pub speed_mps: f64,
}

#[derive(Debug, Clone)]
pub struct Motion {
    track: Track,
    base_speed: f64,
    variation: Variation,
    seed: u64,
    /// Start position along the line.
    s0: f64,
    /// Current position along the line.
    s: f64,
    /// Length of the emitted trace so far.
    path: f64,
}

/// Spacing of the noise control points along the run.
const LATERAL_NOISE_SPACING_M: f64 = 45.0;
const FINE_NOISE_SPACING_M: f64 = 11.0;
const SPEED_NOISE_SPACING_M: f64 = 160.0;
/// Half-length of the chord used to orient lateral offsets.
const NORMAL_CHORD_M: f64 = 4.0;
/// Half-width of the finite difference used to measure path stretch.
const STRETCH_H_M: f64 = 0.5;

impl Motion {
    pub fn new(track: Track, speed_mps: f64, variation: Variation, seed: u64) -> Self {
        let variation = variation.clamped();
        let s0 = variation.start_jitter_m * (unit(seed ^ 0x57A7, 0) + 1.0) * 0.5;
        Self {
            track,
            base_speed: clamp_speed(speed_mps),
            variation,
            seed,
            s0,
            s: s0,
            path: 0.0,
        }
    }

    pub fn track(&self) -> &Track {
        &self.track
    }

    pub fn base_speed(&self) -> f64 {
        self.base_speed
    }

    /// Takes effect immediately and smoothly (position stays continuous).
    pub fn set_speed(&mut self, speed_mps: f64) {
        self.base_speed = clamp_speed(speed_mps);
    }

    /// Takes effect immediately. The sideways offset may shift by a few
    /// meters once; that step is not counted as distance.
    pub fn set_variation(&mut self, v: Variation) {
        self.variation = v.clamped();
    }

    /// Length of the emitted trace so far, meters.
    pub fn distance_m(&self) -> f64 {
        self.path
    }

    fn lap_index(&self, s: f64) -> u32 {
        ((s - self.s0) / self.track.lap_length_m()).floor().max(0.0) as u32
    }

    /// Speed of the emitted trace at line position `s`.
    fn speed_at(&self, s: f64) -> f64 {
        let v = &self.variation;
        let run = 1.0 + v.run_pace_jitter * unit(self.seed ^ 0x2B1, 0);
        let lap = 1.0 + v.lap_pace_jitter * unit(self.seed ^ 0x1A9, self.lap_index(s) as u64);
        let wobble =
            1.0 + v.speed_wobble * value_noise(self.seed ^ 0x5EED, s / SPEED_NOISE_SPACING_M);
        clamp_speed(self.base_speed * run * lap * wobble)
    }

    /// Sideways offset from the line at `s`, meters.
    fn offset_at(&self, s: f64) -> f64 {
        let v = &self.variation;
        v.lane_m * unit(self.seed ^ 0x1A4E, 0)
            + v.lateral_jitter_m * value_noise(self.seed ^ 0x1A7E, s / LATERAL_NOISE_SPACING_M)
            + v.fine_jitter_m * value_noise(self.seed ^ 0xF1E, s / FINE_NOISE_SPACING_M)
    }

    /// Emitted position at line position `s`, local meters.
    fn xy_at(&self, s: f64) -> (f64, f64) {
        let p = self.track.at(s);
        let off = self.offset_at(s);
        if off == 0.0 {
            return (p.x, p.y);
        }
        // Offset along the normal of a short chord rather than of the
        // current segment: the segment normal flips instantly at a corner,
        // which would make the position jump sideways.
        let (a, b) = (
            self.track.at(s - NORMAL_CHORD_M),
            self.track.at(s + NORMAL_CHORD_M),
        );
        let (cx, cy) = (b.x - a.x, b.y - a.y);
        let n = (cx * cx + cy * cy).sqrt();
        let (tx, ty) = if n > 1e-6 {
            (cx / n, cy / n)
        } else {
            (p.tx, p.ty)
        };
        (p.x - ty * off, p.y + tx * off)
    }

    /// Emitted path length per meter of line around `s`.
    fn stretch_at(&self, s: f64) -> f64 {
        let a = self.xy_at(s - STRETCH_H_M);
        let b = self.xy_at(s + STRETCH_H_M);
        let d = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt() / (2.0 * STRETCH_H_M);
        d.clamp(0.5, 2.0)
    }

    /// Line speed (ds/dt) at `s`.
    fn line_speed_at(&self, s: f64) -> f64 {
        let v = self.speed_at(s);
        if self.offset_is_zero() {
            v
        } else {
            v / self.stretch_at(s)
        }
    }

    fn offset_is_zero(&self) -> bool {
        let v = &self.variation;
        v.lane_m == 0.0 && v.lateral_jitter_m == 0.0 && v.fine_jitter_m == 0.0
    }

    /// Advance by `dt` seconds of wall time and return the new fix.
    pub fn advance(&mut self, dt: f64) -> Fix {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 5.0)
        } else {
            0.0
        };
        // Midpoint integration: smooth speed profile, negligible error.
        let v0 = self.line_speed_at(self.s);
        let s_mid = self.s + v0 * dt * 0.5;
        let ds = self.line_speed_at(s_mid) * dt;
        // Arc length of the emitted curve (not the chord, which would cut
        // corners and depend on the tick size).
        self.path += if self.offset_is_zero() {
            ds
        } else {
            ds * self.stretch_at(self.s + ds * 0.5)
        };
        self.s += ds;
        self.fix()
    }

    /// The fix at the current position.
    pub fn fix(&self) -> Fix {
        let s = self.s;
        let (x, y) = self.xy_at(s);
        let lap_len = self.track.lap_length_m();
        Fix {
            position: self.track.to_latlng(x, y),
            distance_m: self.path,
            laps: self.lap_index(s),
            lap_progress: ((s - self.s0).rem_euclid(lap_len) / lap_len).clamp(0.0, 1.0),
            speed_mps: self.speed_at(s),
        }
    }
}

fn splitmix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Deterministic uniform value in [-1, 1] for (seed, index).
fn unit(seed: u64, index: u64) -> f64 {
    let h = splitmix64(seed ^ splitmix64(index));
    (h >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
}

/// Smooth 1-D value noise in [-1, 1]: random control points at integer
/// positions, joined with a C1-continuous smoothstep.
fn value_noise(seed: u64, x: f64) -> f64 {
    let i = x.floor();
    let t = x - i;
    let s = t * t * (3.0 - 2.0 * t);
    let i = i as i64 as u64;
    let a = unit(seed, i);
    let b = unit(seed, i.wrapping_add(1));
    a + (b - a) * s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::{haversine_m, LocalFrame};
    use crate::route::LoopMode;

    fn track() -> Track {
        let f = LocalFrame::new(LatLng::new(39.96, 116.36));
        let pts: Vec<_> = [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)]
            .iter()
            .map(|&(x, y)| f.to_latlng(x, y))
            .collect();
        Track::new(&pts, LoopMode::Closed)
    }

    #[test]
    fn pace_is_exact_without_variation_regardless_of_tick_size() {
        for dt in [0.05, 0.25, 0.7, 1.3] {
            let mut m = Motion::new(track(), 4.0, Variation::NONE, 1);
            let mut t = 0.0;
            while t < 600.0 {
                m.advance(dt);
                t += dt;
            }
            let avg = m.distance_m() / t;
            assert!((avg - 4.0).abs() < 1e-6, "dt={dt} avg={avg}");
        }
    }

    #[test]
    fn pace_with_variation_stays_near_target_on_average() {
        let mut m = Motion::new(track(), 3.5, Variation::default(), 42);
        let secs = 3600.0;
        let mut t = 0.0;
        while t < secs {
            m.advance(0.25);
            t += 0.25;
        }
        let avg = m.distance_m() / secs;
        assert!((avg / 3.5 - 1.0).abs() < 0.03, "avg={avg}");
    }

    #[test]
    fn variation_is_bounded_and_continuous() {
        let v = Variation::default();
        let mut m = Motion::new(track(), 4.5, v, 7);
        let mut prev = m.fix();
        let mut max_step = 0.0f64;
        for _ in 0..20_000 {
            let f = m.advance(0.25);
            let step = haversine_m(prev.position, f.position);
            max_step = max_step.max(step);
            let lo = 4.5
                * (1.0 - v.run_pace_jitter)
                * (1.0 - v.lap_pace_jitter)
                * (1.0 - v.speed_wobble);
            let hi = (4.5
                * (1.0 + v.run_pace_jitter)
                * (1.0 + v.lap_pace_jitter)
                * (1.0 + v.speed_wobble))
                .min(MAX_SPEED_MPS);
            assert!(f.speed_mps >= lo - 1e-9 && f.speed_mps <= hi + 1e-9);
            prev = f;
        }
        // ≤ 5.4 m/s * 0.25 s ≈ 1.35 m plus small lateral drift; never a jump.
        assert!(max_step < 2.0, "max step {max_step}");
        assert!(m.fix().laps > 10);
    }

    #[test]
    fn set_speed_is_immediate_and_clamped() {
        let mut m = Motion::new(track(), 3.0, Variation::NONE, 0);
        m.advance(10.0_f64.min(5.0));
        m.set_speed(5.0);
        let d0 = m.distance_m();
        m.advance(1.0);
        assert!((m.distance_m() - d0 - 5.0).abs() < 1e-9);
        m.set_speed(100.0);
        assert_eq!(m.base_speed(), MAX_SPEED_MPS);
        m.set_speed(f64::NAN);
        assert_eq!(m.base_speed(), MIN_SPEED_MPS);
    }

    /// Sideways movement only (no speed variation): the emitted trace must
    /// still move at exactly the requested speed.
    const PATH_ONLY: Variation = Variation {
        run_pace_jitter: 0.0,
        lap_pace_jitter: 0.0,
        speed_wobble: 0.0,
        lane_m: 2.5,
        lateral_jitter_m: 3.5,
        fine_jitter_m: 1.0,
        start_jitter_m: 10.0,
    };

    #[test]
    fn emitted_trace_speed_is_exact_with_lateral_variation() {
        let mut m = Motion::new(track(), 3.2, PATH_ONLY, 11);
        let mut prev = m.fix().position;
        let (mut measured, mut t) = (0.0, 0.0);
        while t < 1800.0 {
            let f = m.advance(0.25);
            measured += haversine_m(prev, f.position);
            prev = f.position;
            t += 0.25;
        }
        assert!(
            (m.distance_m() / t / 3.2 - 1.0).abs() < 1e-3,
            "{}",
            m.distance_m() / t
        );
        assert!((measured / m.distance_m() - 1.0).abs() < 2e-3);
    }

    #[test]
    fn different_seeds_give_different_traces() {
        let run = |seed| {
            let mut m = Motion::new(track(), 3.2, Variation::default(), seed);
            (0..2400)
                .map(|_| m.advance(0.25).position)
                .collect::<Vec<_>>()
        };
        let (a, b) = (run(1), run(2));
        let mean_gap = a
            .iter()
            .zip(&b)
            .map(|(p, q)| haversine_m(*p, *q))
            .sum::<f64>()
            / a.len() as f64;
        assert!(
            mean_gap > 1.0,
            "traces too similar: mean gap {mean_gap:.2} m"
        );
        assert_eq!(run(5), run(5));
    }

    #[test]
    fn start_offset_is_small_and_not_counted() {
        for seed in 0..50 {
            let m = Motion::new(track(), 3.2, Variation::default(), seed);
            let f = m.fix();
            assert_eq!(f.distance_m, 0.0);
            assert_eq!(f.laps, 0);
            let start = track().at(0.0);
            let p = track().frame().to_xy(f.position);
            let d = ((p.0 - start.x).powi(2) + (p.1 - start.y).powi(2)).sqrt();
            assert!(d < 8.0 + 4.2, "seed {seed}: start {d:.1} m away");
        }
    }

    #[test]
    fn pace_never_faster_than_cap_even_with_high_variation() {
        let high = Variation {
            run_pace_jitter: 0.1,
            lap_pace_jitter: 0.2,
            speed_wobble: 0.2,
            ..Variation::default()
        };
        for seed in 0..20 {
            // 3'15"/km target, the fastest preset.
            let mut m = Motion::new(track(), 1000.0 / 195.0, high, seed);
            for _ in 0..4000 {
                let f = m.advance(0.25);
                assert!(f.speed_mps <= MAX_SPEED_MPS + 1e-12);
                assert!(1000.0 / f.speed_mps >= 185.0 - 1e-6);
            }
        }
    }

    #[test]
    fn value_noise_range() {
        for k in 0..10_000 {
            let v = value_noise(3, k as f64 * 0.137);
            assert!((-1.0..=1.0).contains(&v));
        }
    }
}

/// End-to-end pace accuracy as a campus-run app would see it: the app gets
/// GPS fixes about once per second and sums the distance between them.
#[cfg(test)]
mod measured_pace {
    use super::*;
    use crate::geo::haversine_m;
    use crate::route::{normalize, parse_points, LoopMode, BUILTIN_ROUTES};

    /// Simulate `secs` of running with irregular ticks (USB latency), sample
    /// at 1 Hz like CoreLocation, return the pace an app would compute.
    fn app_measured_speed(route: &str, v: f64, var: Variation, secs: f64, seed: u64) -> f64 {
        let b = BUILTIN_ROUTES
            .iter()
            .find(|r| r.id.ends_with(route))
            .unwrap();
        let pts = normalize(&parse_points(b.content).unwrap().points, b.datum).unwrap();
        let mut m = Motion::new(Track::new(&pts, LoopMode::Auto), v, var, seed);
        let mut rng = seed | 1;
        let (mut t, mut next_sample) = (0.0, 1.0);
        let mut last = m.fix().position;
        let mut measured = 0.0;
        while t < secs {
            // 250 ms tick + 0–300 ms of random extra latency.
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            let dt = 0.25 + (rng % 300) as f64 / 1000.0;
            let fix = m.advance(dt);
            t += dt;
            if t >= next_sample {
                measured += haversine_m(last, fix.position);
                last = fix.position;
                next_sample += 1.0;
            }
        }
        measured / t
    }

    #[test]
    #[ignore = "report helper: cargo test -p cyber-runner-core pace_report -- --ignored --nocapture"]
    fn pace_report() {
        for route in ["xitucheng", "shahe"] {
            for v in [2.6, 3.2, 4.0] {
                let a = app_measured_speed(route, v, Variation::NONE, 1800.0, 9);
                let b: f64 = (1..=5)
                    .map(|s| app_measured_speed(route, v, Variation::default(), 1800.0, s))
                    .sum::<f64>()
                    / 5.0;
                let nospeed = Variation {
                    run_pace_jitter: 0.0,
                    lap_pace_jitter: 0.0,
                    speed_wobble: 0.0,
                    ..Variation::default()
                };
                let c: f64 = (1..=5)
                    .map(|s| app_measured_speed(route, v, nospeed, 1800.0, s))
                    .sum::<f64>()
                    / 5.0;
                let norun = Variation {
                    run_pace_jitter: 0.0,
                    ..Variation::default()
                };
                let d: f64 = (1..=20)
                    .map(|s| app_measured_speed(route, v, norun, 1800.0, s))
                    .sum::<f64>()
                    / 20.0;
                eprintln!(
                    "{route} {v} m/s: 匀速 {:+.2}%  拟真(中) {:+.2}%  仅轨迹 {:+.2}%  无整次 {:+.2}%",
                    (a / v - 1.0) * 100.0,
                    (b / v - 1.0) * 100.0,
                    (c / v - 1.0) * 100.0,
                    (d / v - 1.0) * 100.0
                );
            }
        }
    }

    #[test]
    fn exact_without_variation_on_real_routes() {
        for route in ["xitucheng", "shahe"] {
            for v in [2.6, 3.2, 4.0] {
                let got = app_measured_speed(route, v, Variation::NONE, 1800.0, 9);
                let err = got / v - 1.0;
                // Only chord-vs-arc shortening at 1 Hz sampling remains.
                assert!(
                    err.abs() < 0.005,
                    "{route} v={v}: measured {got:.4} ({:+.2}%)",
                    err * 100.0
                );
            }
        }
    }

    #[test]
    fn close_with_default_variation_on_real_routes() {
        for route in ["xitucheng", "shahe"] {
            for seed in 1..=5 {
                let got = app_measured_speed(route, 3.2, Variation::default(), 1800.0, seed);
                let err = got / 3.2 - 1.0;
                assert!(
                    err.abs() < 0.03,
                    "{route} seed={seed}: {got:.4} ({:+.2}%)",
                    err * 100.0
                );
            }
        }
    }
}
