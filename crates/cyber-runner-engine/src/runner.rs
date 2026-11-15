//! The run actor. One tokio task owns the device connection and the motion
//! model; the UI talks to it through [`Engine`] (commands in, snapshots out).
//!
//! Compared to the old helper/Python design:
//! - no file IPC, no PID files, no second process that can be orphaned;
//! - speed / variation changes apply live;
//! - USB hiccups are retried transparently, the run resumes where it was;
//! - stop always tries to restore the real location (and falls back to a
//!   fresh connection if the current one is dead).

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use cyber_runner_core::{LatLng, LoopMode, Motion, Track, Variation};
use idevice::dvt::location_simulation::LocationSimulationClient;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio::time::{timeout, MissedTickBehavior};

use crate::device::{self, DeviceInfo, DeviceProvider};
use crate::error::{EngineError, ErrorInfo, Result};
use crate::{ddi, link};

/// Position update rate. iOS itself reports GPS at ~1 Hz; 4 Hz keeps the
/// blue dot smooth without loading the link.
const TICK: Duration = Duration::from_millis(250);
/// While paused / finished the position is re-sent at this rate (keeps the
/// DVT channel alive and detects a lost link).
const KEEPALIVE: Duration = Duration::from_secs(2);
const SET_TIMEOUT: Duration = Duration::from_secs(5);
const CLEAR_TIMEOUT: Duration = Duration::from_secs(4);
/// Give up reconnecting after this long and end the run.
const RECONNECT_WINDOW: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StopAfter {
    pub duration_s: Option<f64>,
    pub distance_m: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum FinishAction {
    /// Stay at the finish point until the user presses stop (lets them end
    /// the run in the campus app first).
    #[default]
    Hold,
    /// Immediately restore the real location.
    Restore,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunPlan {
    pub udid: Option<String>,
    pub route_name: String,
    /// WGS-84, normalized (see `cyber_runner_core::normalize`).
    pub points: Vec<LatLng>,
    pub loop_mode: LoopMode,
    pub speed_mps: f64,
    pub variation: Variation,
    pub stop_after: StopAfter,
    pub on_finish: FinishAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Idle,
    Connecting,
    Running,
    Paused,
    Reconnecting,
    Finished,
    Stopping,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub phase: Phase,
    /// Human readable current step ("正在挂载开发者磁盘镜像…").
    pub step: Option<String>,
    pub device: Option<DeviceInfo>,
    pub route_name: Option<String>,
    pub lap_length_m: f64,
    pub distance_m: f64,
    pub laps: u32,
    pub lap_progress: f64,
    /// Time spent actually moving (excludes pause / reconnect).
    pub moving_s: f64,
    pub speed_mps: f64,
    pub target_speed_mps: f64,
    pub position: Option<LatLng>,
    pub stop_after: StopAfter,
    pub on_finish: FinishAction,
    /// Progress towards the stop target, 0..1.
    pub goal_progress: Option<f64>,
    pub error: Option<ErrorInfo>,
    /// The phone's location may currently be simulated by us (stays true
    /// after a stop whose restore failed, until a restore succeeds).
    pub simulating: bool,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            step: None,
            device: None,
            route_name: None,
            lap_length_m: 0.0,
            distance_m: 0.0,
            laps: 0,
            lap_progress: 0.0,
            moving_s: 0.0,
            speed_mps: 0.0,
            target_speed_mps: 0.0,
            position: None,
            stop_after: StopAfter::default(),
            on_finish: FinishAction::default(),
            goal_progress: None,
            error: None,
            simulating: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LogLine {
    pub ts_ms: u64,
    pub level: Level,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Where pairing records created by us are kept.
    pub pair_dir: PathBuf,
    /// Where the downloaded DDI is cached.
    pub ddi_cache_dir: PathBuf,
    /// Optional user-supplied folder containing a DDI.
    pub ddi_extra_dir: Option<PathBuf>,
}

enum Cmd {
    Start(Box<RunPlan>),
    SetSpeed(f64),
    SetVariation(Variation),
    SetStopAfter(StopAfter, FinishAction),
    Pause,
    Resume,
    Stop(oneshot::Sender<()>),
    Restore(Option<String>, oneshot::Sender<Result<()>>),
}

/// Handle to the run actor. Cheap to clone.
#[derive(Clone)]
pub struct Engine {
    tx: mpsc::UnboundedSender<Cmd>,
    state: watch::Receiver<Snapshot>,
    devices: watch::Receiver<DeviceList>,
    logs: broadcast::Sender<LogLine>,
}

/// What the device watcher currently sees.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeviceList {
    /// False until the first scan finished.
    pub scanned: bool,
    pub devices: Vec<DeviceInfo>,
    /// Set when usbmuxd itself is unreachable (e.g. no Apple driver on Windows).
    pub error: Option<ErrorInfo>,
}

const DEVICE_POLL: Duration = Duration::from_millis(1500);

async fn watch_devices(tx: watch::Sender<DeviceList>, pair_dir: PathBuf) {
    // Name / version lookups need a lockdown round trip, so cache them per
    // UDID and only refresh when trust state flips.
    let mut cache: std::collections::HashMap<String, DeviceInfo> = Default::default();
    loop {
        let next = match device::list_devices_cached(&mut cache, &pair_dir).await {
            Ok(devices) => DeviceList {
                scanned: true,
                devices,
                error: None,
            },
            Err(e) => DeviceList {
                scanned: true,
                devices: vec![],
                error: Some(e.info()),
            },
        };
        tx.send_if_modified(|cur| {
            if *cur != next {
                *cur = next;
                true
            } else {
                false
            }
        });
        if tx.is_closed() {
            return;
        }
        tokio::time::sleep(DEVICE_POLL).await;
    }
}

impl Engine {
    /// Starts the actor on its own thread with a single-threaded runtime.
    /// Device I/O never blocks or competes with the UI's runtime, and the
    /// `idevice` futures don't have to be `Send`.
    pub fn spawn(config: EngineConfig) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let (state_tx, state) = watch::channel(Snapshot::default());
        let (logs, _) = broadcast::channel(512);
        let (devices_tx, devices) = watch::channel(DeviceList::default());
        let pair_dir = config.pair_dir.clone();
        let actor = Actor {
            config,
            rx,
            state: state_tx,
            log: Logger(logs.clone()),
        };
        std::thread::Builder::new()
            .name("cyber-runner-engine".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("engine runtime");
                let local = tokio::task::LocalSet::new();
                local.spawn_local(watch_devices(devices_tx, pair_dir));
                rt.block_on(local.run_until(actor.run()));
            })
            .expect("spawn engine thread");
        Self {
            tx,
            state,
            devices,
            logs,
        }
    }

    fn send(&self, cmd: Cmd) -> Result<()> {
        self.tx.send(cmd).map_err(|_| EngineError::EngineGone)
    }

    /// Validate the plan and start. Progress and errors arrive via
    /// [`Engine::subscribe`].
    pub fn start(&self, plan: RunPlan) -> Result<()> {
        if plan.points.len() < 2 {
            return Err(cyber_runner_core::RouteError::TooFewPoints.into());
        }
        if !matches!(self.state.borrow().phase, Phase::Idle) {
            return Err(EngineError::Busy);
        }
        self.send(Cmd::Start(Box::new(plan)))
    }

    pub fn set_speed(&self, mps: f64) -> Result<()> {
        self.send(Cmd::SetSpeed(mps))
    }

    pub fn set_variation(&self, v: Variation) -> Result<()> {
        self.send(Cmd::SetVariation(v))
    }

    pub fn set_stop_after(&self, s: StopAfter, f: FinishAction) -> Result<()> {
        self.send(Cmd::SetStopAfter(s, f))
    }

    pub fn pause(&self) -> Result<()> {
        self.send(Cmd::Pause)
    }

    pub fn resume(&self) -> Result<()> {
        self.send(Cmd::Resume)
    }

    /// Stop and restore the real location. Resolves once done (bounded).
    pub async fn stop(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.send(Cmd::Stop(tx))?;
        let _ = timeout(Duration::from_secs(12), rx).await;
        Ok(())
    }

    /// When idle: connect and clear any simulated location (recovery after a
    /// crash, or after another tool left the phone "stuck").
    pub async fn restore_location(&self, udid: Option<String>) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.send(Cmd::Restore(udid, tx))?;
        rx.await.map_err(|_| EngineError::EngineGone)?
    }

    pub fn snapshot(&self) -> Snapshot {
        self.state.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<Snapshot> {
        self.state.clone()
    }

    pub fn devices(&self) -> watch::Receiver<DeviceList> {
        self.devices.clone()
    }

    pub fn logs(&self) -> broadcast::Receiver<LogLine> {
        self.logs.subscribe()
    }
}

