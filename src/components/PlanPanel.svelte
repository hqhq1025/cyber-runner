<script lang="ts">
  // Treadmill-style controls: one knob for speed, one for how far.
  // Everything else lives under "更多设置".
  import { app } from "../lib/store.svelte";
  import { paceText } from "../lib/format";
  import Segmented from "./Segmented.svelte";
  import type { FinishAction, GoalKind, LoopMode, Realism } from "../lib/api";

  let { onimport, onmanage }: { onimport: () => void; onmanage: () => void } = $props();

  // Pace in seconds per km. The engine caps speed at 3'05"/km, so even with
  // realism wobble the pace never gets under 3'00" (campus apps void that).
  const FASTEST = 185;
  const SLOWEST = 600;
  const PACE_STEP = 5;
  // A little slower than the round numbers, as headroom for the wobble.
  const PACES = [
    { label: "5 分配", sec: 310 },
    { label: "4 分配", sec: 250 },
    { label: "3 分配", sec: 195 },
  ];
  const KMS = [3, 5, 8];

  const st = $derived(app.settings);
  const live = $derived(app.active && app.phase !== "stopping");
  const paceSec = $derived(Math.round(1000 / st.speedMps));
  const runKm = $derived(st.goalKm + st.goalMarginKm);

  function setPace(sec: number) {
    const s = Math.min(SLOWEST, Math.max(FASTEST, Math.round(sec)));
    app.update({ speedMps: 1000 / s });
  }
  function setKm(k: number) {
    app.update({ goalKm: Math.min(100, Math.max(0.5, Math.round(k * 2) / 2)) });
  }
  function setMinutes(m: number) {
    app.update({ goalMinutes: Math.min(600, Math.max(1, Math.round(m))) });
  }

  // Press-and-hold repeats, like treadmill buttons.
  let holdTimer: ReturnType<typeof setTimeout> | undefined;
  function hold(fn: () => void) {
    return {
      onpointerdown: (e: PointerEvent) => {
        (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
        fn();
        let delay = 380;
        const tick = () => {
          fn();
          delay = Math.max(70, delay * 0.8);
          holdTimer = setTimeout(tick, delay);
        };
        holdTimer = setTimeout(tick, delay);
      },
      onpointerup: () => clearTimeout(holdTimer),
      onpointercancel: () => clearTimeout(holdTimer),
      onkeydown: (e: KeyboardEvent) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          fn();
        }
      },
    };
  }
  const faster = hold(() => setPace(paceSec - PACE_STEP));
  const slower = hold(() => setPace(paceSec + PACE_STEP));
  const more = hold(() => (st.goal === "time" ? setMinutes(st.goalMinutes + 1) : setKm(st.goalKm + 0.5)));
  const less = hold(() => (st.goal === "time" ? setMinutes(st.goalMinutes - 1) : setKm(st.goalKm - 0.5)));

</script>

