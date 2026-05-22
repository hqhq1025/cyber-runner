//! 时间/配速文本格式化，跨平台等价于 Swift `RunTimeFormatter`。

use chrono::{DateTime, Utc};

/// 把整数秒格式化为 `HH:MM:SS`（≥1h）或 `MM:SS`（<1h）。负数当作 0。
pub fn duration_text(seconds: i64) -> String {
    let s = seconds.max(0);
    let h = s / 3600;
    let m = (s % 3600) / 60;
    let sec = s % 60;
    if h > 0 {
        format!("{:02}:{:02}:{:02}", h, m, sec)
    } else {
        format!("{:02}:{:02}", m, sec)
    }
}

/// 距 `until` 还有多少秒，最少返回 0（不会负数）。
pub fn remaining_seconds(until: DateTime<Utc>, now: DateTime<Utc>) -> i64 {
    (until - now).num_seconds().max(0)
}

/// 配速文本：把 m/s 转成 `MM:SS /km`；速度 ≤0 返回 `--:-- /km`。
pub fn pace_text(speed_mps: f64) -> String {
    if speed_mps <= 0.0 || !speed_mps.is_finite() {
        return "--:-- /km".into();
    }
    let secs_per_km = (1000.0 / speed_mps).round() as i64;
    let m = secs_per_km / 60;
    let s = secs_per_km % 60;
    format!("{:02}:{:02} /km", m, s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn duration_under_hour_uses_mmss() {
        assert_eq!(duration_text(65), "01:05");
    }

    #[test]
    fn duration_over_hour_uses_hhmmss() {
        assert_eq!(duration_text(3_723), "01:02:03");
    }

    #[test]
    fn duration_zero() {
        assert_eq!(duration_text(0), "00:00");
    }

    #[test]
    fn duration_negative_treated_as_zero() {
        assert_eq!(duration_text(-10), "00:00");
    }

    #[test]
    fn duration_exactly_one_hour() {
        assert_eq!(duration_text(3_600), "01:00:00");
    }

    #[test]
    fn remaining_seconds_clamps_at_zero() {
        let now = Utc::now();
        let past = now - Duration::seconds(10);
        assert_eq!(remaining_seconds(past, now), 0);
    }

    #[test]
    fn remaining_seconds_future_is_positive() {
        let now = Utc::now();
        let future = now + Duration::seconds(300);
        assert_eq!(remaining_seconds(future, now), 300);
    }

    #[test]
    fn pace_4_7_mps_is_3_33_per_km() {
        assert_eq!(pace_text(4.7), "03:33 /km");
    }

    #[test]
    fn pace_zero_speed_returns_placeholder() {
        assert_eq!(pace_text(0.0), "--:-- /km");
    }

    #[test]
    fn pace_negative_speed_returns_placeholder() {
        assert_eq!(pace_text(-1.0), "--:-- /km");
    }

    #[test]
    fn pace_nan_returns_placeholder() {
        assert_eq!(pace_text(f64::NAN), "--:-- /km");
    }

    #[test]
    fn pace_infinity_returns_placeholder() {
        assert_eq!(pace_text(f64::INFINITY), "--:-- /km");
    }
}
