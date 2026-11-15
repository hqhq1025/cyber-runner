//! `cyber-runner` — headless front end, handy for debugging a setup remotely.
//!
//! ```text
//! cyber-runner devices
//! cyber-runner run [--route xitucheng|shahe|<file>] [--datum bd09|gcj02|wgs84]
//!                  [--speed 3.2] [--minutes 30] [--km 3] [--udid <udid>]
//! cyber-runner restore [--udid <udid>]
//! ```

use std::process::ExitCode;
use std::time::Duration;

use cyber_runner_core::{
    format, normalize, parse_points, Datum, LoopMode, Variation, BUILTIN_ROUTES,
};
use cyber_runner_engine::{Engine, EngineConfig, FinishAction, Level, Phase, RunPlan, StopAfter};

struct Args(Vec<String>);

impl Args {
    fn flag(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .position(|a| a == name)
            .and_then(|i| self.0.get(i + 1))
            .map(String::as_str)
    }
    fn num(&self, name: &str) -> Result<Option<f64>, String> {
        self.flag(name)
            .map(|v| {
                v.parse::<f64>()
                    .map_err(|_| format!("{name} 需要数字，收到 {v}"))
            })
            .transpose()
    }
}

fn usage() -> ExitCode {
    eprintln!(
        "用法:\n  cyber-runner devices\n  cyber-runner run [--route xitucheng|shahe|<文件>] [--datum bd09|gcj02|wgs84] [--speed m/s] [--minutes N] [--km N] [--udid UDID]\n  cyber-runner restore [--udid UDID]"
    );
    ExitCode::from(2)
}

fn data_dir() -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| ".".into())
        .join("CyberRunner")
}

fn load_route(args: &Args) -> Result<(String, Vec<cyber_runner_core::LatLng>), String> {
    let key = args.flag("--route").unwrap_or("xitucheng");
    if let Some(b) = BUILTIN_ROUTES.iter().find(|r| r.id.ends_with(key)) {
        let p = parse_points(b.content).map_err(|e| e.to_string())?;
        return Ok((
            b.name.into(),
            normalize(&p.points, b.datum).map_err(|e| e.to_string())?,
        ));
    }
    let text = std::fs::read_to_string(key).map_err(|e| format!("读取 {key} 失败：{e}"))?;
    let parsed = parse_points(&text).map_err(|e| e.to_string())?;
    let datum = match args.flag("--datum") {
        Some("wgs84") => Datum::Wgs84,
        Some("gcj02") => Datum::Gcj02,
        Some("bd09") => Datum::Bd09,
        Some(other) => return Err(format!("未知坐标系 {other}")),
        None => parsed.datum.unwrap_or(Datum::Bd09),
    };
    Ok((
        key.into(),
        normalize(&parsed.points, datum).map_err(|e| e.to_string())?,
    ))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "warn".into()))
        .init();
    let args = Args(std::env::args().skip(1).collect());
    let Some(cmd) = args.0.first().cloned() else {
        return usage();
    };
    match cmd.as_str() {
        "devices" => match cyber_runner_engine::list_devices(&data_dir().join("pairing")).await {
            Ok(list) if list.is_empty() => {
                println!("没有检测到设备");
                ExitCode::SUCCESS
            }
            Ok(list) => {
                for d in list {
                    println!(
                        "{}  {}  iOS {}  {}  {}",
                        d.udid,
                        d.display_name(),
                        d.ios_version.as_deref().unwrap_or("?"),
                        d.connection,
                        if d.trusted { "已信任" } else { "未信任" }
                    );
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}\n{}", e.hint());
                ExitCode::FAILURE
            }
        },
        "restore" => {
            let engine = engine();
            match engine
                .restore_location(args.flag("--udid").map(String::from))
                .await
            {
                Ok(()) => {
                    println!("已恢复真实定位");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}\n{}", e.hint());
                    ExitCode::FAILURE
                }
            }
        }
        "run" => match run(&args).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        _ => usage(),
    }
}

fn engine() -> Engine {
    let engine = Engine::spawn(EngineConfig {
        pair_dir: data_dir().join("pairing"),
        ddi_cache_dir: data_dir().join("ddi"),
        ddi_extra_dir: None,
    });
    let mut logs = engine.logs();
    tokio::spawn(async move {
        while let Ok(l) = logs.recv().await {
            let tag = match l.level {
                Level::Info => "·",
                Level::Warn => "!",
                Level::Error => "✗",
            };
            eprintln!("{tag} {}", l.message);
        }
    });
    engine
}

async fn run(args: &Args) -> Result<(), String> {
    let (name, points) = load_route(args)?;
    let speed = args.num("--speed")?.unwrap_or(3.2);
    let plan = RunPlan {
        udid: args.flag("--udid").map(String::from),
        route_name: name,
        points,
        loop_mode: LoopMode::Auto,
        speed_mps: speed,
        variation: Variation::default(),
        stop_after: StopAfter {
            duration_s: args.num("--minutes")?.map(|m| m * 60.0),
            distance_m: args.num("--km")?.map(|k| k * 1000.0),
        },
        on_finish: FinishAction::Restore,
    };
    let engine = engine();
    let mut state = engine.subscribe();
    engine.start(plan).map_err(|e| e.to_string())?;
    eprintln!("Ctrl+C 停止并恢复真实定位");
    let mut ticker = tokio::time::interval(Duration::from_secs(5));
    let mut started = false;
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                engine.stop().await.ok();
                return Ok(());
            }
            changed = state.changed() => {
                if changed.is_err() { return Ok(()); }
                let s = state.borrow_and_update().clone();
                if s.phase != Phase::Idle { started = true; }
                if started && s.phase == Phase::Idle {
                    return match s.error {
                        Some(e) => Err(format!("{}\n{}", e.message, e.hint)),
                        None => Ok(()),
                    };
                }
            }
            _ = ticker.tick() => {
                let s = engine.snapshot();
                if matches!(s.phase, Phase::Running | Phase::Paused | Phase::Finished) {
                    eprintln!(
                        "{}  {:.2} km  {} 圈  配速 {}",
                        format::duration_text(s.moving_s),
                        s.distance_m / 1000.0,
                        s.laps,
                        format::pace_text(s.speed_mps),
                    );
                }
            }
        }
    }
}