<section class="card panel" aria-labelledby="plan-title">
  <h2 class="sr-only" id="plan-title">跑步设置</h2>

  <div class="routebar">
    <label class="sr-only" for="route-select">路线</label>
    <select
      id="route-select"
      class="input"
      value={st.routeId}
      disabled={app.active}
      title={app.active ? "跑步中不能换路线" : undefined}
      onchange={(e) => app.update({ routeId: e.currentTarget.value })}
    >
      <optgroup label="内置路线">
        {#each app.routes.filter((r) => r.builtin) as r (r.id)}
          <option value={r.id}>{r.name}</option>
        {/each}
      </optgroup>
      {#if app.routes.some((r) => !r.builtin)}
        <optgroup label="我导入的">
          {#each app.routes.filter((r) => !r.builtin) as r (r.id)}
            <option value={r.id}>{r.name}</option>
          {/each}
        </optgroup>
      {/if}
    </select>
    <button class="btn icon" onclick={onimport} disabled={app.active} title="导入路线" aria-label="导入路线">
      <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><path d="M12 5v14M5 12h14" /></svg>
    </button>
  </div>

  <!-- Speed knob -->
  <div class="knob" role="group" aria-label="配速">
    <span class="klabel">配速{#if live}<em>实时生效</em>{/if}</span>
    <div class="kline">
      <button class="kbtn" aria-label="慢一点" title="慢 5 秒" {...slower}>−</button>
      <div class="kval">
        <span class="num">{paceText(st.speedMps)}</span><span class="ku">/km</span>
      </div>
      <button class="kbtn" aria-label="快一点" title="快 5 秒" {...faster} disabled={paceSec <= FASTEST}>+</button>
    </div>
    <div class="chips">
      {#each PACES as p (p.sec)}
        <button class="chip" class:on={Math.abs(paceSec - p.sec) < 1} onclick={() => setPace(p.sec)}>
{p.label}
        </button>
      {/each}
    </div>
  </div>

  <!-- Distance (or time) knob -->
  <div class="knob" role="group" aria-label="跑多远">
    <span class="klabel">{st.goal === "time" ? "跑多久" : "跑多远"}</span>
    {#if st.goal === "none"}
      <p class="khint">手动结束</p>
    {:else}
      <div class="kline">
        <button class="kbtn" aria-label="减少" {...less}>−</button>
        <div class="kval">
          {#if st.goal === "time"}
            <span class="num">{st.goalMinutes}</span><span class="ku">分钟</span>
          {:else}
            <span class="num">{st.goalKm}</span><span class="ku">km</span>
          {/if}
        </div>
        <button class="kbtn" aria-label="增加" {...more}>+</button>
      </div>
      {#if st.goal === "distance"}
        <div class="chips">
          {#each KMS as k (k)}
            <button class="chip" class:on={st.goalKm === k} onclick={() => setKm(k)}>
{k} km
            </button>
          {/each}
        </div>
        {#if st.goalMarginKm > 0}
          <p class="khint num" title="校园跑 App 计的距离可能偏少，多跑一点更稳">实际跑 {runKm} km，多 {st.goalMarginKm} km 防少记</p>
        {/if}
      {/if}
    {/if}
  </div>

  <details class="more">
    <summary>更多设置</summary>
    <div class="mbody">
      <div class="mrow">
        <span class="mlabel">目标方式</span>
        <Segmented
          size="sm"
          label="目标方式"
          value={st.goal}
          options={[
            { value: "distance" as GoalKind, label: "按距离" },
            { value: "time" as GoalKind, label: "按时间" },
            { value: "none" as GoalKind, label: "不限" },
          ]}
          onchange={(v) => app.update({ goal: v })}
        />
      </div>
      {#if st.goal === "distance"}
        <div class="mrow">
          <span class="mlabel" title="校园跑 App 计的距离可能偏少，多跑一点更稳">多跑余量</span>
          <Segmented
            size="sm"
            label="多跑余量"
            value={String(st.goalMarginKm)}
            options={[0, 0.5, 1, 2].map((m) => ({ value: String(m), label: m === 0 ? "不加" : `+${m}` }))}
            onchange={(v) => app.update({ goalMarginKm: Number(v) })}
          />
        </div>
      {/if}
      <div class="mrow">
        <span class="mlabel">跑完后</span>
        <Segmented
          size="sm"
          label="跑完后"
          value={st.onFinish}
          options={[
            { value: "hold" as FinishAction, label: "停在终点", title: "方便先在校园跑 App 里结束，再恢复定位" },
            { value: "restore" as FinishAction, label: "恢复定位" },
          ]}
          onchange={(v) => app.update({ onFinish: v })}
        />
      </div>
      <div class="mrow">
        <span class="mlabel" title="每次轨迹都不同，配速有自然起伏">拟真程度</span>
        <Segmented
          size="sm"
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
      </div>
      <div class="mrow">
        <span class="mlabel">路线形态</span>
        <Segmented
          size="sm"
          label="路线形态"
          value={st.loopMode}
          disabled={app.active}
          options={[
            { value: "auto" as LoopMode, label: "自动", title: "首尾相距 60 米以内按绕圈，否则往返" },
            { value: "closed" as LoopMode, label: "绕圈" },
            { value: "outAndBack" as LoopMode, label: "往返" },
          ]}
          onchange={(v) => app.update({ loopMode: v })}
        />
      </div>
      <div class="mrow">
        <span class="mlabel">我的路线</span>
        <button class="btn sm" onclick={onmanage} disabled={app.active}>管理导入的路线</button>
      </div>
    </div>
  </details>
</section>

<style>
  .panel {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .routebar {
    display: flex;
    gap: 6px;
  }
  .routebar select {
    flex: 1;
    min-width: 0;
    font-weight: 600;
  }
  .knob {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border-radius: 12px;
    background: var(--panel-2);
    border: 1px solid var(--line);
  }
  .klabel em {
    margin-left: 8px;
    font-style: normal;
    font-weight: 500;
    color: var(--accent);
  }
  .klabel {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--fg-2);
  }
  .kline {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .kval {
    flex: 1;
    text-align: center;
  }
  .kval .num {
    font-size: 34px;
    font-weight: 720;
    letter-spacing: -0.01em;
  }
  .ku {
    margin-left: 4px;
    font-size: 13px;
    font-weight: 600;
    color: var(--muted);
  }
  .kbtn {
    width: 48px;
    height: 48px;
    border-radius: 50%;
    border: 1px solid var(--line-strong);
    background: var(--panel);
    color: var(--fg);
    font-size: 24px;
    line-height: 1;
    display: grid;
    place-items: center;
    cursor: pointer;
    user-select: none;
    touch-action: none;
  }
  .kbtn:hover:not(:disabled) {
    border-color: var(--accent);
    color: var(--accent);
  }
  .kbtn:active:not(:disabled) {
    transform: scale(0.95);
    background: var(--accent-soft);
  }
  .kbtn:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .chips {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 6px;
  }
  .chip {
    display: flex;
    justify-content: center;
    align-items: center;
    height: 34px;
    border-radius: 9px;
    border: 1px solid var(--line);
    background: var(--panel);
    font-size: 13.5px;
    color: var(--fg-2);
    cursor: pointer;
  }
  .chip:hover {
    border-color: var(--line-strong);
  }
  .chip.on {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--fg);
    font-weight: 600;
  }
  .khint {
    margin: 0;
    font-size: 11.5px;
    color: var(--muted);
    text-align: center;
  }
  .more {
    border-top: 1px solid var(--line);
    padding-top: 10px;
  }
  .more summary {
    display: flex;
    align-items: baseline;
    gap: 8px;
    cursor: pointer;
    font-size: 13px;
    font-weight: 600;
    color: var(--fg-2);
    list-style: none;
  }
  .more summary::-webkit-details-marker {
    display: none;
  }
  .more summary::before {
    content: "›";
    display: inline-block;
    transition: transform 0.15s;
    color: var(--muted);
  }
  .more[open] summary::before {
    transform: rotate(90deg);
  }
  .mbody {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding-top: 12px;
  }
  .mrow {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .mrow :global(.seg) {
    flex: 1;
  }
  .mlabel {
    width: 60px;
    flex-shrink: 0;
    font-size: 12.5px;
    color: var(--fg-2);
  }
</style>
