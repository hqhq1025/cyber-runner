//! `cyber-runner` — the command-line front end. Same engine, same data folder
//! (pairing, disk image cache, imported routes) as the desktop app.

use std::io::{IsTerminal, Write};
use std::process::ExitCode;
use std::time::Duration;

use cyber_runner_core::{
    format, normalize, parse_points, Datum, LatLng, LoopMode, Variation, BUILTIN_ROUTES,
};
use cyber_runner_engine::{Engine, EngineConfig, FinishAction, Level, Phase, RunPlan, StopAfter};

/// Fastest pace allowed, in seconds per km (matches the engine's speed cap).
const FASTEST_PACE_S: f64 = 185.0;
/// Preset paces, a little slower than the round numbers so the wobble stays legal.
const PACE_PRESETS: &[(&str, f64)] = &[("5", 310.0), ("4", 250.0), ("3", 195.0)];

const HELP: &str = "\
Cyber Runner 命令行 — 让 iPhone 沿着路线跑起来

用法:
  cyber-runner run [选项]     开始跑步（Ctrl+C 结束并恢复真实定位）
  cyber-runner devices        列出连接的手机
  cyber-runner routes         列出可用路线（内置 + 在 app 里导入的）
  cyber-runner restore        恢复手机的真实定位

run 选项:
  --route <名字|文件>   路线：xitucheng（默认）、shahe、导入路线的名字，或 GPX/KML/txt 文件
  --datum <坐标系>      文件坐标系：bd09 | gcj02 | wgs84（默认自动识别，txt 按 bd09）
  --pace <配速>         5 | 4 | 3（5'10\" / 4'10\" / 3'15\"），或 5:30 这样的分:秒；默认 5
  --km <公里>           目标距离，默认 5
  --margin <公里>       多跑的余量，防 App 少记，默认 1（即默认实跑 6 km）
  --minutes <分钟>      改为按时间跑（与 --km 二选一）
  --forever             不设目标，一直跑到 Ctrl+C
  --realism <程度>      off | low | normal（默认）| high：每次轨迹和配速起伏都不同
  --restore-at-end      到达目标后立即恢复定位（默认停在终点，等你回车）
  --udid <UDID>         有多台手机时指定一台

例子:
  cyber-runner run                           默认：西土城，5 分配，实跑 6 km
  cyber-runner run --pace 4 --km 3           4 分配，实跑 4 km
  cyber-runner run --route shahe --minutes 30
";

struct Args(Vec<String>);

impl Args {
    fn has(&self, name: &str) -> bool {
        self.0.iter().any(|a| a == name)
    }
    fn flag(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .position(|a| a == name)
            .and_then(|i| self.0.get(i + 1))
            .map(String::as_str)
    }
    fn num(&self, name: &str) -> Result<Option<f64>, String> {
        self.flag(name)
            .map(|v| match v.parse::<f64>() {
                Ok(n) if n.is_finite() && n >= 0.0 => Ok(n),
                _ => Err(format!("{name} 需要一个非负数字，收到 {v}")),
            })
            .transpose()
    }
}

fn data_dir() -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| ".".into())
        .join("CyberRunner")
}

/// Routes imported in the desktop app: `routes/*.json` with `name` + WGS-84 `points`.
fn imported_routes() -> Vec<(String, Vec<LatLng>)> {
    #[derive(serde::Deserialize)]
    struct Stored {
        name: String,
        points: Vec<LatLng>,
    }
    let Ok(dir) = std::fs::read_dir(data_dir().join("routes")) else {
        return Vec::new();
    };
    let mut out: Vec<(String, Vec<LatLng>)> = dir
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| std::fs::read(e.path()).ok())
        .filter_map(|b| serde_json::from_slice::<Stored>(&b).ok())
        .map(|r| (r.name, r.points))
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn load_route(args: &Args) -> Result<(String, Vec<LatLng>), String> {
    let key = args.flag("--route").unwrap_or("xitucheng");
    if let Some(b) = BUILTIN_ROUTES
        .iter()
        .find(|r| r.id.ends_with(key) || r.name == key)
    {
        let p = parse_points(b.content).map_err(|e| e.to_string())?;
        return Ok((
            b.name.into(),
            normalize(&p.points, b.datum).map_err(|e| e.to_string())?,
        ));
    }
    if let Some((name, pts)) = imported_routes().into_iter().find(|(n, _)| n == key) {
        return Ok((name, pts));
    }
    let text = std::fs::read_to_string(key).map_err(|_| {
        format!("找不到路线「{key}」：不是内置路线、导入的路线，也不是文件。运行 cyber-runner routes 查看")
    })?;
    let parsed = parse_points(&text).map_err(|e| e.to_string())?;
    let datum = match args.flag("--datum") {
        Some("wgs84") => Datum::Wgs84,
        Some("gcj02") => Datum::Gcj02,
        Some("bd09") => Datum::Bd09,
        Some(other) => return Err(format!("未知坐标系 {other}（可选 bd09 / gcj02 / wgs84）")),
        None => parsed.datum.unwrap_or(Datum::Bd09),
    };
    let name = std::path::Path::new(key)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| key.into());
    Ok((
        name,
        normalize(&parsed.points, datum).map_err(|e| e.to_string())?,
    ))
}

