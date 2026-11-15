//! Geodesy helpers: Chinese map datums (BD-09 / GCJ-02) → WGS-84, distances,
//! and a local planar projection used for interpolation and lateral offsets.
//!
//! iOS location simulation expects WGS-84. Routes picked on Baidu Maps are
//! BD-09, routes picked on Amap / Tencent Maps are GCJ-02, GPX/KML from a
//! watch are already WGS-84.

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

const X_PI: f64 = PI * 3000.0 / 180.0;
const KRASOVSKY_A: f64 = 6_378_245.0;
const KRASOVSKY_EE: f64 = 0.006_693_421_622_965_943;
/// Mean earth radius (IUGG), meters.
pub const EARTH_RADIUS_M: f64 = 6_371_008.8;

/// A WGS-84 (or source-datum) coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LatLng {
    pub lat: f64,
    pub lng: f64,
}

impl LatLng {
    pub const fn new(lat: f64, lng: f64) -> Self {
        Self { lat, lng }
    }

    pub fn is_valid(&self) -> bool {
        self.lat.is_finite()
            && self.lng.is_finite()
            && (-90.0..=90.0).contains(&self.lat)
            && (-180.0..=180.0).contains(&self.lng)
    }
}

/// Which datum a route's coordinates are expressed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Datum {
    /// GPS / iOS native.
    Wgs84,
    /// "Mars coordinates": Amap (高德), Tencent Maps, Apple Maps inside China.
    Gcj02,
    /// Baidu Maps. The legacy route files are BD-09.
    #[default]
    Bd09,
}

impl Datum {
    pub fn to_wgs84(self, p: LatLng) -> LatLng {
        match self {
            Datum::Wgs84 => p,
            Datum::Gcj02 => gcj02_to_wgs84(p),
            Datum::Bd09 => gcj02_to_wgs84(bd09_to_gcj02(p)),
        }
    }
}

/// GCJ-02 is only applied inside mainland China; outside it equals WGS-84.
pub fn out_of_china(p: LatLng) -> bool {
    !(73.66..=135.05).contains(&p.lng) || !(3.86..=53.55).contains(&p.lat)
}

pub fn bd09_to_gcj02(p: LatLng) -> LatLng {
    let x = p.lng - 0.0065;
    let y = p.lat - 0.006;
    let z = (x * x + y * y).sqrt() - 0.00002 * (y * X_PI).sin();
    let theta = y.atan2(x) - 0.000_003 * (x * X_PI).cos();
    LatLng::new(z * theta.sin(), z * theta.cos())
}

/// Forward WGS-84 → GCJ-02 (the official obfuscation).
pub fn wgs84_to_gcj02(p: LatLng) -> LatLng {
    if out_of_china(p) {
        return p;
    }
    let (d_lat, d_lng) = gcj_delta(p);
    LatLng::new(p.lat + d_lat, p.lng + d_lng)
}

/// Inverse GCJ-02 → WGS-84. There is no closed form; a few fixed-point
/// iterations converge to < 1 cm (the single-step approximation the old
/// Python code used is off by up to ~1-2 m).
pub fn gcj02_to_wgs84(g: LatLng) -> LatLng {
    if out_of_china(g) {
        return g;
    }
    let mut w = g;
    for _ in 0..8 {
        let f = wgs84_to_gcj02(w);
        let (e_lat, e_lng) = (f.lat - g.lat, f.lng - g.lng);
        w.lat -= e_lat;
        w.lng -= e_lng;
        if e_lat.abs() < 1e-10 && e_lng.abs() < 1e-10 {
            break;
        }
    }
    w
}

fn gcj_delta(p: LatLng) -> (f64, f64) {
    let x = p.lng - 105.0;
    let y = p.lat - 35.0;
    let mut d_lat = -100.0 + 2.0 * x + 3.0 * y + 0.2 * y * y + 0.1 * x * y + 0.2 * x.abs().sqrt();
    d_lat += (20.0 * (6.0 * x * PI).sin() + 20.0 * (2.0 * x * PI).sin()) * 2.0 / 3.0;
    d_lat += (20.0 * (y * PI).sin() + 40.0 * (y / 3.0 * PI).sin()) * 2.0 / 3.0;
    d_lat += (160.0 * (y / 12.0 * PI).sin() + 320.0 * (y * PI / 30.0).sin()) * 2.0 / 3.0;
    let mut d_lng = 300.0 + x + 2.0 * y + 0.1 * x * x + 0.1 * x * y + 0.1 * x.abs().sqrt();
    d_lng += (20.0 * (6.0 * x * PI).sin() + 20.0 * (2.0 * x * PI).sin()) * 2.0 / 3.0;
    d_lng += (20.0 * (x * PI).sin() + 40.0 * (x / 3.0 * PI).sin()) * 2.0 / 3.0;
    d_lng += (150.0 * (x / 12.0 * PI).sin() + 300.0 * (x / 30.0 * PI).sin()) * 2.0 / 3.0;

    let rad_lat = p.lat / 180.0 * PI;
    let magic = 1.0 - KRASOVSKY_EE * rad_lat.sin().powi(2);
    let sqrt_magic = magic.sqrt();
    d_lat = (d_lat * 180.0) / ((KRASOVSKY_A * (1.0 - KRASOVSKY_EE)) / (magic * sqrt_magic) * PI);
    d_lng = (d_lng * 180.0) / (KRASOVSKY_A / sqrt_magic * rad_lat.cos() * PI);
    (d_lat, d_lng)
}

