//! User preferences, persisted as JSON in the app data dir.

use std::path::PathBuf;

use cyber_runner_core::{LoopMode, Variation};
use cyber_runner_engine::{FinishAction, StopAfter};
use serde::{Deserialize, Serialize};

use crate::routes::DEFAULT_ROUTE_ID;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Realism {
    Off,
    Low,
    #[default]
    Normal,
    High,
}

impl Realism {
    pub fn variation(self) -> Variation {
        match self {
            Realism::Off => Variation::NONE,
            Realism::Low => Variation::LOW,
            Realism::Normal => Variation::default(),
            Realism::High => Variation::HIGH,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum GoalKind {
    None,
    Time,
    #[default]
    Distance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub route_id: String,
    pub speed_mps: f64,
    pub realism: Realism,
    pub loop_mode: LoopMode,
    pub goal: GoalKind,
    pub goal_minutes: f64,
    pub goal_km: f64,
    /// Extra distance on top of `goal_km`, in case the campus app counts a
    /// little short. The run stops at `goal_km + goal_margin_km`.
    pub goal_margin_km: f64,
    pub on_finish: FinishAction,
    /// Preferred device; `None` = first USB device.
    pub udid: Option<String>,
    /// Folder with a Developer Disk Image, for offline setups.
    pub ddi_dir: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            route_id: DEFAULT_ROUTE_ID.into(),
            // "5 分配" preset (5'10"/km).
            speed_mps: 1000.0 / 310.0,
            realism: Realism::Normal,
            loop_mode: LoopMode::Auto,
            goal: GoalKind::Distance,
            goal_minutes: 20.0,
            goal_km: 5.0,
            goal_margin_km: 1.0,
            on_finish: FinishAction::Hold,
            udid: None,
            ddi_dir: None,
        }
    }
}

impl Settings {
    pub fn stop_after(&self) -> StopAfter {
        match self.goal {
            GoalKind::None => StopAfter::default(),
            GoalKind::Time => StopAfter {
                duration_s: Some(self.goal_minutes.clamp(1.0, 600.0) * 60.0),
                distance_m: None,
            },
            GoalKind::Distance => StopAfter {
                duration_s: None,
                distance_m: Some(
                    (self.goal_km.clamp(0.1, 100.0) + self.goal_margin_km.clamp(0.0, 5.0)) * 1000.0,
                ),
            },
        }
    }

    pub fn sanitized(mut self) -> Self {
        self.speed_mps = cyber_runner_core::clamp_speed(self.speed_mps);
        if !self.goal_minutes.is_finite() {
            self.goal_minutes = 20.0;
        }
        if !self.goal_km.is_finite() {
            self.goal_km = 5.0;
        }
        if !self.goal_margin_km.is_finite() {
            self.goal_margin_km = 1.0;
        }
        self.goal_margin_km = self.goal_margin_km.clamp(0.0, 5.0);
        self.goal_minutes = self.goal_minutes.clamp(1.0, 600.0);
        self.goal_km = self.goal_km.clamp(0.1, 100.0);
        self
    }
}

pub struct SettingsFile {
    path: PathBuf,
}

impl SettingsFile {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> Settings {
        std::fs::read(&self.path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
            .unwrap_or_default()
            .sanitized()
    }

    pub fn save(&self, s: &Settings) -> std::io::Result<()> {
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(s)?)?;
        std::fs::rename(tmp, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_and_garbage_files_fall_back_to_defaults() {
        let s: Settings = serde_json::from_str(r#"{"speedMps": 99}"#).unwrap();
        assert_eq!(s.sanitized().speed_mps, cyber_runner_core::MAX_SPEED_MPS);
        let dir = std::env::temp_dir().join(format!("cr-set-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = SettingsFile::new(dir.join("settings.json"));
        std::fs::write(dir.join("settings.json"), "not json").unwrap();
        assert_eq!(f.load(), Settings::default());
        let s = Settings {
            goal: GoalKind::Time,
            ..Default::default()
        };
        f.save(&s).unwrap();
        assert_eq!(f.load(), s);
        assert_eq!(s.stop_after().duration_s, Some(1200.0));
        let _ = std::fs::remove_dir_all(dir);
    }
}
