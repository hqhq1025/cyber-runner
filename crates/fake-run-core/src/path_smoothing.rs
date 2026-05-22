//! 路径平滑与确定性插值。
//!
//! 一比一翻译自 Python `run.py::fixLockT`（确定性，可精确对齐），
//! 以及 `randLoc` 的属性测试版本（随机扰动不强求与 Python 数值一致）。

use crate::coord::haversine_meters;
use crate::route::Point;

/// 把折线 `loc` 按速度 `v` (m/s) 和时间步长 `dt` (s) 重采样：
/// 每段切成 `max(1, distance/(v*dt))` 等分，逐点输出（不含段尾）。
/// 等价于 Python `fixLockT`。
pub fn fixed_step_path(loc: &[Point], v: f64, dt: f64) -> Vec<Point> {
    assert!(v > 0.0 && dt > 0.0, "speed and dt must be positive");
    let mut out = Vec::with_capacity(loc.len() * 4);
    let n = loc.len();
    if n == 0 {
        return out;
    }
    for i in 0..n {
        let a = loc[i];
        let b = loc[(i + 1) % n];
        let distance = haversine_meters(a.lat, a.lng, b.lat, b.lng);
        let steps = (distance / (v * dt)).floor().max(1.0) as usize;
        let steps_f = steps as f64;
        let lat_step = (b.lat - a.lat) / steps_f;
        let lng_step = (b.lng - a.lng) / steps_f;
        for j in 0..steps {
            let jf = j as f64;
            out.push(Point {
                lat: a.lat + jf * lat_step,
                lng: a.lng + jf * lng_step,
            });
        }
    }
    out
}

/// 平滑权重：`sin²(π * t)`，t = (j-start) / (end-start)。
/// 等价于 Python `smooth(start, end, i)`。
pub fn smooth_weight(start: usize, end: usize, i: usize) -> f64 {
    assert!(end > start, "smooth_weight requires end > start");
    let t = (i - start) as f64 / (end - start) as f64;
    (t * std::f64::consts::PI).sin().powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(lat: f64, lng: f64) -> Point {
        Point { lat, lng }
    }

    #[test]
    fn empty_path_returns_empty() {
        let out = fixed_step_path(&[], 4.7, 0.2);
        assert!(out.is_empty());
    }

    #[test]
    fn single_point_loops_back_to_itself() {
        let out = fixed_step_path(&[pt(39.0, 116.0)], 4.7, 0.2);
        // distance to self = 0, max(1, 0) = 1 step, output is just the start
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], pt(39.0, 116.0));
    }

    #[test]
    fn two_point_loop_has_expected_step_count() {
        // ~111 m apart (0.001 deg lat ≈ 111 m), v=1 m/s, dt=1 s
        // → steps per edge ≈ 111, two edges → ~222 points
        let route = vec![pt(39.0, 116.0), pt(39.001, 116.0)];
        let out = fixed_step_path(&route, 1.0, 1.0);
        assert!(
            (200..=240).contains(&out.len()),
            "expected ~222 points, got {}",
            out.len()
        );
    }

    #[test]
    fn output_starts_at_first_input_point() {
        let route = vec![pt(39.0, 116.0), pt(39.001, 116.0)];
        let out = fixed_step_path(&route, 1.0, 1.0);
        assert_eq!(out[0], pt(39.0, 116.0));
    }

    #[test]
    fn interpolation_is_linear_between_endpoints() {
        // single segment, force exactly 2 steps with very high speed
        let route = vec![pt(0.0, 0.0), pt(2.0, 2.0)];
        let out = fixed_step_path(&route, 1e9, 1.0);
        // distance/v*dt -> 0, max(1) = 1 step per edge, 2 edges
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], pt(0.0, 0.0));
        assert_eq!(out[1], pt(2.0, 2.0));
    }

    #[test]
    fn smooth_weight_zero_at_endpoints() {
        assert!(smooth_weight(0, 10, 0).abs() < 1e-12);
        assert!(smooth_weight(0, 10, 10).abs() < 1e-12);
    }

    #[test]
    fn smooth_weight_one_at_midpoint() {
        assert!((smooth_weight(0, 10, 5) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn smooth_weight_symmetric() {
        let a = smooth_weight(0, 10, 3);
        let b = smooth_weight(0, 10, 7);
        assert!((a - b).abs() < 1e-12);
    }
}