#[derive(Clone)]
struct Logger(broadcast::Sender<LogLine>);

impl Logger {
    fn emit(&self, level: Level, message: impl Into<String>) {
        let message = message.into();
        match level {
            Level::Info => tracing::info!("{message}"),
            Level::Warn => tracing::warn!("{message}"),
            Level::Error => tracing::error!("{message}"),
        }
        let ts_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let _ = self.0.send(LogLine {
            ts_ms,
            level,
            message,
        });
    }
    fn info(&self, m: impl Into<String>) {
        self.emit(Level::Info, m)
    }
    fn warn(&self, m: impl Into<String>) {
        self.emit(Level::Warn, m)
    }
    fn error(&self, m: impl Into<String>) {
        self.emit(Level::Error, m)
    }
}

struct Actor {
    config: EngineConfig,
    rx: mpsc::UnboundedReceiver<Cmd>,
    state: watch::Sender<Snapshot>,
    log: Logger,
}

/// Mutable run state that survives reconnects.
struct Run {
    plan: RunPlan,
    motion: Motion,
    moving_s: f64,
    paused: bool,
    finished: bool,
}

enum Outcome {
    /// `cleared`: the real location was restored.
    Stopped(Option<oneshot::Sender<()>>, bool),
    LinkLost(EngineError),
    /// Finished with `Restore`, already cleared.
    Done,
}

