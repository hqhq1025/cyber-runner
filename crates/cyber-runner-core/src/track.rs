//! A lap as a planar polyline with cumulative distances, so any distance
//! along the run maps to a position in O(log n).

use crate::geo::{LatLng, LocalFrame};
use crate::route::LoopMode;

/// Resampling step and smoothing width. Hand-picked routes zig-zag (the
/// bundled 西土城 route turns ~3600° per lap instead of ~360°). Walking the
/// raw zig-zag is longer than what a phone sampling GPS at 1 Hz measures, so
/// the campus app saw a shorter distance and a slower pace than configured.
/// Smoothing turns it into the line a runner would actually follow.
const RESAMPLE_M: f64 = 1.0;
const SMOOTH_SIGMA_M: f64 = 3.0;

/// Turns sharper than this are treated as picking mistakes (the line goes
/// out and straight back). The bundled 西土城 route has 9 of them per lap,
/// each adding ~10–20 m of back-and-forth and a visible reversal.
const SPIKE_TURN_DEG: f64 = 150.0;

fn turn_deg(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    let h1 = (b.1 - a.1).atan2(b.0 - a.0);
    let h2 = (c.1 - b.1).atan2(c.0 - b.0);
    let mut d = (h2 - h1).abs() % (2.0 * std::f64::consts::PI);
    if d > std::f64::consts::PI {
        d = 2.0 * std::f64::consts::PI - d;
    }
    d.to_degrees()
}

/// Iteratively drop vertices where the path doubles back on itself.
pub(crate) fn remove_spikes(pts: &[(f64, f64)], closed: bool) -> Vec<(f64, f64)> {
    let mut v: Vec<(f64, f64)> = pts.to_vec();
    loop {
        let n = v.len();
        let min = if closed { 4 } else { 3 };
        if n < min {
            return v;
        }
        let range: Box<dyn Iterator<Item = usize>> = if closed {
            Box::new(0..n)
        } else {
            Box::new(1..n - 1)
        };
        let mut worst: Option<(usize, f64)> = None;
        for i in range {
            let a = v[(i + n - 1) % n];
            let c = v[(i + 1) % n];
            let t = turn_deg(a, v[i], c);
            if t > SPIKE_TURN_DEG && worst.is_none_or(|(_, w)| t > w) {
                worst = Some((i, t));
            }
        }
        match worst {
            Some((i, _)) => {
                v.remove(i);
            }
            None => return v,
        }
    }
}

fn resample(pts: &[(f64, f64)], closed: bool, step: f64) -> Vec<(f64, f64)> {
    let mut poly: Vec<(f64, f64)> = pts.to_vec();
    if closed {
        poly.push(pts[0]);
    }
    let mut out = vec![poly[0]];
    let mut carry = 0.0; // distance since last emitted point
    for w in poly.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        if len == 0.0 {
            continue;
        }
        let mut d = step - carry;
        while d <= len {
            let t = d / len;
            out.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
            d += step;
        }
        carry = len - (d - step);
    }
    if closed {
        // The wrap-around point duplicates the start.
        if out.len() > 2 {
            let last = *out.last().unwrap();
            if ((last.0 - out[0].0).powi(2) + (last.1 - out[0].1).powi(2)).sqrt() < step * 0.5 {
                out.pop();
            }
        }
    } else if let Some(&end) = poly.last() {
        if out.last() != Some(&end) {
            out.push(end);
        }
    }
    out
}

