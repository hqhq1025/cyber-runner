//! 路由文件解析。等价于 Python `util/route.py::parse_route`。
//!
//! 输入是逗号分隔的 `{"lat":"...","lng":"..."}` 序列（注意数值带引号），
//! 输出 `Vec<Point>`。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Point {
    pub lat: f64,
    pub lng: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum RouteError {
    #[error("empty route file")]
    Empty,
    #[error("invalid route value: {0}")]
    InvalidValue(String),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// 兼容字符串或数值的浮点字段。Python 的 `ast.literal_eval` + `float(...)`
/// 同时接受两种，路由文件实际是字符串形式。
#[derive(Deserialize)]
#[serde(untagged)]
enum NumberOrString {
    Number(f64),
    String(String),
}

impl NumberOrString {
    fn into_f64(self) -> Result<f64, RouteError> {
        match self {
            Self::Number(v) => Ok(v),
            Self::String(s) => s
                .trim()
                .parse::<f64>()
                .map_err(|_| RouteError::InvalidValue(s)),
        }
    }
}

#[derive(Deserialize)]
struct RawPoint {
    lat: NumberOrString,
    lng: NumberOrString,
}

pub fn parse_route(content: &str) -> Result<Vec<Point>, RouteError> {
    let trimmed = content.trim().trim_end_matches(',').trim();
    if trimmed.is_empty() {
        return Err(RouteError::Empty);
    }
    let wrapped = format!("[{}]", trimmed);
    let raw: Vec<RawPoint> = serde_json::from_str(&wrapped)?;
    let points: Result<Vec<Point>, RouteError> = raw
        .into_iter()
        .map(|p| {
            Ok(Point {
                lat: p.lat.into_f64()?,
                lng: p.lng.into_f64()?,
            })
        })
        .collect();
    let points = points?;
    if points.is_empty() {
        return Err(RouteError::Empty);
    }
    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_string_quoted_format() {
        let input = r#"{"lng":"116.366","lat":"39.967"},{"lng":"116.367","lat":"39.968"}"#;
        let pts = parse_route(input).unwrap();
        assert_eq!(pts.len(), 2);
        assert!((pts[0].lat - 39.967).abs() < 1e-9);
        assert!((pts[1].lng - 116.367).abs() < 1e-9);
    }

    #[test]
    fn parses_unquoted_numeric_format() {
        let input = r#"{"lng":116.366,"lat":39.967},{"lng":116.367,"lat":39.968}"#;
        let pts = parse_route(input).unwrap();
        assert_eq!(pts.len(), 2);
        assert!((pts[0].lng - 116.366).abs() < 1e-9);
    }

    #[test]
    fn tolerates_trailing_comma() {
        let input = r#"{"lng":"1.0","lat":"2.0"},{"lng":"3.0","lat":"4.0"},"#;
        let pts = parse_route(input).unwrap();
        assert_eq!(pts.len(), 2);
    }

    #[test]
    fn tolerates_surrounding_whitespace() {
        let input = "  \n{\"lng\":\"1.0\",\"lat\":\"2.0\"}\n  ";
        let pts = parse_route(input).unwrap();
        assert_eq!(pts.len(), 1);
    }

    #[test]
    fn rejects_empty_input() {
        assert!(matches!(parse_route(""), Err(RouteError::Empty)));
        assert!(matches!(parse_route("   "), Err(RouteError::Empty)));
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(parse_route("not even close to json").is_err());
    }

    #[test]
    fn rejects_missing_keys() {
        let result = parse_route(r#"{"x":"1.0"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn rejects_invalid_number_string() {
        let result = parse_route(r#"{"lng":"abc","lat":"39.9"}"#);
        assert!(matches!(result, Err(RouteError::InvalidValue(_))));
    }
}
