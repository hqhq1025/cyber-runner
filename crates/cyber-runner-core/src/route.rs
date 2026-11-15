//! Route files → a clean list of WGS-84 points.
//!
//! Accepted formats (auto-detected):
//! - legacy Cyber Runner / iOSRealRun `{"lng":"..","lat":".."},...` (optionally wrapped in `[...]`)
//! - JSON arrays of `{lat,lng}` / `{latitude,longitude}` / `[lng,lat]`
//! - GeoJSON (`LineString`, `MultiLineString`, `Polygon`, `Feature`, `FeatureCollection`)
//! - GPX (`trkpt` / `rtept` / `wpt`)
//! - KML (`<coordinates>`)
//! - plain text, one `lng,lat` (or `lat,lng`, auto-detected) pair per line

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::geo::{haversine_m, Datum, LatLng};

/// Consecutive points closer than this are merged (the bundled BUPT routes
/// contain many exact duplicates, which used to produce zero-length steps).
const MIN_POINT_SPACING_M: f64 = 0.3;
/// Routes shorter than this are almost certainly a mistake.
pub const MIN_ROUTE_LENGTH_M: f64 = 20.0;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum RouteError {
    #[error("路线文件是空的")]
    Empty,
    #[error("无法识别的路线格式：{0}")]
    UnknownFormat(String),
    #[error("路线解析失败：{0}")]
    Invalid(String),
    #[error("有效点太少（需要至少 2 个不重合的点）")]
    TooFewPoints,
    #[error("路线太短（{0:.0} 米），至少需要 {MIN_ROUTE_LENGTH_M:.0} 米")]
    TooShort(f64),
}

/// Raw points as they appear in the file, plus the datum if the format
/// defines one (GPX / KML / GeoJSON are WGS-84 by spec).
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedPoints {
    pub points: Vec<LatLng>,
    pub datum: Option<Datum>,
}

/// How a lap is formed from the drawn polyline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum LoopMode {
    /// Pick automatically: closed if start and end are close, else out-and-back.
    #[default]
    Auto,
    /// Run the polyline and return straight from the last point to the first.
    Closed,
    /// Run the polyline, then run it backwards.
    OutAndBack,
}

/// Distance between start and end under which `Auto` treats a route as a loop.
pub const AUTO_CLOSE_GAP_M: f64 = 60.0;

impl LoopMode {
    pub fn resolve(self, points: &[LatLng]) -> LoopMode {
        match self {
            LoopMode::Auto => match (points.first(), points.last()) {
                (Some(a), Some(b)) if haversine_m(*a, *b) <= AUTO_CLOSE_GAP_M => LoopMode::Closed,
                _ => LoopMode::OutAndBack,
            },
            m => m,
        }
    }
}

pub fn parse_points(content: &str) -> Result<ParsedPoints, RouteError> {
    let text = content.trim_start_matches('\u{feff}').trim();
    if text.is_empty() {
        return Err(RouteError::Empty);
    }
    if text.starts_with('<') {
        return parse_xml(text);
    }
    if text.starts_with('{') || text.starts_with('[') {
        return parse_json(text);
    }
    parse_plain(text)
}

fn parse_json(text: &str) -> Result<ParsedPoints, RouteError> {
    // Legacy format is a comma-separated list of objects without brackets.
    let value: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => {
            let wrapped = format!("[{}]", text.trim_end_matches(',').trim_end());
            serde_json::from_str(&wrapped).map_err(|e| RouteError::Invalid(e.to_string()))?
        }
    };
    if value.get("type").is_some() {
        let mut out = Vec::new();
        collect_geojson(&value, &mut out)?;
        return Ok(ParsedPoints {
            points: out,
            datum: Some(Datum::Wgs84),
        });
    }
    let items = match &value {
        Value::Array(items) => items.as_slice(),
        Value::Object(_) => std::slice::from_ref(&value),
        _ => return Err(RouteError::UnknownFormat("JSON 不是数组或对象".into())),
    };
    let points = items
        .iter()
        .map(json_point)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ParsedPoints {
        points,
        datum: None,
    })
}

fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn json_point(v: &Value) -> Result<LatLng, RouteError> {
    let bad = || RouteError::Invalid(format!("无法解析的点：{v}"));
    match v {
        Value::Object(o) => {
            let get = |keys: &[&str]| keys.iter().find_map(|k| o.get(*k)).and_then(num);
            let lat = get(&["lat", "latitude", "Lat", "Latitude"]).ok_or_else(bad)?;
            let lng = get(&["lng", "lon", "long", "longitude", "Lng", "Lon", "Longitude"])
                .ok_or_else(bad)?;
            Ok(LatLng::new(lat, lng))
        }
        // GeoJSON order: [lng, lat]
        Value::Array(a) if a.len() >= 2 => Ok(LatLng::new(
            num(&a[1]).ok_or_else(bad)?,
            num(&a[0]).ok_or_else(bad)?,
        )),
        _ => Err(bad()),
    }
}

fn collect_geojson(v: &Value, out: &mut Vec<LatLng>) -> Result<(), RouteError> {
    let ty = v.get("type").and_then(Value::as_str).unwrap_or_default();
    let coords = || v.get("coordinates").and_then(Value::as_array);
    match ty {
        "FeatureCollection" => {
            for f in v
                .get("features")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                collect_geojson(f, out)?;
            }
        }
        "Feature" => {
            if let Some(g) = v.get("geometry") {
                collect_geojson(g, out)?;
            }
        }
        "GeometryCollection" => {
            for g in v
                .get("geometries")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                collect_geojson(g, out)?;
            }
        }
        "LineString" | "MultiPoint" => {
            for c in coords().into_iter().flatten() {
                out.push(json_point(c)?);
            }
        }
        "MultiLineString" | "Polygon" => {
            // Use the first ring / line only; multiple lines are rare for a run.
            if let Some(line) = coords().and_then(|c| c.first()).and_then(Value::as_array) {
                for c in line {
                    out.push(json_point(c)?);
                }
            }
        }
        "Point" => {
            if let Some(c) = v.get("coordinates") {
                out.push(json_point(c)?);
            }
        }
        other => return Err(RouteError::UnknownFormat(format!("GeoJSON 类型 {other}"))),
    }
    Ok(())
}

fn xml_attr(tag: &str, name: &str) -> Option<f64> {
    let mut rest = tag;
    while let Some(i) = rest.find(name) {
        let before_ok = i == 0 || rest[..i].ends_with(|c: char| c.is_whitespace());
        let after = rest[i + name.len()..].trim_start();
        if before_ok {
            if let Some(after) = after.strip_prefix('=') {
                let after = after.trim_start();
                let quote = after.chars().next()?;
                if quote == '"' || quote == '\'' {
                    let body = &after[1..];
                    let end = body.find(quote)?;
                    return body[..end].trim().parse().ok();
                }
            }
        }
        rest = &rest[i + name.len()..];
    }
    None
}

fn parse_xml(text: &str) -> Result<ParsedPoints, RouteError> {
    let mut points = Vec::new();
    // GPX: prefer track points, then route points, then waypoints.
    for tag in ["trkpt", "rtept", "wpt"] {
        let open = format!("<{tag}");
        let mut rest = text;
        while let Some(i) = rest.find(&open) {
            let after = &rest[i + open.len()..];
            let end = after.find('>').unwrap_or(after.len());
            let attrs = &after[..end];
            if let (Some(lat), Some(lng)) = (xml_attr(attrs, "lat"), xml_attr(attrs, "lon")) {
                points.push(LatLng::new(lat, lng));
            }
            rest = &after[end..];
        }
        if !points.is_empty() {
            return Ok(ParsedPoints {
                points,
                datum: Some(Datum::Wgs84),
            });
        }
    }
    // KML: <coordinates>lng,lat[,alt] lng,lat[,alt] ...</coordinates>
    let mut rest = text;
    while let Some(i) = rest.find("<coordinates>") {
        let body = &rest[i + "<coordinates>".len()..];
        let end = body.find("</coordinates>").unwrap_or(body.len());
        for tuple in body[..end].split_whitespace() {
            let mut it = tuple.split(',').map(|s| s.trim().parse::<f64>());
            if let (Some(Ok(lng)), Some(Ok(lat))) = (it.next(), it.next()) {
                points.push(LatLng::new(lat, lng));
            }
        }
        rest = &body[end..];
        // Keep only the first line string, like GeoJSON.
        if points.len() >= 2 {
            break;
        }
    }
    if points.is_empty() {
        return Err(RouteError::UnknownFormat(
            "XML 里没有 GPX 点或 KML 坐标".into(),
        ));
    }
    Ok(ParsedPoints {
        points,
        datum: Some(Datum::Wgs84),
    })
}

