<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { lengthText } from "../lib/format";
  import RouteMap from "./RouteMap.svelte";
  import Segmented from "./Segmented.svelte";
  import type { LoopMode } from "../lib/api";

  let { onimport, onmanage }: { onimport: () => void; onmanage: () => void } = $props();

  const route = $derived(app.selectedRoute);
  const st = $derived(app.settings);
  const s = $derived(app.snapshot);
  const locked = $derived(app.active);
  const closed = $derived(st.loopMode === "closed" || (st.loopMode === "auto" && !!route?.autoClosed));
  const lapLen = $derived(route ? (closed ? route.lapClosedM : route.lapOutAndBackM) : 0);
  const running = $derived(s.phase === "running");
</script>

<section class="card mapcard" aria-label="路线">
  <div class="bar">
    <label class="sr-only" for="route-select">路线</label>
    <select
      id="route-select"
      class="input"
      value={st.routeId}
      disabled={locked}
      title={locked ? "跑步中不能换路线" : undefined}
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
    <button class="btn" onclick={onimport} disabled={locked}>
      <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><path d="M12 5v14M5 12h14" /></svg>
      导入路线
    </button>
    <button class="btn ghost" onclick={onmanage} disabled={locked}>管理</button>
  </div>

  <div class="mapwrap">
    {#if route}
      <RouteMap
        points={route.preview}
        {closed}
        position={s.position}
        lapProgress={s.phase === "idle" ? 0 : s.lapProgress}
        {running}
        dim={s.phase === "paused"}
      />
    {/if}
  </div>

  <div class="foot">
    <div class="facts">
      {#if route}
        <span><b class="num">{lengthText(lapLen)}</b> / 圈</span>
        <span class="sep">·</span>
        <span>{route.pointCount} 个点</span>
        {#if s.phase !== "idle" && s.laps > 0}
          <span class="sep">·</span>
          <span>已完成 <b class="num">{s.laps}</b> 圈</span>
        {/if}
      {/if}
    </div>
    <div class="mode">
      <span class="mlabel">每圈</span>
      <Segmented
        size="sm"
        label="路线形态"
        value={st.loopMode}
        disabled={locked}
        options={[
          { value: "auto" as LoopMode, label: "自动", title: "首尾相距 60 米以内按绕圈，否则往返" },
          { value: "closed" as LoopMode, label: "绕圈", title: "跑到终点后直接回到起点" },
          { value: "outAndBack" as LoopMode, label: "往返", title: "跑到终点后原路返回" },
        ]}
        onchange={(v) => app.update({ loopMode: v })}
      />
    </div>
  </div>
</section>

<style>
  .mapcard {
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-height: 0;
  }
  .bar {
    display: flex;
    gap: 8px;
    padding: 12px;
    border-bottom: 1px solid var(--line);
  }
  .bar select {
    flex: 1;
    min-width: 0;
    font-weight: 600;
  }
  .mapwrap {
    flex: 1;
    min-height: 0;
  }
  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 12px;
    border-top: 1px solid var(--line);
    font-size: 12.5px;
    color: var(--fg-2);
  }
  .facts b {
    color: var(--fg);
  }
  .sep {
    margin: 0 6px;
    color: var(--muted);
  }
  .mode {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 250px;
  }
  .mode :global(.seg) {
    flex: 1;
  }
  .mlabel {
    color: var(--muted);
    white-space: nowrap;
  }
</style>
