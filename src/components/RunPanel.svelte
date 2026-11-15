<script lang="ts">
  // The "treadmill display": how far you've run, how much is left, when you finish.
  import { app } from "../lib/store.svelte";
  import { durationText, km, paceText } from "../lib/format";
  import Readiness from "./Readiness.svelte";
  import { primeAudio } from "../lib/alert";

  const s = $derived(app.snapshot);
  const st = $derived(app.settings);
  const phase = $derived(s.phase);

  const PHASE: Record<string, { label: string; tone: string }> = {
    idle: { label: "准备就绪", tone: "idle" },
    connecting: { label: "正在连接", tone: "busy" },
    running: { label: "跑步中", tone: "run" },
    paused: { label: "已暂停", tone: "pause" },
    reconnecting: { label: "连接中断，重连中", tone: "busy" },
    finished: { label: "跑完了", tone: "done" },
    stopping: { label: "正在停止", tone: "busy" },
  };
  const head = $derived(PHASE[phase] ?? PHASE.idle);

  // Before the first run, preview the target from the current settings.
  const fresh = $derived(phase === "idle" && s.distanceM === 0);
  const needsSetup = $derived(!!app.devices.error || !app.device);
  const avgSpeed = $derived(s.movingS > 5 ? s.distanceM / s.movingS : 0);
  const target = $derived.by(() => {
    if (fresh) {
      return {
        distanceM: st.goal === "distance" ? (st.goalKm + st.goalMarginKm) * 1000 : null,
        durationS: st.goal === "time" ? st.goalMinutes * 60 : null,
      };
    }
    return s.stopAfter;
  });
  const speedForEta = $derived(avgSpeed || s.targetSpeedMps || st.speedMps);
  const view = $derived.by(() => {
    if (target.distanceM) {
      const left = Math.max(0, target.distanceM - s.distanceM);
      return {
        done: km(s.distanceM), doneUnit: "km",
        left: km(left), leftUnit: "km",
        frac: Math.min(1, s.distanceM / target.distanceM),
        eta: left / speedForEta,
        total: `${km(target.distanceM, 1)} km`,
      };
    }
    if (target.durationS) {
      const left = Math.max(0, target.durationS - s.movingS);
      return {
        done: durationText(s.movingS), doneUnit: "",
        left: durationText(left), leftUnit: "",
        frac: Math.min(1, s.movingS / target.durationS),
        eta: left,
        total: durationText(target.durationS),
      };
    }
    return { done: km(s.distanceM), doneUnit: "km", left: null, leftUnit: "", frac: 0, eta: 0, total: null };
  });
  const etaClock = $derived.by(() => {
    if (view.eta <= 0 || phase !== "running") return null;
    const d = new Date(Date.now() + view.eta * 1000);
    return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  });

  const plannedKms = $derived(target.distanceM ? Math.ceil(target.distanceM / 1000) : 0);

  const blockers = $derived.by(() => {
    const out: string[] = [];
    if (app.devices.error) out.push("缺少苹果设备服务");
    else if (app.devices.scanned && app.devices.devices.length === 0) out.push("请先用数据线连接手机");
    if (!app.selectedRoute) out.push("请先选择路线");
    return out;
  });
  const canStart = $derived(phase === "idle" && !app.busy && blockers.length === 0);

  let announce = $state("");
  $effect(() => {
    announce = s.step ?? head.label;
  });
</script>

