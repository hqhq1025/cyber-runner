//! Route library: built-in routes + user imports stored as JSON files.

use std::path::{Path, PathBuf};

use cyber_runner_core::{
    geo::haversine_m, normalize, parse_points, Datum, LatLng, LoopMode, Track, BUILTIN_ROUTES,
};
use serde::{Deserialize, Serialize};

/// Max points sent to the UI for drawing.
const PREVIEW_POINTS: usize = 600;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredRoute {
    id: String,
    name: String,
    /// Datum of the imported file, kept for display.
    source_datum: Datum,
    /// WGS-84.
    points: Vec<LatLng>,
    created_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteView {
    pub id: String,
    pub name: String,
    pub builtin: bool,
    pub source_datum: Datum,
    /// One lap when run as a closed loop / out-and-back, after cleanup
    /// (spike removal + smoothing) — i.e. what the phone will really cover.
    pub lap_closed_m: f64,
    pub lap_out_and_back_m: f64,
    /// What `LoopMode::Auto` resolves to.
    pub auto_closed: bool,
    pub point_count: usize,
    /// Gap between first and last point.
    pub end_gap_m: f64,
    /// Cleaned line, `[lat, lng]` pairs, WGS-84, decimated.
    pub preview: Vec<[f64; 2]>,
}

/// Result of reading a file the user picked, before saving it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteDraft {
    pub text: String,
    pub suggested_name: String,
    /// Datum fixed by the file format (GPX/KML/GeoJSON), if any.
    pub detected_datum: Option<Datum>,
}

pub struct RouteStore {
    dir: PathBuf,
}

