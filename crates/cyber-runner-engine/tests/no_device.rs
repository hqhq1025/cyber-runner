//! Without a phone (CI machines), a start must fail fast with an actionable
//! error and leave the engine idle and reusable — never hang.

use std::time::Duration;

use cyber_runner_core::{normalize, parse_points, LoopMode, Variation, BUILTIN_ROUTES};
use cyber_runner_engine::{Engine, EngineConfig, FinishAction, Phase, RunPlan, StopAfter};

#[tokio::test]
async fn start_without_device_reports_error_and_returns_to_idle() {
    let tmp = std::env::temp_dir().join(format!("cr-engine-{}", std::process::id()));
    let engine = Engine::spawn(EngineConfig {
        pair_dir: tmp.join("pairing"),
        ddi_cache_dir: tmp.join("ddi"),
        ddi_extra_dir: None,
    });
    let b = &BUILTIN_ROUTES[0];
    let points = normalize(&parse_points(b.content).unwrap().points, b.datum).unwrap();
    let plan = RunPlan {
        // A UDID that is never attached, so this also holds on a dev box
        // with a real phone plugged in.
        udid: Some("00000000-NOT-A-REAL-DEVICE".into()),
        route_name: b.name.into(),
        points,
        loop_mode: LoopMode::Auto,
        speed_mps: 3.0,
        variation: Variation::default(),
        stop_after: StopAfter::default(),
        on_finish: FinishAction::Hold,
    };
    let mut rx = engine.subscribe();
    engine.start(plan.clone()).unwrap();
    let snap = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            rx.changed().await.unwrap();
            let s = rx.borrow_and_update().clone();
            if s.phase == Phase::Idle && s.error.is_some() {
                return s;
            }
        }
    })
    .await
    .expect("engine must give up quickly without a device");
    let code = snap.error.unwrap().code;
    assert!(
        ["device_gone", "no_device", "usbmuxd"].contains(&code.as_str()),
        "unexpected error code {code}"
    );
    assert!(!snap.simulating);
    // Still usable afterwards.
    engine.start(plan).unwrap();
    engine.stop().await.unwrap();
}