/// Commands that arrive while some async step is in flight.
enum Interrupt {
    Stop(Option<oneshot::Sender<()>>),
}

impl Actor {
    async fn run(mut self) {
        while let Some(cmd) = self.rx.recv().await {
            match cmd {
                Cmd::Start(plan) => self.run_plan(*plan).await,
                Cmd::Restore(udid, reply) => {
                    let r = self.restore(udid).await;
                    let _ = reply.send(r);
                }
                Cmd::Stop(reply) => {
                    let _ = reply.send(());
                }
                // Settings while idle are applied at the next start (the
                // UI sends them as part of the plan).
                _ => {}
            }
        }
    }

    fn publish(&self, f: impl FnOnce(&mut Snapshot)) {
        self.state.send_modify(f);
    }

    fn fail(&self, err: &EngineError) {
        self.log
            .error(format!("{err}{} — {}", err.detail(), err.hint()));
        let info = err.info();
        self.publish(|s| {
            s.phase = Phase::Idle;
            s.step = None;
            s.error = Some(info);
        });
    }

    /// Apply a non-stop command to the run. Returns an interrupt for stop.
    fn apply(&self, run: &mut Run, cmd: Cmd) -> Option<Interrupt> {
        match cmd {
            Cmd::Stop(reply) => return Some(Interrupt::Stop(Some(reply))),
            Cmd::SetSpeed(v) => {
                run.motion.set_speed(v);
                run.plan.speed_mps = run.motion.base_speed();
                let v = run.motion.base_speed();
                self.publish(|s| s.target_speed_mps = v);
            }
            Cmd::SetVariation(v) => {
                run.motion.set_variation(v);
                run.plan.variation = v;
            }
            Cmd::SetStopAfter(sa, fa) => {
                run.plan.stop_after = sa;
                run.plan.on_finish = fa;
                // Raising the target after finishing continues the run.
                if run.finished && !goal_reached(run) {
                    run.finished = false;
                }
                self.publish(|s| {
                    s.stop_after = sa;
                    s.on_finish = fa;
                });
            }
            Cmd::Pause => run.paused = true,
            Cmd::Resume => {
                run.paused = false;
                if run.finished && goal_reached(run) {
                    // Resume after finishing = keep running without a target.
                    run.plan.stop_after = StopAfter::default();
                    self.publish(|s| s.stop_after = StopAfter::default());
                }
                run.finished = false;
            }
            Cmd::Start(_) => self.log.warn("已经在运行中，忽略重复的开始"),
            Cmd::Restore(_, reply) => {
                let _ = reply.send(Err(EngineError::Busy));
            }
        }
        None
    }

    /// Await `fut` while still processing commands. `Err(Interrupt)` if the
    /// user pressed stop (or the engine handle was dropped).
    async fn guarded<T>(
        &mut self,
        run: &mut Run,
        fut: impl std::future::Future<Output = T>,
    ) -> std::result::Result<T, Interrupt> {
        tokio::pin!(fut);
        loop {
            tokio::select! {
                r = &mut fut => return Ok(r),
                cmd = self.rx.recv() => match cmd {
                    None => return Err(Interrupt::Stop(None)),
                    Some(cmd) => if let Some(i) = self.apply(run, cmd) { return Err(i) },
                },
            }
        }
    }

