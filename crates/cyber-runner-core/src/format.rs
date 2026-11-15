//! Small text helpers shared by the CLI and the app.

/// `m/s` → `M'SS"` per km.
pub fn pace_text(speed_mps: f64) -> String {
    if !(speed_mps.is_finite() && speed_mps > 0.0) {
        return "--'--\"".into();
    }
    let total = (1000.0 / speed_mps).round() as u64;
    format!("{}'{:02}\"", total / 60, total % 60)
}

/// Seconds → `MM:SS` or `H:MM:SS`.
pub fn duration_text(seconds: f64) -> String {
    let s = seconds.max(0.0).floor() as u64;
    let (h, m, s) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pace() {
        assert_eq!(pace_text(4.0), "4'10\"");
        assert_eq!(pace_text(3.0), "5'33\"");
        assert_eq!(pace_text(0.0), "--'--\"");
    }

    #[test]
    fn duration() {
        assert_eq!(duration_text(65.9), "01:05");
        assert_eq!(duration_text(3725.0), "1:02:05");
        assert_eq!(duration_text(-3.0), "00:00");
    }
}