/// Gaussian smoothing; `sigma` in samples. Closed tracks wrap around, open
/// ones keep their end points fixed.
fn smooth(pts: &[(f64, f64)], closed: bool, sigma: f64) -> Vec<(f64, f64)> {
    let n = pts.len();
    if n < 3 || sigma <= 0.0 {
        return pts.to_vec();
    }
    let r = (sigma * 3.0).ceil() as isize;
    let kernel: Vec<f64> = (-r..=r)
        .map(|k| (-(k as f64).powi(2) / (2.0 * sigma * sigma)).exp())
        .collect();
    (0..n as isize)
        .map(|i| {
            if !closed && (i == 0 || i == n as isize - 1) {
                return pts[i as usize];
            }
            // Open tracks: shrink the window symmetrically near the ends so
            // the line doesn't get pulled towards them.
            let rr = if closed {
                r
            } else {
                r.min(i).min(n as isize - 1 - i)
            };
            let (mut sx, mut sy, mut sw) = (0.0, 0.0, 0.0);
            for k in -rr..=rr {
                let j = (i + k).rem_euclid(n as isize) as usize;
                let w = kernel[(k + r) as usize];
                sx += pts[j].0 * w;
                sy += pts[j].1 * w;
                sw += w;
            }
            (sx / sw, sy / sw)
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct Track {
    frame: LocalFrame,
    xy: Vec<(f64, f64)>,
    /// `cum[i]` = distance from lap start to vertex `i`.
    cum: Vec<f64>,
    mode: LoopMode,
    /// Number of leading vertices that form the drawn shape (one way).
    shape_len: usize,
}

/// A point on the track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackPoint {
    pub x: f64,
    pub y: f64,
    /// Unit tangent (direction of travel).
    pub tx: f64,
    pub ty: f64,
}

impl Track {
    /// `points` must be WGS-84 and already normalized (≥ 2 distinct points).
    pub fn new(points: &[LatLng], mode: LoopMode) -> Self {
        assert!(points.len() >= 2, "track needs at least two points");
        let mode = mode.resolve(points);
        let frame = LocalFrame::new(points[0]);
        let raw: Vec<(f64, f64)> = points.iter().map(|p| frame.to_xy(*p)).collect();
        let closed = mode == LoopMode::Closed;
        let raw = remove_spikes(&raw, closed);
        let mut xy = smooth(
            &resample(&raw, closed, RESAMPLE_M),
            closed,
            SMOOTH_SIGMA_M / RESAMPLE_M,
        );
        match mode {
            LoopMode::Closed => xy.push(xy[0]),
            LoopMode::OutAndBack => {
                let back: Vec<_> = xy.iter().rev().skip(1).copied().collect();
                xy.extend(back);
            }
            LoopMode::Auto => unreachable!("resolved above"),
        }
        let mut cum = Vec::with_capacity(xy.len());
        let mut acc = 0.0;
        cum.push(0.0);
        for w in xy.windows(2) {
            acc += ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt();
            cum.push(acc);
        }
        let shape_len = match mode {
            LoopMode::OutAndBack => xy.len().div_ceil(2),
            _ => xy.len(),
        };
        Self {
            frame,
            xy,
            cum,
            mode,
            shape_len,
        }
    }

    pub fn lap_length_m(&self) -> f64 {
        *self.cum.last().unwrap()
    }

    pub fn mode(&self) -> LoopMode {
        self.mode
    }

    pub fn frame(&self) -> &LocalFrame {
        &self.frame
    }

    /// Position at `distance` meters from the start of the run (wraps per lap).
    pub fn at(&self, distance: f64) -> TrackPoint {
        let lap = self.lap_length_m();
        let d = distance.rem_euclid(lap);
        // First vertex with cum > d; segment is [i-1, i].
        let i = self
            .cum
            .partition_point(|&c| c <= d)
            .clamp(1, self.cum.len() - 1);
        let (a, b) = (self.xy[i - 1], self.xy[i]);
        let seg = self.cum[i] - self.cum[i - 1];
        let t = if seg > 0.0 {
            (d - self.cum[i - 1]) / seg
        } else {
            0.0
        };
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let n = (dx * dx + dy * dy).sqrt().max(1e-9);
        TrackPoint {
            x: a.0 + dx * t,
            y: a.1 + dy * t,
            tx: dx / n,
            ty: dy / n,
        }
    }

    pub fn to_latlng(&self, x: f64, y: f64) -> LatLng {
        self.frame.to_latlng(x, y)
    }

    /// The cleaned, smoothed line that is actually run (one way for
    /// out-and-back; closed loops include the return to the start).
    pub fn shape(&self) -> Vec<LatLng> {
        self.xy[..self.shape_len]
            .iter()
            .map(|&(x, y)| self.frame.to_latlng(x, y))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::haversine_m;

    fn square() -> Vec<LatLng> {
        // ~100 m square
        let f = LocalFrame::new(LatLng::new(39.96, 116.36));
        [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)]
            .iter()
            .map(|&(x, y)| f.to_latlng(x, y))
            .collect()
    }

    #[test]
    fn closed_lap_length_and_wrap() {
        let t = Track::new(&square(), LoopMode::Closed);
        // Corners are rounded by smoothing, so a lap is a bit under 400 m.
        assert!(
            (t.lap_length_m() - 400.0).abs() < 8.0,
            "{}",
            t.lap_length_m()
        );
        let d = t.lap_length_m() * 150.0 / 400.0;
        let p = t.at(d);
        assert!(
            (p.x - 100.0).abs() < 0.5 && (p.y - 50.0).abs() < 2.0,
            "{p:?}"
        );
        let q = t.at(d + t.lap_length_m() * 3.0);
        assert!((p.x - q.x).abs() < 1e-6 && (p.y - q.y).abs() < 1e-6);
        assert!((p.ty - 1.0).abs() < 1e-6, "heading north on the east edge");
    }

    #[test]
    fn spikes_are_removed() {
        // forward 10, back 8 (mistake), forward 10
        let v = [
            (0.0, 0.0),
            (10.0, 0.0),
            (2.0, 0.1),
            (20.0, 0.0),
            (30.0, 0.0),
        ];
        let out = remove_spikes(&v, false);
        assert!(out.iter().all(|p| p.0 >= 0.0));
        let mut prev = out[0].0;
        for p in &out[1..] {
            assert!(p.0 > prev, "path must be monotone after cleanup: {out:?}");
            prev = p.0;
        }
    }

    #[test]
    fn bundled_routes_have_no_reversals_after_cleanup() {
        use crate::route::{normalize, parse_points, BUILTIN_ROUTES};
        for b in BUILTIN_ROUTES {
            let pts = normalize(&parse_points(b.content).unwrap().points, b.datum).unwrap();
            let t = Track::new(&pts, LoopMode::Auto);
            let lap = t.lap_length_m();
            let mut prev = t.at(0.0);
            let mut prev_dir: Option<(f64, f64)> = None;
            let mut d = 0.5;
            while d < lap {
                let p = t.at(d);
                let dir = (p.x - prev.x, p.y - prev.y);
                if let Some(pd) = prev_dir {
                    let cos = (pd.0 * dir.0 + pd.1 * dir.1)
                        / ((pd.0.hypot(pd.1)) * dir.0.hypot(dir.1)).max(1e-12);
                    // Allow the deliberate turnaround of out-and-back laps.
                    let at_turn = t.mode() == LoopMode::OutAndBack && (d - lap / 2.0).abs() < 2.0;
                    assert!(cos > 0.0 || at_turn, "{}: reversal at {d:.1} m", b.id);
                }
                prev_dir = Some(dir);
                prev = p;
                d += 0.5;
            }
        }
    }

    #[test]
    fn smoothing_removes_zigzag_but_keeps_shape() {
        // A 100 m straight line with ±1.5 m zig-zag every 2 m.
        let f = LocalFrame::new(LatLng::new(39.96, 116.36));
        let pts: Vec<_> = (0..=50)
            .map(|i| f.to_latlng(i as f64 * 2.0, if i % 2 == 0 { 1.5 } else { -1.5 }))
            .collect();
        let raw_len: f64 = pts.windows(2).map(|w| haversine_m(w[0], w[1])).sum();
        let t = Track::new(&pts, LoopMode::OutAndBack);
        let one_way = t.lap_length_m() / 2.0;
        assert!(raw_len > 140.0, "raw zig-zag is long: {raw_len}");
        assert!(
            (one_way - 100.0).abs() < 2.0,
            "smoothed ≈ straight 100 m, got {one_way}"
        );
    }

    #[test]
    fn out_and_back_returns_to_start() {
        let pts = &square()[..3];
        let t = Track::new(pts, LoopMode::OutAndBack);
        assert!((t.lap_length_m() - 400.0).abs() < 8.0);
        let end = t.at(t.lap_length_m() - 1e-6);
        assert!(end.x.abs() < 0.01 && end.y.abs() < 0.01);
        // End points are kept exactly; the turnaround is the last drawn point.
        let mid = t.at(t.lap_length_m() / 2.0);
        let far = t.to_latlng(mid.x, mid.y);
        assert!(haversine_m(far, pts[2]) < 0.5);
    }
}