/// Great-circle distance in meters.
pub fn haversine_m(a: LatLng, b: LatLng) -> f64 {
    let phi1 = a.lat.to_radians();
    let phi2 = b.lat.to_radians();
    let dphi = (b.lat - a.lat).to_radians();
    let dlam = (b.lng - a.lng).to_radians();
    let h = (dphi / 2.0).sin().powi(2) + phi1.cos() * phi2.cos() * (dlam / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * h.sqrt().atan2((1.0 - h).sqrt())
}

/// Equirectangular projection around a fixed origin. Campus routes span a
/// few km at most, where the error is well below a centimeter per meter —
/// plenty for interpolation and for applying lateral offsets in meters.
#[derive(Debug, Clone, Copy)]
pub struct LocalFrame {
    origin: LatLng,
    m_per_deg_lat: f64,
    m_per_deg_lng: f64,
}

impl LocalFrame {
    pub fn new(origin: LatLng) -> Self {
        let m_per_deg_lat = EARTH_RADIUS_M * PI / 180.0;
        Self {
            origin,
            m_per_deg_lat,
            m_per_deg_lng: m_per_deg_lat * origin.lat.to_radians().cos(),
        }
    }

    /// (east, north) in meters.
    pub fn to_xy(&self, p: LatLng) -> (f64, f64) {
        (
            (p.lng - self.origin.lng) * self.m_per_deg_lng,
            (p.lat - self.origin.lat) * self.m_per_deg_lat,
        )
    }

    pub fn to_latlng(&self, x: f64, y: f64) -> LatLng {
        LatLng::new(
            self.origin.lat + y / self.m_per_deg_lat,
            self.origin.lng + x / self.m_per_deg_lng,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The single-pass formula the old Python `bd09Towgs84` used.
    fn legacy_bd09_to_wgs84(p: LatLng) -> LatLng {
        let g = bd09_to_gcj02(p);
        let (d_lat, d_lng) = gcj_delta(g);
        LatLng::new(g.lat - d_lat, g.lng - d_lng)
    }

    #[test]
    fn legacy_golden_value_still_reproduced() {
        // Golden value from the original Python implementation (Beijing).
        let w = legacy_bd09_to_wgs84(LatLng::new(39.90, 116.40));
        assert!((w.lng - 116.387_366_482_093_98).abs() < 1e-9);
        assert!((w.lat - 39.892_267_192_340_015).abs() < 1e-9);
    }

    #[test]
    fn iterative_inverse_is_exact_and_close_to_legacy() {
        for p in [
            LatLng::new(39.967, 116.366), // BUPT 西土城
            LatLng::new(40.157, 116.287), // BUPT 沙河
            LatLng::new(31.30, 120.68),
            LatLng::new(23.13, 113.27),
        ] {
            let w = Datum::Bd09.to_wgs84(p);
            // Round trip back through the forward transform must be exact.
            let g = wgs84_to_gcj02(w);
            let g0 = bd09_to_gcj02(p);
            assert!(haversine_m(g, g0) < 0.01, "round trip error too big");
            // And within a few meters of the legacy approximation.
            assert!(haversine_m(w, legacy_bd09_to_wgs84(p)) < 5.0);
        }
    }

    #[test]
    fn outside_china_gcj_is_identity() {
        let sf = LatLng::new(37.77, -122.42);
        assert_eq!(gcj02_to_wgs84(sf), sf);
        assert_eq!(Datum::Wgs84.to_wgs84(sf), sf);
    }

    #[test]
    fn haversine_one_km() {
        let d = haversine_m(LatLng::new(39.0, 116.0), LatLng::new(39.009, 116.0));
        assert!((d - 1000.7).abs() < 1.0, "{d}");
    }

    #[test]
    fn local_frame_round_trip() {
        let f = LocalFrame::new(LatLng::new(39.96, 116.36));
        let p = LatLng::new(39.965, 116.37);
        let (x, y) = f.to_xy(p);
        let q = f.to_latlng(x, y);
        assert!(haversine_m(p, q) < 1e-6);
        assert!(((x * x + y * y).sqrt() - haversine_m(LatLng::new(39.96, 116.36), p)).abs() < 1.0);
    }
}
