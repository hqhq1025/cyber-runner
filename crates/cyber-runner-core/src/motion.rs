//! Time-based motion along a [`Track`].
//!
//! The old Python engine pre-sliced each segment into `int(len / (v*dt))`
//! steps and slept `dt` per step. Truncation made short segments run up to
//! ~2× too fast, and network latency added on top of the sleep made the
//! effective pace drift. Here position is a pure function of *distance
//! travelled*, and distance is integrated from the real elapsed time, so the
//! pace is exact no matter how often (or how irregularly) we tick.
//!
//! Human-like variation (all smooth and deterministic for a given seed):
//! - per-lap pace factor (± `lap_pace_jitter`)
//! - slow in-lap speed wobble (± `speed_wobble`)
//! - lateral drift across the track (≤ `lateral_jitter_m`), continuous
//!   across lap boundaries so there is never a jump.

use serde::{Deserialize, Serialize};

use crate::geo::LatLng;
use crate::track::Track;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Variation {
    /// Fractional per-lap pace change, e.g. 0.04 = ±4 %.
    pub lap_pace_jitter: f64,
    /// Fractional smooth in-lap speed wobble.
    pub speed_wobble: f64,
    /// Max sideways drift from the drawn line, meters.
    pub lateral_jitter_m: f64,
}

impl Default for Variation {
    fn default() -> Self {
        Self {
            lap_pace_jitter: 0.04,
            speed_wobble: 0.03,
            lateral_jitter_m: 2.0,
        }
    }
}

impl Variation {
    pub const NONE: Variation = Variation {
        lap_pace_jitter: 0.0,
        speed_wobble: 0.0,
        lateral_jitter_m: 0.0,
    };

    fn clamped(self) -> Self {
        Self {
            lap_pace_jitter: self.lap_pace_jitter.clamp(0.0, 0.2),
            speed_wobble: self.speed_wobble.clamp(0.0, 0.2),
            lateral_jitter_m: self.lateral_jitter_m.clamp(0.0, 10.0),
        }
    }
}

/// Speed limits accepted by the engine (m/s).
pub const MIN_SPEED_MPS: f64 = 0.5;
pub const MAX_SPEED_MPS: f64 = 10.0;

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
    /// Total distance since start, meters.
    pub distance_m: f64,
    /// Completed laps.
    pub laps: u32,
    /// Progress in the current lap, 0..1.
    pub lap_progress: f64,
    /// Instantaneous speed, m/s.
    pub speed_mps: f64,
}

#[derive(Debug, Clone)]
pub struct Motion {
    track: Track,
    base_speed: f64,
    variation: Variation,
    seed: u64,
    distance: f64,
}

/// Spacing of the noise control points along the run.
const LATERAL_NOISE_SPACING_M: f64 = 45.0;
const SPEED_NOISE_SPACING_M: f64 = 160.0;
/// Half-length of the chord used to orient lateral offsets.
const NORMAL_CHORD_M: f64 = 4.0;

impl Motion {
    pub fn new(track: Track, speed_mps: f64, variation: Variation, seed: u64) -> Self {
        Self {
            track,
            base_speed: clamp_speed(speed_mps),
            variation: variation.clamped(),
            seed,
            distance: 0.0,
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

    pub fn set_variation(&mut self, v: Variation) {
        self.variation = v.clamped();
    }

    pub fn distance_m(&self) -> f64 {
        self.distance
    }

    fn lap_index(&self, distance: f64) -> u32 {
        (distance / self.track.lap_length_m()).floor().max(0.0) as u32
    }

    /// Speed at a given distance (before integrating).
    pub fn speed_at(&self, distance: f64) -> f64 {
        let lap = self.lap_index(distance) as u64;
        let lap_factor = 1.0 + self.variation.lap_pace_jitter * unit(self.seed ^ 0x1A9, lap);
        let wobble = 1.0
            + self.variation.speed_wobble
                * value_noise(self.seed ^ 0x5EED, distance / SPEED_NOISE_SPACING_M);
        clamp_speed(self.base_speed * lap_factor * wobble)
    }

    /// Advance by `dt` seconds of wall time and return the new fix.
    pub fn advance(&mut self, dt: f64) -> Fix {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 5.0)
        } else {
            0.0
        };
        // Midpoint integration: smooth speed profile, negligible error.
        let v0 = self.speed_at(self.distance);
        let v_mid = self.speed_at(self.distance + v0 * dt * 0.5);
        self.distance += v_mid * dt;
        self.fix()
    }

    /// The fix at the current distance.
    pub fn fix(&self) -> Fix {
        let d = self.distance;
        let p = self.track.at(d);
        let off = self.variation.lateral_jitter_m
            * value_noise(self.seed ^ 0x1A7E, d / LATERAL_NOISE_SPACING_M);
        // Offset along the normal of a short chord rather than of the
        // current segment: the segment normal flips instantly at a corner,
        // which would make the position jump sideways.
        let (a, b) = (
            self.track.at(d - NORMAL_CHORD_M),
            self.track.at(d + NORMAL_CHORD_M),
        );
        let (cx, cy) = (b.x - a.x, b.y - a.y);
        let n = (cx * cx + cy * cy).sqrt();
        let (tx, ty) = if n > 1e-6 {
            (cx / n, cy / n)
        } else {
            (p.tx, p.ty)
        };
        let (x, y) = (p.x - ty * off, p.y + tx * off);
        let lap_len = self.track.lap_length_m();
        Fix {
            position: self.track.to_latlng(x, y),
            distance_m: d,
            laps: self.lap_index(d),
            lap_progress: (d.rem_euclid(lap_len) / lap_len).clamp(0.0, 1.0),
            speed_mps: self.speed_at(d),
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
        let mut m = Motion::new(track(), 5.0, v, 7);
        let mut prev = m.fix();
        let mut max_step = 0.0f64;
        for _ in 0..20_000 {
            let f = m.advance(0.25);
            let step = haversine_m(prev.position, f.position);
            max_step = max_step.max(step);
            let lo = 5.0 * (1.0 - v.lap_pace_jitter) * (1.0 - v.speed_wobble);
            let hi = 5.0 * (1.0 + v.lap_pace_jitter) * (1.0 + v.speed_wobble);
            assert!(f.speed_mps >= lo - 1e-9 && f.speed_mps <= hi + 1e-9);
            prev = f;
        }
        // 5 m/s * 1.07 * 0.25 s ≈ 1.34 m plus small lateral drift; never a jump.
        assert!(max_step < 2.0, "max step {max_step}");
        assert!(m.fix().laps > 10);
    }

    #[test]
    fn set_speed_is_immediate_and_clamped() {
        let mut m = Motion::new(track(), 3.0, Variation::NONE, 0);
        m.advance(10.0_f64.min(5.0));
        m.set_speed(6.0);
        let d0 = m.distance_m();
        m.advance(1.0);
        assert!((m.distance_m() - d0 - 6.0).abs() < 1e-9);
        m.set_speed(100.0);
        assert_eq!(m.base_speed(), MAX_SPEED_MPS);
        m.set_speed(f64::NAN);
        assert_eq!(m.base_speed(), MIN_SPEED_MPS);
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
                eprintln!(
                    "{route} {v} m/s: 匀速 {:+.2}%  拟真(中) {:+.2}%",
                    (a / v - 1.0) * 100.0,
                    (b / v - 1.0) * 100.0
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