fn view(id: &str, name: &str, builtin: bool, datum: Datum, pts: &[LatLng]) -> RouteView {
    let auto = Track::new(pts, LoopMode::Auto);
    let auto_closed = auto.mode() == LoopMode::Closed;
    let (closed, oab) = if auto_closed {
        (
            auto.lap_length_m(),
            Track::new(pts, LoopMode::OutAndBack).lap_length_m(),
        )
    } else {
        (
            Track::new(pts, LoopMode::Closed).lap_length_m(),
            auto.lap_length_m(),
        )
    };
    let shape = auto.shape();
    let step = shape.len().div_ceil(PREVIEW_POINTS).max(1);
    let mut preview: Vec<[f64; 2]> = shape.iter().step_by(step).map(|p| [p.lat, p.lng]).collect();
    if let Some(last) = shape.last() {
        if preview.last() != Some(&[last.lat, last.lng]) {
            preview.push([last.lat, last.lng]);
        }
    }
    RouteView {
        id: id.into(),
        name: name.into(),
        builtin,
        source_datum: datum,
        lap_closed_m: closed,
        lap_out_and_back_m: oab,
        auto_closed,
        point_count: pts.len(),
        end_gap_m: match (pts.first(), pts.last()) {
            (Some(a), Some(b)) => haversine_m(*a, *b),
            _ => 0.0,
        },
        preview,
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl RouteStore {
    pub fn new(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        Self { dir }
    }

    fn user_routes(&self) -> Vec<StoredRoute> {
        let mut out: Vec<StoredRoute> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter_map(|e| std::fs::read(e.path()).ok())
            .filter_map(|b| serde_json::from_slice(&b).ok())
            .collect();
        out.sort_by_key(|r| r.created_ms);
        out
    }

    pub fn list(&self) -> Vec<RouteView> {
        let mut out: Vec<RouteView> = BUILTIN_ROUTES
            .iter()
            .filter_map(|b| {
                let p = parse_points(b.content).ok()?;
                let pts = normalize(&p.points, b.datum).ok()?;
                Some(view(b.id, b.name, true, b.datum, &pts))
            })
            .collect();
        out.extend(
            self.user_routes()
                .iter()
                .map(|r| view(&r.id, &r.name, false, r.source_datum, &r.points)),
        );
        out
    }

    /// WGS-84 points + display name for a route id.
    pub fn points(&self, id: &str) -> Option<(String, Vec<LatLng>)> {
        if let Some(b) = BUILTIN_ROUTES.iter().find(|b| b.id == id) {
            let p = parse_points(b.content).ok()?;
            return Some((b.name.into(), normalize(&p.points, b.datum).ok()?));
        }
        self.user_routes()
            .into_iter()
            .find(|r| r.id == id)
            .map(|r| (r.name, r.points))
    }

    pub fn read_file(path: &Path) -> Result<RouteDraft, String> {
        let meta = std::fs::metadata(path).map_err(|e| format!("无法读取文件：{e}"))?;
        if meta.len() > 20 * 1024 * 1024 {
            return Err("文件太大（超过 20 MB），不像是一条路线".into());
        }
        let bytes = std::fs::read(path).map_err(|e| format!("无法读取文件：{e}"))?;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let parsed = parse_points(&text).map_err(|e| e.to_string())?;
        Ok(RouteDraft {
            text,
            suggested_name: path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "导入的路线".into()),
            detected_datum: parsed.datum,
        })
    }

    pub fn preview(text: &str, datum: Datum) -> Result<RouteView, String> {
        let parsed = parse_points(text).map_err(|e| e.to_string())?;
        let datum = parsed.datum.unwrap_or(datum);
        let pts = normalize(&parsed.points, datum).map_err(|e| e.to_string())?;
        Ok(view("preview", "预览", false, datum, &pts))
    }

    pub fn create(&self, name: &str, text: &str, datum: Datum) -> Result<RouteView, String> {
        let parsed = parse_points(text).map_err(|e| e.to_string())?;
        let datum = parsed.datum.unwrap_or(datum);
        let pts = normalize(&parsed.points, datum).map_err(|e| e.to_string())?;
        let name = name.trim();
        let name = if name.is_empty() {
            "导入的路线"
        } else {
            name
        };
        let r = StoredRoute {
            id: format!("user:{}", uuid::Uuid::new_v4().simple()),
            name: name.chars().take(40).collect(),
            source_datum: datum,
            points: pts,
            created_ms: now_ms(),
        };
        self.write(&r)?;
        Ok(view(&r.id, &r.name, false, r.source_datum, &r.points))
    }

    fn file_for(&self, id: &str) -> Option<PathBuf> {
        let stem = id.strip_prefix("user:")?;
        if !stem.chars().all(|c| c.is_ascii_alphanumeric()) {
            return None;
        }
        Some(self.dir.join(format!("{stem}.json")))
    }

    fn write(&self, r: &StoredRoute) -> Result<(), String> {
        let path = self.file_for(&r.id).ok_or("无效的路线 id")?;
        let tmp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec(r).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
    }

    pub fn rename(&self, id: &str, name: &str) -> Result<(), String> {
        let mut r = self
            .user_routes()
            .into_iter()
            .find(|r| r.id == id)
            .ok_or("内置路线不能改名")?;
        let name = name.trim();
        if name.is_empty() {
            return Err("名字不能为空".into());
        }
        r.name = name.chars().take(40).collect();
        self.write(&r)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let path = self.file_for(id).ok_or("内置路线不能删除")?;
        std::fs::remove_file(path).map_err(|e| e.to_string())
    }
}

/// Default route for new installs.
pub const DEFAULT_ROUTE_ID: &str = "builtin:bupt-xitucheng";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_listed_and_user_routes_round_trip() {
        let dir = std::env::temp_dir().join(format!("cr-routes-{}", uuid::Uuid::new_v4()));
        let store = RouteStore::new(dir.clone());
        let list = store.list();
        assert!(list.iter().any(|r| r.id == DEFAULT_ROUTE_ID));
        assert!(list.iter().all(|r| r.preview.len() <= PREVIEW_POINTS + 1));

        let text = "116.3600,39.9600\n116.3610,39.9600\n116.3610,39.9610\n";
        let v = store.create("测试", text, Datum::Bd09).unwrap();
        assert!(v.lap_closed_m > 150.0);
        assert!(v.lap_out_and_back_m > v.lap_closed_m);
        assert!(store.points(&v.id).is_some());
        store.rename(&v.id, "新名字").unwrap();
        assert!(store.list().iter().any(|r| r.name == "新名字"));
        store.delete(&v.id).unwrap();
        assert!(store.points(&v.id).is_none());
        assert!(store.delete(DEFAULT_ROUTE_ID).is_err());
        assert!(store.file_for("user:../../etc").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}