fn parse_plain(text: &str) -> Result<ParsedPoints, RouteError> {
    let mut pairs = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let nums: Vec<f64> = line
            .split(|c: char| c == ',' || c == ';' || c == '\t' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        match nums.as_slice() {
            [a, b, ..] => pairs.push((*a, *b)),
            // Header lines like "lng,lat" are skipped silently.
            _ if pairs.is_empty() => continue,
            _ => return Err(RouteError::Invalid(format!("无法解析的行：{line}"))),
        }
    }
    if pairs.is_empty() {
        return Err(RouteError::UnknownFormat(
            "既不是 JSON / GPX / KML，也不是经纬度文本".into(),
        ));
    }
    // Default is `lng,lat` (what Baidu's coordinate picker emits). Switch to
    // `lat,lng` only when the data can't be `lng,lat`.
    let lat_first =
        pairs.iter().all(|(a, _)| a.abs() <= 90.0) && pairs.iter().any(|(_, b)| b.abs() > 90.0);
    let points = pairs
        .into_iter()
        .map(|(a, b)| {
            if lat_first {
                LatLng::new(a, b)
            } else {
                LatLng::new(b, a)
            }
        })
        .collect();
    Ok(ParsedPoints {
        points,
        datum: None,
    })
}

/// Convert to WGS-84, drop invalid / duplicate points and validate length.
pub fn normalize(points: &[LatLng], datum: Datum) -> Result<Vec<LatLng>, RouteError> {
    let mut out: Vec<LatLng> = Vec::with_capacity(points.len());
    for p in points.iter().filter(|p| p.is_valid()) {
        let w = datum.to_wgs84(*p);
        if out
            .last()
            .is_none_or(|last| haversine_m(*last, w) >= MIN_POINT_SPACING_M)
        {
            out.push(w);
        }
    }
    if out.len() < 2 {
        return Err(RouteError::TooFewPoints);
    }
    let len = polyline_length_m(&out);
    if len < MIN_ROUTE_LENGTH_M {
        return Err(RouteError::TooShort(len));
    }
    Ok(out)
}

pub fn polyline_length_m(points: &[LatLng]) -> f64 {
    points.windows(2).map(|w| haversine_m(w[0], w[1])).sum()
}

/// A route that ships with the app.
#[derive(Debug, Clone, Copy)]
pub struct BuiltinRoute {
    pub id: &'static str,
    pub name: &'static str,
    pub datum: Datum,
    pub content: &'static str,
}

