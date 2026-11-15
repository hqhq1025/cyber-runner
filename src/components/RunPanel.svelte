<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { durationText, km, paceText } from "../lib/format";
  import Readiness from "./Readiness.svelte";

  const s = $derived(app.snapshot);
  const phase = $derived(s.phase);

  const PHASE: Record<string, { label: string; tone: string }> = {
    idle: { label: "准备就绪", tone: "idle" },
    connecting: { label: "正在连接", tone: "busy" },
    running: { label: "跑步中", tone: "run" },
    paused: { label: "已暂停", tone: "pause" },
    reconnecting: { label: "连接中断，重连中", tone: "busy" },
    finished: { label: "已达到目标", tone: "done" },
    stopping: { label: "正在停止", tone: "busy" },
  };
  const head = $derived(PHASE[phase] ?? PHASE.idle);

  // Live metrics while a run exists; otherwise show the last run (if any).
  const hasRun = $derived(s.distanceM > 0 || phase !== "idle");
  const goalLeft = $derived.by(() => {
    const sa = s.stopAfter;
    const v = s.targetSpeedMps || app.settings.speedMps;
    if (sa.distanceM) {
      const left = Math.max(0, sa.distanceM - s.distanceM);
      return { text: `${km(s.distanceM)} / ${km(sa.distanceM)} km`, eta: left / v };
    }
    if (sa.durationS) {
      return { text: `${durationText(s.movingS)} / ${durationText(sa.durationS)}`, eta: Math.max(0, sa.durationS - s.movingS) };
    }
    return null;
  });

  const blockers = $derived.by(() => {
    const out: string[] = [];
    if (app.devices.error) out.push("缺少苹果设备服务");
    else if (app.devices.scanned && app.devices.devices.length === 0) out.push("请先用数据线连接手机");
    if (!app.selectedRoute) out.push("请先选择路线");
    return out;
  });
  const canStart = $derived(phase === "idle" && !app.busy && blockers.length === 0);

  // Screen-reader announcement for phase changes only (not every tick).
  let announce = $state("");
  $effect(() => {
    announce = s.step ?? head.label;
  });
</script>

<section class="card panel" aria-labelledby="run-title">
  <div class="status {head.tone}">
    <span class="pulse" aria-hidden="true"></span>
    <h2 id="run-title">{head.label}</h2>
    {#if s.step}
      <span class="step">{s.step}</span>
    {/if}
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
  {:else if phase === "idle" && !hasRun}
    <Readiness />
  {/if}

  {#if hasRun && !(phase === "idle" && s.error)}
    <div class="metrics">
      <div class="metric big">
        <span class="label">距离</span>
        <span class="value num">{km(s.distanceM)}<small>km</small></span>
      </div>
      <div class="metric">
        <span class="label">用时</span>
        <span class="value num">{durationText(s.movingS)}</span>
      </div>
      <div class="metric">
        <span class="label">实时配速</span>
        <span class="value num">{phase === "running" ? paceText(s.speedMps) : "--'--\""}</span>
      </div>
      <div class="metric">
        <span class="label">圈数</span>
        <span class="value num">{s.laps}<small>+{Math.round(s.lapProgress * 100)}%</small></span>
      </div>
    </div>
    {#if goalLeft}
      <div class="goal">
        <div class="bar" role="progressbar" aria-label="目标进度" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round((s.goalProgress ?? 0) * 100)}>
          <div class="fill" style="width:{(s.goalProgress ?? 0) * 100}%"></div>
        </div>
        <div class="goaltext">
          <span class="num">{goalLeft.text}</span>
          {#if phase === "running" && goalLeft.eta > 0}
            <span class="muted">还需约 {durationText(goalLeft.eta)}</span>
          {/if}
        </div>
      </div>
    {/if}
  {/if}

  {#if phase === "finished"}
    <div class="finish">
      已停在终点。先在校园跑 App 里<strong>结束跑步</strong>，再点下面的按钮恢复真实定位。
    </div>
  {/if}

  <div class="controls">
    {#if phase === "idle"}
      <button class="btn primary go" disabled={!canStart} onclick={() => app.start()}>
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
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    min-height: 26px;
  }
  .status h2 {
    margin: 0;
    font-size: 17px;
    letter-spacing: -0.01em;
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
  .metrics {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    gap: 10px 12px;
  }
  .metric {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 8px 10px;
    border-radius: 10px;
    background: var(--panel-2);
  }
  .metric.big {
    grid-column: 1 / -1;
    background: var(--accent-soft);
  }
  .label {
    font-size: 11.5px;
    color: var(--muted);
  }
  .value {
    font-size: 19px;
    font-weight: 650;
  }
  .big .value {
    font-size: 40px;
    line-height: 1.05;
    font-weight: 700;
  }
  .value small {
    font-size: 0.45em;
    font-weight: 600;
    margin-left: 4px;
    color: var(--fg-2);
  }
  .goal {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .bar {
    height: 8px;
    border-radius: 99px;
    background: var(--panel-2);
    border: 1px solid var(--line);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent);
    border-radius: inherit;
    transition: width 0.3s linear;
  }
  .goaltext {
    display: flex;
    justify-content: space-between;
    font-size: 12.5px;
  }
  .muted {
    color: var(--muted);
  }
  .controls {
    display: flex;
    gap: 8px;
  }
  .controls .btn {
    height: 44px;
    font-size: 15px;
    border-radius: 12px;
  }
  .go {
    flex: 1;
  }
  .stop {
    width: 96px;
    color: var(--danger);
  }
  .blocker {
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
    font-size: 13px;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--accent-soft);
    color: var(--fg);
  }
</style>
