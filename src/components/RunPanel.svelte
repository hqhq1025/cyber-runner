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
    stopping: { label: "正在恢复定位", tone: "busy" },
  };
  const needsSetup = $derived(!!app.devices.error || !app.device);
  const head = $derived(
    phase === "idle" && needsSetup ? { label: "等待连接手机", tone: "idle" } : (PHASE[phase] ?? PHASE.idle),
  );

  // Before the first run, preview the target from the current settings.
  const fresh = $derived(phase === "idle" && s.distanceM === 0);
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
        done: km(s.distanceM),
        left: km(left),
        unit: "km",
        frac: Math.min(1, s.distanceM / target.distanceM),
        eta: left / speedForEta,
      };
    }
    if (target.durationS) {
      const left = Math.max(0, target.durationS - s.movingS);
      return {
        done: durationText(s.movingS),
        left: durationText(left),
        unit: "",
        frac: Math.min(1, s.movingS / target.durationS),
        eta: left,
      };
    }
    return { done: km(s.distanceM), left: null, unit: "km", frac: 0, eta: 0 };
  });
  const etaClock = $derived.by(() => {
    if (view.eta <= 0 || phase !== "running") return null;
    const d = new Date(Date.now() + view.eta * 1000);
    return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  });

  const blockers = $derived.by(() => {
    const out: string[] = [];
    if (app.devices.error) out.push("缺少苹果设备服务");
    else if (app.devices.scanned && app.devices.devices.length === 0) out.push("请先用数据线连接手机");
    if (!app.selectedRoute) out.push("请先选择路线");
    return out;
  });
  const canStart = $derived(phase === "idle" && !app.busy && blockers.length === 0);
  const pace = $derived(phase === "running" ? s.speedMps : avgSpeed || st.speedMps);

  let announce = $state("");
  $effect(() => {
    announce = s.step ?? head.label;
  });
</script>

<section class="card panel" aria-labelledby="run-title">
  <p class="sr-only" aria-live="polite">{announce}</p>

  {#if phase === "idle" && s.error}
    <div class="error" role="alert">
      <div class="etext">
        <strong>{s.error.message}</strong>
        <span>{s.error.hint}</span>
      </div>
      <button class="btn sm" onclick={() => app.start()} disabled={!canStart}>重试</button>
    </div>
  {:else if phase === "idle" && needsSetup}
    <Readiness />
  {/if}

  <div class="hero {head.tone}">
    <div class="status">
      <span class="pulse" aria-hidden="true"></span>
      <h2 id="run-title">{head.label}</h2>
      {#if s.step}<span class="step">{s.step}</span>{/if}
    </div>

    <div class="pair">
      <div class="half">
        <span class="hl">已跑</span>
        <span class="hv num">{view.done}<small>{view.unit}</small></span>
      </div>
      {#if view.left !== null}
        <div class="half right">
          <span class="hl">还剩</span>
          <span class="hv num dim">{view.left}<small>{view.unit}</small></span>
        </div>
      {/if}
    </div>

    {#if view.left !== null}
      <div class="bar" role="progressbar" aria-label="进度" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(view.frac * 100)}>
        <div class="fill" style="width:{view.frac * 100}%"></div>
      </div>
    {/if}

    <div class="when num">
      {#if phase === "finished"}
        用时 <b>{durationText(s.movingS)}</b>
      {:else if view.left === null}
        不限距离，手动结束
      {:else if etaClock}
        <b>{etaClock}</b> 跑完 · 还要 {durationText(view.eta)}
      {:else}
        预计用时 <b>{durationText(view.eta)}</b>
      {/if}
    </div>
  </div>

  <div class="metrics">
    <div class="metric">
      <span class="label">用时</span>
      <span class="value num">{durationText(s.movingS)}</span>
    </div>
    <div class="metric">
      <span class="label">配速</span>
      <span class="value num">{paceText(pace)}</span>
    </div>
    <div class="metric">
      <span class="label">圈数</span>
      <span class="value num">{(s.laps + (phase === "idle" ? 0 : s.lapProgress)).toFixed(1)}</span>
    </div>
  </div>

  {#if phase === "finished"}
    <p class="note strong">先在校园跑 App 里点<b>结束跑步</b>，再点下面的按钮。</p>
  {:else if phase === "paused"}
    <p class="note">暂停时定位停在原地。</p>
  {:else if phase === "idle" && blockers.length > 0 && !s.error && !needsSetup}
    <p class="note">{blockers[0]}</p>
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
      <button class="btn side" onclick={() => app.resume()}>继续跑</button>
    {:else if phase === "stopping"}
      <button class="btn go" disabled>正在恢复真实定位…</button>
    {:else}
      {#if phase === "paused"}
        <button class="btn primary go" onclick={() => app.resume()}>继续</button>
      {:else}
        <button class="btn go" onclick={() => app.pause()} disabled={phase === "reconnecting"}>暂停</button>
      {/if}
      <button class="btn side stop" onclick={() => app.stop()} disabled={app.busy}>结束</button>
    {/if}
  </div>
</section>

<style>
  .panel {
    padding: 18px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-height: 0;
    overflow: auto;
  }

  .error {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--danger-soft);
    border: 1px solid color-mix(in srgb, var(--danger) 30%, transparent);
  }
  .etext {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 13px;
    color: var(--fg-2);
  }
  .etext strong {
    color: var(--danger);
    font-size: 14px;
  }

  .hero {
    flex: 1 0 auto;
    padding: 18px 24px 22px;
    border-radius: 14px;
    background: var(--accent-soft);
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    gap: 18px;
  }
  .hero.done {
    border: 1px solid color-mix(in srgb, var(--accent) 40%, transparent);
  }

  .status {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .status h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 650;
  }
  .step {
    font-size: 13px;
    color: var(--fg-2);
  }
  .pulse {
    width: 9px;
    height: 9px;
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

  .pair {
    display: flex;
    justify-content: space-between;
    align-items: flex-end;
    gap: 16px;
  }
  .half {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .half.right {
    align-items: flex-end;
  }
  .hl {
    font-size: 13px;
    font-weight: 600;
    color: var(--fg-2);
  }
  .hv {
    font-size: 76px;
    font-weight: 750;
    line-height: 1;
    letter-spacing: -0.02em;
  }
  .hv.dim {
    color: var(--fg-2);
  }
  .hv small {
    font-size: 18px;
    font-weight: 650;
    margin-left: 5px;
    color: var(--fg-2);
    letter-spacing: 0;
  }
  .bar {
    height: 12px;
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
  .when {
    text-align: center;
    font-size: 15px;
    color: var(--fg-2);
  }
  .when b {
    font-size: 26px;
    font-weight: 700;
    color: var(--fg);
    margin: 0 4px;
  }

  .metrics {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
  }
  .metric {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 10px 14px;
    border-radius: 10px;
    background: var(--panel-2);
  }
  .label {
    font-size: 12px;
    color: var(--muted);
  }
  .value {
    font-size: 22px;
    font-weight: 650;
  }

  .note {
    margin: 0;
    font-size: 13px;
    color: var(--muted);
    text-align: center;
  }
  .note.strong {
    color: var(--fg);
  }

  .controls {
    display: flex;
    gap: 8px;
  }
  .controls .btn {
    height: 50px;
    font-size: 16px;
    border-radius: 12px;
  }
  .go {
    flex: 1;
  }
  .side {
    width: 110px;
  }
  .stop {
    color: var(--danger);
  }
</style>