/// Pace in seconds per km from `5` / `4` / `3`, `5:30`, or `5'30`.
fn parse_pace(v: &str) -> Result<f64, String> {
    if let Some(&(_, s)) = PACE_PRESETS.iter().find(|(k, _)| *k == v) {
        return Ok(s);
    }
    let bad = || format!("看不懂配速 {v}：用 5 / 4 / 3，或 5:30 这样的分:秒");
    let (m, s) = v.split_once([':', '\'']).ok_or_else(bad)?;
    let m: f64 = m.trim().parse().map_err(|_| bad())?;
    let s: f64 = s.trim().trim_end_matches('"').parse().map_err(|_| bad())?;
    if !(0.0..60.0).contains(&s) {
        return Err(bad());
    }
    let pace = m * 60.0 + s;
    if pace < FASTEST_PACE_S {
        return Err(format!(
            "配速 {v} 太快：最快 3'05\"/km（快于 3 分配，校园跑 App 会判无效）"
        ));
    }
    if pace > 1200.0 {
        return Err(format!("配速 {v} 太慢：最慢 20'00\"/km"));
    }
    Ok(pace)
}

fn realism(args: &Args) -> Result<Variation, String> {
    Ok(match args.flag("--realism").unwrap_or("normal") {
        "off" => Variation::NONE,
        "low" => Variation::LOW,
        "normal" => Variation::default(),
        "high" => Variation::HIGH,
        other => {
            return Err(format!(
                "未知拟真程度 {other}（可选 off / low / normal / high）"
            ))
        }
    })
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "off".into()))
        .init();
    let args = Args(std::env::args().skip(1).collect());
    let cmd = args.0.first().cloned().unwrap_or_default();
    if args.has("--help") || args.has("-h") || cmd == "help" {
        print!("{HELP}");
        return ExitCode::SUCCESS;
    }
    let result = match cmd.as_str() {
        "devices" => devices().await,
        "routes" => {
            routes();
            Ok(())
        }
        "restore" => restore(&args).await,
        "run" => run(&args).await,
        "" => {
            print!("{HELP}");
            return ExitCode::from(2);
        }
        other => Err(format!(
            "未知命令 {other}，运行 cyber-runner --help 查看用法"
        )),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("✗ {e}");
            ExitCode::FAILURE
        }
    }
}

async fn devices() -> Result<(), String> {
    let list = cyber_runner_engine::list_devices(&data_dir().join("pairing"))
        .await
        .map_err(|e| format!("{e}\n  {}", e.hint()))?;
    if list.is_empty() {
        println!("没有检测到手机：用数据线连上 iPhone 并解锁");
    }
    for d in list {
        let link = if d.connection == "usb" {
            "USB"
        } else {
            "无线（不能用，请插线）"
        };
        println!(
            "{}  iOS {}  {}  {}\n  {}",
            d.display_name(),
            d.ios_version.as_deref().unwrap_or("?"),
            link,
            if d.trusted { "已信任" } else { "未信任" },
            d.udid,
        );
    }
    Ok(())
}

fn routes() {
    println!("内置:");
    for r in BUILTIN_ROUTES {
        let key = r.id.rsplit(['-', ':', '/']).next().unwrap_or(r.id);
        println!("  {key:<12} {}", r.name);
    }
    let imported = imported_routes();
    if !imported.is_empty() {
        println!("导入的（用 --route \"名字\"）:");
        for (name, pts) in imported {
            println!("  {name}  （{} 个点）", pts.len());
        }
    }
}

async fn restore(args: &Args) -> Result<(), String> {
    engine(false)
        .restore_location(args.flag("--udid").map(String::from))
        .await
        .map_err(|e| format!("{e}\n  {}", e.hint()))?;
    println!("✓ 已恢复真实定位");
    Ok(())
}