<section class="card panel" aria-labelledby="run-title">
  <div class="status {head.tone}">
    <span class="pulse" aria-hidden="true"></span>
    <h2 id="run-title">{head.label}</h2>
    {#if s.routeName && phase !== "idle"}<span class="route">{s.routeName}</span>{/if}
    {#if s.step}<span class="step">{s.step}</span>{/if}
  </div>
  <p class="sr-only" aria-live="polite">{announce}</p>

  {#if phase === "idle" && s.error}
    <div class="error" role="alert">
      <div class="etitle">{s.error.message}</div>
      <div class="ehint">{s.error.hint}</div>
      <div class="eactions">
        <button class="btn sm primary" onclick={() => app.start()} disabled={!canStart}>重试</button>
      </div>
    </div>
  {:else if phase === "idle" && needsSetup}
    <Readiness />
  {/if}

  <!-- Hero: done vs left -->
  <div class="hero" class:done={phase === "finished"}>
    <div class="pair">
      <div class="half">
        <span class="hl">已跑</span>
        <span class="hv num">{view.done}<small>{view.doneUnit}</small></span>
      </div>
      {#if view.left !== null}
        <div class="half right">
          <span class="hl">{phase === "finished" ? "目标" : "还剩"}</span>
          <span class="hv num dim">{phase === "finished" ? view.total : view.left}<small>{phase === "finished" ? "" : view.leftUnit}</small></span>
        </div>
      {/if}
    </div>
    {#if view.left !== null}
      <div class="bar" role="progressbar" aria-label="目标进度" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(view.frac * 100)}>
        <div class="fill" style="width:{view.frac * 100}%"></div>
      </div>
      <div class="sub num">
        <span>{Math.round(view.frac * 100)}%</span>
        <span>目标 {view.total}</span>
      </div>
      <div class="when num">
        {#if fresh}
          <span class="wl">预计用时</span><span class="wv">{durationText(view.eta)}</span>
        {:else if phase === "finished"}
          <span class="wl">完成</span><span class="wv">用时 {durationText(s.movingS)}</span>
        {:else if etaClock}
          <span class="wl">预计</span><span class="wv">{etaClock} 跑完</span><span class="wl">还需 {durationText(view.eta)}</span>
        {:else}
          <span class="wl">还需</span><span class="wv">{durationText(view.eta)}</span>
        {/if}
      </div>
    {:else}
      <div class="sub"><span>不限距离，手动结束</span></div>
    {/if}
  </div>

  <div class="metrics">
    <div class="metric">
      <span class="label">用时</span>
      <span class="value num">{durationText(s.movingS)}</span>
    </div>
    <div class="metric">
      <span class="label">当前配速</span>
      <span class="value num">{phase === "running" ? paceText(s.speedMps) : "--'--\""}</span>
    </div>
    <div class="metric">
      <span class="label">{fresh ? "设定配速" : "平均配速"}</span>
      <span class="value num">{fresh ? paceText(st.speedMps) : paceText(avgSpeed)}</span>
    </div>
    <div class="metric">
      <span class="label">圈数</span>
      <span class="value num">{(s.laps + (phase === "idle" ? 0 : s.lapProgress)).toFixed(1)}</span>
    </div>
  </div>


  {#if phase === "finished"}
    <div class="finish" role="status">
      <strong>已停在终点。</strong>先在校园跑 App 里<strong>结束跑步</strong>，再点「结束并恢复定位」。
    </div>
  {:else if phase === "idle" && !fresh}
    <p class="lastrun">上次跑步的成绩 · 下次开始时清零</p>
  {/if}

  <div class="controls">
    {#if phase === "idle"}
      <button class="btn primary go" disabled={!canStart} onclick={() => { primeAudio(); app.start(); }}>
        <svg width="16" height="16" viewBox="0 0 24 24" aria-hidden="true"><path d="M7 4.5v15l13-7.5z" fill="currentColor" /></svg>
        开始跑步
      </button>
    {:else if phase === "connecting"}
      <button class="btn go" onclick={() => app.stop()} disabled={app.busy}>取消</button>
    {:else if phase === "finished"}
      <button class="btn primary go" onclick={() => app.stop()} disabled={app.busy}>结束并恢复定位</button>
      <button class="btn" onclick={() => app.resume()}>继续跑</button>
    {:else if phase === "stopping"}
      <button class="btn go" disabled>正在恢复真实定位…</button>
    {:else}
      {#if phase === "paused"}
        <button class="btn primary go" onclick={() => app.resume()}>继续</button>
      {:else}
        <button class="btn go" onclick={() => app.pause()} disabled={phase === "reconnecting"}>暂停</button>
      {/if}
      <button class="btn stop" onclick={() => app.stop()} disabled={app.busy}>结束</button>
    {/if}
  </div>
  {#if phase === "idle" && blockers.length > 0 && !s.error}
    <p class="blocker">{blockers[0]}</p>
  {/if}
  {#if phase === "paused"}
    <p class="blocker">暂停时定位停在原地；结束后才会恢复真实定位。</p>
  {/if}
</section>

<style>
  .panel {
    padding: 18px 20px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-height: 0;
    overflow: auto;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    min-height: 24px;
  }
  .status h2 {
    margin: 0;
    font-size: 16px;
    letter-spacing: -0.01em;
  }
  .route {
    font-size: 12.5px;
    color: var(--muted);
  }
  .step {
    flex-basis: 100%;
    font-size: 12.5px;
    color: var(--fg-2);
    padding-left: 20px;
  }
  .pulse {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--muted);
  }
  .run .pulse {
    background: var(--accent);
    animation: beat 1.2s ease-in-out infinite;
  }
  .busy .pulse {
    background: var(--warn);
    animation: beat 0.8s ease-in-out infinite;
  }
  .pause .pulse {
    background: var(--warn);
  }
  .done .pulse {
    background: var(--accent);
  }
  @keyframes beat {
    50% {
      transform: scale(0.6);
      opacity: 0.6;
    }
  }

  .hero {
    flex: 1;
    min-height: 0;
    padding: 22px 24px 20px;
    border-radius: 14px;
    background: var(--accent-soft);
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 14px;
  }
  .when {
    margin-top: 6px;
    display: flex;
    align-items: baseline;
    justify-content: center;
    gap: 10px;
    padding-top: 14px;
    border-top: 1px solid var(--line);
  }
  .wl {
    font-size: 14px;
    color: var(--fg-2);
  }
  .wv {
    font-size: 30px;
    font-weight: 700;
  }
  .hero.done {
    border: 1px solid color-mix(in srgb, var(--accent) 40%, transparent);
  }
  .pair {
    display: flex;
    justify-content: space-between;
    align-items: flex-end;
    gap: 16px;
  }
  .half {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .half.right {
    align-items: flex-end;
  }
  .hl {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--fg-2);
  }
  .hv {
    font-size: 72px;
    font-weight: 750;
    line-height: 1;
    letter-spacing: -0.02em;
  }
  .hv.dim {
    color: var(--fg-2);
  }
  .hv small {
    font-size: 17px;
    font-weight: 650;
    margin-left: 5px;
    color: var(--fg-2);
    letter-spacing: 0;
  }
  .bar {
    height: 14px;
    border-radius: 99px;
    background: var(--panel);
    border: 1px solid var(--line);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent);
    border-radius: inherit;
  }
  .sub {
    display: flex;
    justify-content: space-between;
    font-size: 13px;
    font-weight: 550;
    color: var(--fg-2);
  }

  .metrics {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 10px;
  }
  .metric {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 9px 12px;
    border-radius: 10px;
    background: var(--panel-2);
  }
  .label {
    font-size: 11.5px;
    color: var(--muted);
  }
  .value {
    font-size: 21px;
    font-weight: 650;
  }


  .controls {
    display: flex;
    gap: 8px;
  }
  .controls .btn {
    height: 48px;
    font-size: 15.5px;
    border-radius: 12px;
  }
  .go {
    flex: 1;
  }
  .stop {
    width: 96px;
    color: var(--danger);
  }
  .blocker,
  .lastrun {
    margin: -6px 0 0;
    font-size: 12.5px;
    color: var(--muted);
    text-align: center;
  }
  .error {
    border-radius: 10px;
    padding: 12px;
    background: var(--danger-soft);
    border: 1px solid color-mix(in srgb, var(--danger) 30%, transparent);
  }
  .etitle {
    font-weight: 650;
    color: var(--danger);
  }
  .ehint {
    margin-top: 4px;
    font-size: 13px;
    color: var(--fg-2);
  }
  .eactions {
    margin-top: 10px;
  }
  .finish {
    font-size: 14px;
    padding: 12px 14px;
    border-radius: 10px;
    background: var(--accent-soft);
    border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
    color: var(--fg);
  }
</style>
