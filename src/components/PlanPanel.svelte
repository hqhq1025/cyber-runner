<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { durationText, paceText } from "../lib/format";
  import Segmented from "./Segmented.svelte";
  import type { FinishAction, GoalKind, Realism } from "../lib/api";

  const S_MIN = 1.0;
  const S_MAX = 6.5;
  const PRESETS = [
    { label: "快走", v: 1.8 },
    { label: "慢跑", v: 2.6 },
    { label: "常规", v: 3.2 },
    { label: "较快", v: 4.0 },
  ];
  const st = $derived(app.settings);
  const live = $derived(app.active && app.phase !== "stopping");
  const fill = $derived(((Math.min(S_MAX, Math.max(S_MIN, st.speedMps)) - S_MIN) / (S_MAX - S_MIN)) * 100);

  const estimate = $derived.by(() => {
    if (st.goal === "distance") return `预计用时 ${durationText((st.goalKm * 1000) / st.speedMps)}`;
    if (st.goal === "time") return `预计 ${((st.speedMps * st.goalMinutes * 60) / 1000).toFixed(2)} km`;
    return "一直跑，直到手动结束";
  });

  function setSpeed(v: number) {
    app.update({ speedMps: Math.round(Math.min(S_MAX, Math.max(S_MIN, v)) * 10) / 10 });
  }
  function stepGoal(delta: number) {
    if (st.goal === "distance") app.update({ goalKm: Math.max(0.1, Math.round((st.goalKm + delta * 0.1) * 10) / 10) });
    else app.update({ goalMinutes: Math.max(1, st.goalMinutes + delta) });
  }
  function onGoalInput(e: Event) {
    const v = Number((e.currentTarget as HTMLInputElement).value);
    if (!isFinite(v) || v <= 0) return;
    if (st.goal === "distance") app.update({ goalKm: Math.min(100, v) });
    else app.update({ goalMinutes: Math.min(600, Math.round(v)) });
  }

  const REALISM_HINT: Record<Realism, string> = {
    off: "严格匀速、贴线行进",
    low: "轻微的速度与轨迹波动",
    normal: "像真人一样：每圈配速不同，轨迹略有偏移",
    high: "波动更明显（±7% 配速，±3.5 m 偏移）",
  };
</script>

<section class="card panel" aria-labelledby="plan-title">
  <div class="row-head">
    <h2 class="card-title" id="plan-title">跑步设置</h2>
    {#if live}<span class="live">修改实时生效</span>{/if}
  </div>

  <div class="group">
    <div class="pace">
      <div>
        <span class="pace-big num">{paceText(st.speedMps)}</span><span class="unit">/km</span>
      </div>
      <span class="speed num">{st.speedMps.toFixed(1)} m/s · {(st.speedMps * 3.6).toFixed(1)} km/h</span>
    </div>
    <label class="sr-only" for="speed">速度（米每秒）</label>
    <input
      id="speed"
      class="range"
      type="range"
      min={S_MIN}
      max={S_MAX}
      step="0.1"
      value={st.speedMps}
      style="--fill:{fill}%"
      aria-valuetext="配速 {paceText(st.speedMps)} 每公里"
      oninput={(e) => setSpeed(Number(e.currentTarget.value))}
    />
    <div class="presets">
      {#each PRESETS as p (p.v)}
        <button
          class="chip"
          class:on={Math.abs(st.speedMps - p.v) < 0.05}
          onclick={() => setSpeed(p.v)}
        >
          <span>{p.label}</span><span class="num">{paceText(p.v)}</span>
        </button>
      {/each}
    </div>
  </div>

  <div class="group">
    <span class="glabel">目标</span>
    <Segmented
      label="目标"
      value={st.goal}
      options={[
        { value: "distance" as GoalKind, label: "按距离" },
        { value: "time" as GoalKind, label: "按时间" },
        { value: "none" as GoalKind, label: "不限" },
      ]}
      onchange={(v) => app.update({ goal: v })}
    />
    {#if st.goal !== "none"}
      <div class="goalrow">
        <div class="stepper">
          <button class="btn icon" aria-label="减少" onclick={() => stepGoal(-1)}>−</button>
          <input
            class="input num"
            type="number"
            min={st.goal === "distance" ? 0.1 : 1}
            step={st.goal === "distance" ? 0.1 : 1}
            value={st.goal === "distance" ? st.goalKm : st.goalMinutes}
            onchange={onGoalInput}
            aria-label={st.goal === "distance" ? "目标距离（公里）" : "目标时间（分钟）"}
          />
          <span class="u">{st.goal === "distance" ? "km" : "分钟"}</span>
          <button class="btn icon" aria-label="增加" onclick={() => stepGoal(1)}>+</button>
        </div>
        <span class="est">{estimate}</span>
      </div>
      <div class="finish">
        <span class="flabel">到达后</span>
        <Segmented
          size="sm"
          label="到达目标后"
          value={st.onFinish}
          options={[
            { value: "hold" as FinishAction, label: "停在终点", title: "方便先在校园跑 App 里结束，再恢复定位" },
            { value: "restore" as FinishAction, label: "立即恢复定位" },
          ]}
          onchange={(v) => app.update({ onFinish: v })}
        />
      </div>
    {:else}
      <span class="est">{estimate}</span>
    {/if}
  </div>

  <div class="group">
    <span class="glabel">拟真程度</span>
    <Segmented
      label="拟真程度"
      value={st.realism}
      options={[
        { value: "off" as Realism, label: "关" },
        { value: "low" as Realism, label: "低" },
        { value: "normal" as Realism, label: "中" },
        { value: "high" as Realism, label: "高" },
      ]}
      onchange={(v) => app.update({ realism: v })}
    />
    <span class="est">{REALISM_HINT[st.realism]}</span>
  </div>
</section>

<style>
  .panel {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .row-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .live {
    font-size: 11.5px;
    color: var(--accent);
    font-weight: 600;
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .glabel,
  .flabel {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--fg-2);
  }
  .pace {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }
  .pace-big {
    font-size: 30px;
    font-weight: 700;
  }
  .unit {
    margin-left: 3px;
    color: var(--muted);
    font-weight: 600;
  }
  .speed {
    font-size: 12px;
    color: var(--muted);
  }
  .presets {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 6px;
  }
  .chip {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 5px 0;
    border-radius: 9px;
    border: 1px solid var(--line);
    background: var(--panel-2);
    font-size: 12px;
    color: var(--fg-2);
    line-height: 1.25;
  }
  .chip .num {
    font-size: 11px;
    color: var(--muted);
  }
  .chip:hover {
    border-color: var(--line-strong);
  }
  .chip.on {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--fg);
  }
  .goalrow {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .stepper {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .stepper .input {
    width: 64px;
    text-align: center;
    padding: 0 4px;
  }
  .stepper .u {
    font-size: 12.5px;
    color: var(--fg-2);
    margin: 0 4px 0 2px;
  }
  .est {
    font-size: 12px;
    color: var(--muted);
  }
  .finish {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .finish :global(.seg) {
    flex: 1;
  }
  input[type="number"]::-webkit-inner-spin-button {
    -webkit-appearance: none;
  }
</style>