fn engine(quiet_info: bool) -> Engine {
    let engine = Engine::spawn(EngineConfig {
        pair_dir: data_dir().join("pairing"),
        ddi_cache_dir: data_dir().join("ddi"),
        ddi_extra_dir: None,
    });
    let mut logs = engine.logs();
    tokio::spawn(async move {
        while let Ok(l) = logs.recv().await {
            let tag = match l.level {
                Level::Info if quiet_info => continue,
                Level::Info => "·",
                Level::Warn => "!",
                Level::Error => "✗",
            };
            Status::clear();
            eprintln!("{tag} {}", l.message);
        }
    });
    engine
}

/// One status line that redraws in place on a terminal, or prints every 30 s otherwise.
struct Status;

impl Status {
    fn tty() -> bool {
        std::io::stderr().is_terminal()
    }
    fn clear() {
        if Self::tty() {
            eprint!("\r\x1b[2K");
        }
    }
    fn show(line: &str) {
        if Self::tty() {
            eprint!("\r\x1b[2K{line}");
            std::io::stderr().flush().ok();
        } else {
            eprintln!("{line}");
        }
    }
}

fn clock_in(secs: f64) -> String {
    let t = std::time::SystemTime::now() + Duration::from_secs_f64(secs.max(0.0));
    let since = t
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    // Local time without a tz crate: use the offset the C library knows.
    let offset = local_offset_s();
    let day = (since + offset).rem_euclid(86_400);
    format!("{:02}:{:02}", day / 3600, day % 3600 / 60)
}

#[cfg(unix)]
fn local_offset_s() -> i64 {
    // `date +%z` is available on every macOS/Linux; fall back to UTC+8 (the
    // campus apps this tool targets are in China).
    std::process::Command::new("date")
        .arg("+%z")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|z| {
            let z = z.trim();
            let sign = if z.starts_with('-') { -1 } else { 1 };
            let h: i64 = z.get(1..3)?.parse().ok()?;
            let m: i64 = z.get(3..5)?.parse().ok()?;
            Some(sign * (h * 3600 + m * 60))
        })
        .unwrap_or(8 * 3600)
}

#[cfg(not(unix))]
fn local_offset_s() -> i64 {
    8 * 3600
}