pub const BUILTIN_ROUTES: &[BuiltinRoute] = &[
    BuiltinRoute {
        id: "builtin:bupt-xitucheng",
        name: "北邮 · 西土城操场",
        datum: Datum::Bd09,
        content: include_str!("../routes/bupt-xitucheng.txt"),
    },
    BuiltinRoute {
        id: "builtin:bupt-shahe",
        name: "北邮 · 沙河操场",
        datum: Datum::Bd09,
        content: include_str!("../routes/bupt-shahe.txt"),
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_format_with_strings_and_trailing_comma() {
        let p = parse_points(r#"{"lng":"116.366","lat":"39.967"},{"lng":116.367,"lat":"39.968"},"#)
            .unwrap();
        assert_eq!(p.points.len(), 2);
        assert_eq!(p.datum, None);
        assert!((p.points[1].lng - 116.367).abs() < 1e-12);
    }

    #[test]
    fn json_array_variants() {
        let p = parse_points(r#"[{"latitude":1,"longitude":2},[4,3]]"#).unwrap();
        assert_eq!(p.points, vec![LatLng::new(1.0, 2.0), LatLng::new(3.0, 4.0)]);
    }

    #[test]
    fn geojson_feature_collection() {
        let g = r#"{"type":"FeatureCollection","features":[{"type":"Feature","geometry":{"type":"LineString","coordinates":[[116.1,39.1],[116.2,39.2]]}}]}"#;
        let p = parse_points(g).unwrap();
        assert_eq!(p.datum, Some(Datum::Wgs84));
        assert_eq!(p.points[1], LatLng::new(39.2, 116.2));
    }

    #[test]
    fn gpx_track() {
        let g = r#"<?xml version="1.0"?><gpx><trk><trkseg>
            <trkpt lat="39.1" lon="116.1"><ele>50</ele></trkpt>
            <trkpt lon='116.2' lat='39.2'/></trkseg></trk></gpx>"#;
        let p = parse_points(g).unwrap();
        assert_eq!(
            p.points,
            vec![LatLng::new(39.1, 116.1), LatLng::new(39.2, 116.2)]
        );
        assert_eq!(p.datum, Some(Datum::Wgs84));
    }

    #[test]
    fn kml_coordinates() {
        let k = "<kml><Placemark><LineString><coordinates>\n116.1,39.1,0 116.2,39.2,0\n</coordinates></LineString></Placemark></kml>";
        let p = parse_points(k).unwrap();
        assert_eq!(
            p.points,
            vec![LatLng::new(39.1, 116.1), LatLng::new(39.2, 116.2)]
        );
    }

    #[test]
    fn plain_text_detects_order() {
        let a = parse_points("lng,lat\n116.1,39.1\n116.2 39.2\n").unwrap();
        assert_eq!(a.points[0], LatLng::new(39.1, 116.1));
        let b = parse_points("39.1,116.1\n39.2,116.2").unwrap();
        assert_eq!(b.points[1], LatLng::new(39.2, 116.2));
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse_points("  "), Err(RouteError::Empty));
        assert!(parse_points("hello world").is_err());
        assert!(parse_points("<html></html>").is_err());
    }

    #[test]
    fn normalize_dedupes_and_validates() {
        let pts = [
            LatLng::new(39.0, 116.0),
            LatLng::new(39.0, 116.0),
            LatLng::new(f64::NAN, 1.0),
            LatLng::new(39.001, 116.0),
        ];
        let out = normalize(&pts, Datum::Wgs84).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(
            normalize(&pts[..2], Datum::Wgs84),
            Err(RouteError::TooFewPoints)
        );
        assert!(matches!(
            normalize(
                &[LatLng::new(39.0, 116.0), LatLng::new(39.00005, 116.0)],
                Datum::Wgs84
            ),
            Err(RouteError::TooShort(_))
        ));
    }

    #[test]
    fn builtin_routes_all_parse() {
        for r in BUILTIN_ROUTES {
            let p = parse_points(r.content).unwrap();
            let pts = normalize(&p.points, r.datum).unwrap();
            let len = polyline_length_m(&pts);
            assert!(len > 200.0, "{} too short: {len}", r.id);
        }
    }

    #[test]
    fn loop_mode_auto() {
        let closed = [
            LatLng::new(39.0, 116.0),
            LatLng::new(39.001, 116.0),
            LatLng::new(39.0001, 116.0),
        ];
        assert_eq!(LoopMode::Auto.resolve(&closed), LoopMode::Closed);
        let open = [LatLng::new(39.0, 116.0), LatLng::new(39.01, 116.0)];
        assert_eq!(LoopMode::Auto.resolve(&open), LoopMode::OutAndBack);
        assert_eq!(LoopMode::Closed.resolve(&open), LoopMode::Closed);
    }
}