    async fn run_plan(&mut self, plan: RunPlan) {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x5EED);
        let track = Track::new(&plan.points, plan.loop_mode);
        let lap_len = track.lap_length_m();
        let motion = Motion::new(track, plan.speed_mps, plan.variation, seed);
        let mut run = Run {
            motion,
            moving_s: 0.0,
            paused: false,
            finished: false,
            plan,
        };
        let start_fix = run.motion.fix();
        let (name, speed, sa, fa) = (
            run.plan.route_name.clone(),
            run.motion.base_speed(),
            run.plan.stop_after,
            run.plan.on_finish,
        );
        self.publish(|s| {
            *s = Snapshot {
                phase: Phase::Connecting,
                route_name: Some(name),
                lap_length_m: lap_len,
                target_speed_mps: speed,
                position: Some(start_fix.position),
                stop_after: sa,
                on_finish: fa,
                ..Snapshot::default()
            }
        });
        self.log.info(format!(
            "开始：{}（单圈 {:.0} 米，目标 {:.1} m/s）",
            run.plan.route_name, lap_len, speed
        ));

        let udid = run.plan.udid.clone();
        let started = Instant::now();
        let connected = self
            .guarded(&mut run, self_connect(self, udid.clone(), true))
            .await;
        let (provider, info, mut link) = match connected {
            Err(Interrupt::Stop(reply)) => {
                self.log.info("已取消");
                self.publish(|s| *s = Snapshot::default());
                if let Some(r) = reply {
                    let _ = r.send(());
                }
                return;
            }
            Ok(Err(e)) => return self.fail(&e),
            Ok(Ok(c)) => c,
        };
        self.log.info(format!(
            "已连接 {}（iOS {}），用时 {:.1} 秒",
            info.display_name(),
            info.ios_version.as_deref().unwrap_or("?"),
            started.elapsed().as_secs_f64()
        ));