async fn run(args: &Args) -> Result<(), String> {
    let (name, points) = load_route(args)?;
    let pace_s = match (args.flag("--pace"), args.num("--speed")?) {
        (Some(p), _) => parse_pace(p)?,
        (None, Some(mps)) if mps > 0.0 => (1000.0 / mps).max(FASTEST_PACE_S),
        _ => PACE_PRESETS[0].1,
    };
    let speed = 1000.0 / pace_s;
    let minutes = args.num("--minutes")?;
    let goal_km = args.num("--km")?.unwrap_or(5.0);
    let margin_km = args.num("--margin")?.unwrap_or(1.0).min(5.0);
    if args.flag("--km").is_some() && minutes.is_some() {
        return Err("--km 和 --minutes 只能选一个".into());
    }
    let stop_after = if args.has("--forever") {
        StopAfter::default()
    } else if let Some(m) = minutes {
        StopAfter {
            duration_s: Some(m * 60.0),
            distance_m: None,
        }
    } else {
        StopAfter {
            duration_s: None,
            distance_m: Some((goal_km + margin_km) * 1000.0),
        }
    };
    let restore_at_end = args.has("--restore-at-end");
    let plan = RunPlan {
        udid: args.flag("--udid").map(String::from),
        route_name: name.clone(),
        points,
        loop_mode: LoopMode::Auto,
        speed_mps: speed,
        variation: realism(args)?,
        stop_after,
        on_finish: if restore_at_end {
            FinishAction::Restore
        } else {
            FinishAction::Hold
        },
    };

    let goal_text = match (stop_after.distance_m, stop_after.duration_s) {
        (Some(d), _) if margin_km > 0.0 => {
            format!(
                "实跑 {:.1} km（{goal_km} km + {margin_km} km 余量）",
                d / 1000.0
            )
        }
        (Some(d), _) => format!("{:.1} km", d / 1000.0),
        (_, Some(t)) => format!("跑 {}", format::duration_text(t)),
        _ => "不设目标".into(),
    };
    eprintln!(
        "▶ {name} · 配速 {} · {goal_text}\n  Ctrl+C 随时结束并恢复真实定位",
        format::pace_text(speed)
    );

    let engine = engine(true);
    let mut state = engine.subscribe();
    engine.start(plan).map_err(|e| e.to_string())?;
    let mut ticker = tokio::time::interval(Duration::from_secs(if Status::tty() { 1 } else { 30 }));
    let mut started = false;
    let mut last_step: Option<String> = None;
    let mut finished = false;
    let (enter_tx, mut enter_rx) = tokio::sync::mpsc::channel::<()>(1);

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                Status::clear();
                eprintln!("■ 正在结束并恢复真实定位…");
                engine.stop().await.map_err(|e| e.to_string())?;
                eprintln!("✓ 已恢复真实定位");
                return Ok(());
            }
            Some(()) = enter_rx.recv() => {
                eprintln!("■ 正在恢复真实定位…");
                engine.stop().await.map_err(|e| e.to_string())?;
                eprintln!("✓ 已恢复真实定位");
                return Ok(());
            }
            changed = state.changed() => {
                if changed.is_err() { return Ok(()); }
                let s = state.borrow_and_update().clone();
                if s.phase != Phase::Idle { started = true; }
                if s.step != last_step {
                    if let Some(step) = &s.step {
                        Status::clear();
                        eprintln!("· {step}");
                    }
                    last_step = s.step.clone();
                }
                if s.phase == Phase::Finished && !finished {
                    finished = true;
                    Status::clear();
                    eprintln!(
                        "\x07✓ 跑完了：{:.2} km，用时 {}",
                        s.distance_m / 1000.0,
                        format::duration_text(s.moving_s)
                    );
                    notify(&format!("跑完了 {:.2} km", s.distance_m / 1000.0));
                    eprintln!("  定位停在终点。先在校园跑 App 里点「结束跑步」，再按回车恢复真实定位。");
                    let tx = enter_tx.clone();
                    std::thread::spawn(move || {
                        let mut line = String::new();
                        if std::io::stdin().read_line(&mut line).is_ok() {
                            tx.blocking_send(()).ok();
                        }
                    });
                }
                if started && s.phase == Phase::Idle {
                    Status::clear();
                    return match s.error {
                        Some(e) => Err(format!("{}\n  {}", e.message, e.hint)),
                        None => {
                            if finished || restore_at_end {
                                eprintln!("✓ 已恢复真实定位");
                            }
                            Ok(())
                        }
                    };
                }
            }
            _ = ticker.tick() => {
                let s = engine.snapshot();
                if !matches!(s.phase, Phase::Running | Phase::Paused | Phase::Reconnecting) {
                    continue;
                }
                let speed_for_eta = if s.moving_s > 5.0 { s.distance_m / s.moving_s } else { speed };
                let (left, eta) = match (s.stop_after.distance_m, s.stop_after.duration_s) {
                    (Some(d), _) => {
                        let left = (d - s.distance_m).max(0.0);
                        (format!(" · 还剩 {:.2} km", left / 1000.0), left / speed_for_eta)
                    }
                    (_, Some(t)) => {
                        let left = (t - s.moving_s).max(0.0);
                        (format!(" · 还剩 {}", format::duration_text(left)), left)
                    }
                    _ => (String::new(), 0.0),
                };
                let when = if eta > 0.0 && s.phase == Phase::Running {
                    format!(" · 预计 {} 跑完", clock_in(eta))
                } else if s.phase == Phase::Reconnecting {
                    " · 重连中…".into()
                } else {
                    String::new()
                };
                Status::show(&format!(
                    "已跑 {:.2} km{left} · {} · 配速 {} · {:.1} 圈{when}",
                    s.distance_m / 1000.0,
                    format::duration_text(s.moving_s),
                    format::pace_text(s.speed_mps),
                    s.laps as f64 + s.lap_progress,
                ));
            }
        }
    }
}

/// Best-effort desktop notification for when the terminal is in the background.
fn notify(text: &str) {
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "display notification \"{}\" with title \"Cyber Runner\" sound name \"Glass\"",
            text.replace('"', "")
        );
        std::process::Command::new("osascript")
            .args(["-e", &script])
            .spawn()
            .ok();
    }
    #[cfg(not(target_os = "macos"))]
    let _ = text;
}

#[cfg(test)]
mod tests {
    use super::parse_pace;

    #[test]
    fn pace_presets_and_formats() {
        assert_eq!(parse_pace("5").unwrap(), 310.0);
        assert_eq!(parse_pace("4").unwrap(), 250.0);
        assert_eq!(parse_pace("3").unwrap(), 195.0);
        assert_eq!(parse_pace("5:30").unwrap(), 330.0);
        assert_eq!(parse_pace("4'45").unwrap(), 285.0);
        assert_eq!(parse_pace("4'45\"").unwrap(), 285.0);
    }

    #[test]
    fn pace_rejects_too_fast_and_garbage() {
        assert!(parse_pace("2:59").is_err());
        assert!(parse_pace("3:00").is_err());
        assert!(parse_pace("3:05").is_ok());
        assert!(parse_pace("abc").is_err());
        assert!(parse_pace("5:75").is_err());
    }
}