        loop {
            match self.drive(&mut run, &mut link).await {
                Outcome::Stopped(reply, cleared) => {
                    drop(link);
                    self.finish_idle(None, cleared);
                    if let Some(r) = reply {
                        let _ = r.send(());
                    }
                    return;
                }
                Outcome::Done => {
                    self.finish_idle(Some("已完成目标并恢复真实定位".into()), true);
                    return;
                }
                Outcome::LinkLost(err) => {
                    self.log
                        .warn(format!("连接中断：{err}{}，正在自动重连…", err.detail()));
                    match self.reconnect(&mut run, provider.udid().to_string()).await {
                        Ok(Some(new_link)) => link = new_link,
                        Ok(None) => return, // stopped during reconnect
                        Err(e) => return self.fail(&e),
                    }
                }
            }
        }
    }

    fn finish_idle(&self, msg: Option<String>, cleared: bool) {
        if let Some(m) = &msg {
            self.log.info(m.clone());
        }
        self.publish(|s| {
            s.phase = Phase::Idle;
            s.step = None;
            if cleared {
                s.simulating = false;
            }
            s.speed_mps = 0.0;
        });
    }

    async fn reconnect(&mut self, run: &mut Run, udid: String) -> Result<Option<link::Link>> {
        let deadline = Instant::now() + RECONNECT_WINDOW;
        let mut backoff = Duration::from_millis(500);
        self.publish(|s| {
            s.phase = Phase::Reconnecting;
            s.step = Some("连接中断，正在重连（请保持数据线连接）…".into());
        });
        loop {
            let attempt = self
                .guarded(run, self_connect(self, Some(udid.clone()), true))
                .await;
            match attempt {
                Err(Interrupt::Stop(reply)) => {
                    // The device is unreachable, so the location can't be
                    // restored now; keep `simulating` so the UI offers it.
                    self.publish(|s| {
                        s.phase = Phase::Idle;
                        s.step = None;
                        s.speed_mps = 0.0;
                    });
                    self.log.warn(
                        "已停止。若手机定位仍未恢复，重新连接后点「恢复真实定位」或重启手机。",
                    );
                    if let Some(r) = reply {
                        let _ = r.send(());
                    }
                    return Ok(None);
                }
                Ok(Ok((_, _, link))) => {
                    self.log.info("已重新连接，继续跑步");
                    return Ok(Some(link));
                }
                Ok(Err(e)) => {
                    if Instant::now() >= deadline {
                        return Err(e);
                    }
                    let hard = matches!(
                        e,
                        EngineError::DeveloperModeOff
                            | EngineError::UnsupportedIos(_)
                            | EngineError::TrustDenied
                    );
                    if hard {
                        return Err(e);
                    }
                    self.publish(|s| s.step = Some(format!("重连中：{e}")));
                    if self
                        .guarded(run, tokio::time::sleep(backoff))
                        .await
                        .is_err()
                    {
                        self.publish(|s| {
                            s.phase = Phase::Idle;
                            s.step = None;
                            s.speed_mps = 0.0;
                        });
                        return Ok(None);
                    }
                    backoff = (backoff * 2).min(Duration::from_secs(5));
                }
            }
        }
    }

    async fn drive(&mut self, run: &mut Run, link: &mut link::Link) -> Outcome {
        let mut ls = match LocationSimulationClient::new(&mut link.server).await {
            Ok(ls) => ls,
            Err(e) => return Outcome::LinkLost(e.into()),
        };
        let mut tick = tokio::time::interval(TICK);
        tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut last = Instant::now();
        let mut last_sent: Option<Instant> = None;
        loop {
            tokio::select! {
                _ = tick.tick() => {
                    let now = Instant::now();
                    let dt = now.duration_since(last).as_secs_f64();
                    last = now;
                    let moving = !run.paused && !run.finished;
                    let fix = if moving {
                        run.moving_s += dt;
                        run.motion.advance(dt)
                    } else {
                        run.motion.fix()
                    };
                    let due = moving || last_sent.is_none_or(|t| now.duration_since(t) >= KEEPALIVE);
                    if due {
                        match timeout(SET_TIMEOUT, ls.set(fix.position.lat, fix.position.lng)).await {
                            Ok(Ok(())) => last_sent = Some(now),
                            Ok(Err(e)) => return Outcome::LinkLost(e.into()),
                            Err(_) => return Outcome::LinkLost(EngineError::Tunnel("设备无响应".into())),
                        }
                    }
                    if moving && goal_reached(run) {
                        run.finished = true;
                        match run.plan.on_finish {
                            FinishAction::Hold => self.log.info(
                                "已达到目标，停在终点。在校园跑 App 里结束后，再点「停止」恢复真实定位。",
                            ),
                            FinishAction::Restore => {
                                let _ = timeout(CLEAR_TIMEOUT, ls.clear()).await;
                                self.publish(|s| s.goal_progress = Some(1.0));
                                return Outcome::Done;
                            }
                        }
                    }
                    let phase = if run.finished {
                        Phase::Finished
                    } else if run.paused {
                        Phase::Paused
                    } else {
                        Phase::Running
                    };
                    let goal = goal_progress(run);
                    let target = run.motion.base_speed();
                    let moving_s = run.moving_s;
                    self.publish(|s| {
                        s.phase = phase;
                        s.step = None;
                        s.error = None;
                        s.simulating = true;
                        s.distance_m = fix.distance_m;
                        s.laps = fix.laps;
                        s.lap_progress = fix.lap_progress;
                        s.speed_mps = if moving { fix.speed_mps } else { 0.0 };
                        s.target_speed_mps = target;
                        s.position = Some(fix.position);
                        s.moving_s = moving_s;
                        s.goal_progress = goal;
                    });
                }
                cmd = self.rx.recv() => {
                    let interrupt = match cmd {
                        None => Some(Interrupt::Stop(None)),
                        Some(cmd) => self.apply(run, cmd),
                    };
                    if let Some(Interrupt::Stop(reply)) = interrupt {
                        self.publish(|s| {
                            s.phase = Phase::Stopping;
                            s.step = Some("正在恢复真实定位…".into());
                        });
                        let cleared = match timeout(CLEAR_TIMEOUT, ls.clear()).await {
                            Ok(Ok(())) => true,
                            _ => {
                                self.log.warn("当前连接无法恢复定位，尝试重新连接…");
                                let r = timeout(
                                    Duration::from_secs(8),
                                    Self::clear_fresh(run.plan.udid.clone(), self.config.pair_dir.clone()),
                                )
                                .await;
                                matches!(r, Ok(Ok(())))
                            }
                        };
                        if cleared {
                            self.log.info("已恢复真实定位");
                        } else {
                            self.log.error("恢复失败：请重新连接后点「恢复真实定位」，或重启手机。");
                        }
                        return Outcome::Stopped(reply, cleared);
                    }
                }
            }
        }
    }

    async fn clear_fresh(udid: Option<String>, pair_dir: PathBuf) -> Result<()> {
        let provider = device::provider_for(udid.as_deref()).await?;
        let provider = device::ensure_trusted(provider, &pair_dir, || {}).await?;
        let mut link = link::open(&provider).await?;
        let mut ls = LocationSimulationClient::new(&mut link.server).await?;
        ls.clear().await?;
        Ok(())
    }

    async fn restore(&self, udid: Option<String>) -> Result<()> {
        self.log.info("正在恢复真实定位…");
        self.publish(|s| {
            s.phase = Phase::Stopping;
            s.step = Some("正在恢复真实定位…".into());
            s.error = None;
        });
        let r = timeout(Duration::from_secs(30), async {
            let (_, _, mut link) = self_connect(self, udid, false).await?;
            let mut ls = LocationSimulationClient::new(&mut link.server).await?;
            ls.clear().await?;
            Ok::<_, EngineError>(())
        })
        .await
        .unwrap_or(Err(EngineError::Tunnel("超时".into())));
        match &r {
            Ok(()) => self.finish_idle(Some("已恢复真实定位".into()), true),
            Err(e) => self.fail(e),
        }
        r
    }
}

/// `guarded` borrows `self` mutably, so the connect future must not borrow
/// it. Build it from a cloned view instead.
fn self_connect(
    actor: &Actor,
    udid: Option<String>,
    ddi_needed: bool,
) -> impl std::future::Future<Output = Result<(DeviceProvider, DeviceInfo, link::Link)>> + 'static {
    let view = ActorView {
        config: actor.config.clone(),
        state: actor.state.clone(),
        log: actor.log.clone(),
    };
    async move { view.connect(udid.as_deref(), ddi_needed).await }
}

/// The parts of [`Actor`] needed to connect, without the command receiver.
struct ActorView {
    config: EngineConfig,
    state: watch::Sender<Snapshot>,
    log: Logger,
}

impl ActorView {
    async fn connect(
        &self,
        udid: Option<&str>,
        ddi_needed: bool,
    ) -> Result<(DeviceProvider, DeviceInfo, link::Link)> {
        let step = |m: &str| {
            self.log.info(m);
            self.state.send_modify(|s| s.step = Some(m.to_string()));
        };
        step("查找设备…");
        let provider = device::provider_for(udid).await?;
        let provider = device::ensure_trusted(provider, &self.config.pair_dir, || {
            step("请解锁手机，在弹窗中点「信任」并输入锁屏密码")
        })
        .await?;
        step("检查设备状态…");
        let pre = device::preflight(&provider).await?;
        self.state
            .send_modify(|s| s.device = Some(pre.info.clone()));
        if ddi_needed {
            step("检查开发者磁盘镜像…");
            let log = self.log.clone();
            let state = self.state.clone();
            let progress = move |m: String| {
                log.info(m.clone());
                state.send_modify(|s| s.step = Some(m));
            };
            ddi::ensure_mounted(
                &provider,
                pre.unique_chip_id,
                &self.config.ddi_cache_dir,
                self.config.ddi_extra_dir.as_deref(),
                &progress,
            )
            .await?;
        }
        step("建立定位通道…");
        let link = link::open(&provider).await?;
        Ok((provider, pre.info, link))
    }
}

fn goal_reached(run: &Run) -> bool {
    let sa = run.plan.stop_after;
    sa.duration_s.is_some_and(|d| run.moving_s >= d)
        || sa.distance_m.is_some_and(|d| run.motion.distance_m() >= d)
}

fn goal_progress(run: &Run) -> Option<f64> {
    let sa = run.plan.stop_after;
    let by_time = sa.duration_s.filter(|d| *d > 0.0).map(|d| run.moving_s / d);
    let by_dist = sa
        .distance_m
        .filter(|d| *d > 0.0)
        .map(|d| run.motion.distance_m() / d);
    match (by_time, by_dist) {
        (Some(a), Some(b)) => Some(a.max(b).min(1.0)),
        (a, b) => a.or(b).map(|v| v.min(1.0)),
    }
}
