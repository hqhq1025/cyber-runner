<script lang="ts">
  // 1:1 移植自 Sources/RunControl/ContentView.swift 的布局：
  // header(logo + title + 状态徽章) / 4 个 MetricTile + 倒计时控制 / 速度行 / 三按钮 / 日志面板
  //
  // Svelte 5 runes — `$state` 局部状态、`$derived` 派生、`$effect` 生命周期。
  // 全局轮询走 stores.svelte.ts 里的 RunStore 单例。

  import { onMount } from "svelte";
  import { runStore } from "./lib/stores.svelte";
  import { SPEED_MAX, SPEED_MIN, SPEED_STEP, isTauri } from "./lib/api";

  // 启动时挂上 1s 轮询；组件销毁时清掉。
  onMount(() => runStore.startPolling(1000));

  // 现在时间 —— 用 server_now 显示，但 server 是每秒拉一次；
  // 为了"现在"那块儿走秒级动画，本地 tick 也每秒走一次。
  let localNow = $state(new Date());
  $effect(() => {
    const id = setInterval(() => (localNow = new Date()), 1000);
    return () => clearInterval(id);
  });

  // ===== Derived (从 store + tick 计算 UI 文案) =====
  const status = $derived(runStore.status);
  const isRunning = $derived(status.isRunning);
  const isBusy = $derived(runStore.busy);

  const currentTimeText = $derived(formatClock(localNow));
  const elapsedTimeText = $derived(formatDuration(status.elapsedSeconds));
  const remainingTimeText = $derived(formatDuration(status.remainingSeconds));
  const paceText = $derived(status.paceText);

  const statusBadgeText = $derived(isRunning ? "运行中" : "未运行");

  // ===== 工具函数（与 Rust fake_run_core::{duration_text,...} 对齐）=====
  function formatClock(d: Date): string {
    const hh = d.getHours().toString().padStart(2, "0");
    const mm = d.getMinutes().toString().padStart(2, "0");
    const ss = d.getSeconds().toString().padStart(2, "0");
    return `${hh}:${mm}:${ss}`;
  }

  function formatDuration(totalSeconds: number): string {
    const s = Math.max(0, Math.floor(totalSeconds));
    const h = Math.floor(s / 3600);
    const m = Math.floor((s % 3600) / 60);
    const sec = s % 60;
    const pad = (n: number) => n.toString().padStart(2, "0");
    return h > 0 ? `${pad(h)}:${pad(m)}:${pad(sec)}` : `${pad(m)}:${pad(sec)}`;
  }

  // ===== 事件处理 =====
  function onStart() {
    void runStore.start();
  }
  function onStop() {
    void runStore.stop();
  }
  function onRefresh() {
    void runStore.refresh();
  }
  function onResetCountdown() {
    // Mock 阶段后端没有 "set countdown" 命令；UI 仅做视觉重置。
    // 真接引擎时再加 set_countdown command。
    runStore.countdownMinutes = runStore.countdownMinutes;
  }

  function onCountdownChange(delta: number) {
    const next = Math.max(1, Math.min(240, runStore.countdownMinutes + delta));
    runStore.countdownMinutes = next;
  }

  function onSpeedStep(delta: number) {
    const next = Math.round((runStore.desiredSpeed + delta) * 10) / 10;
    runStore.desiredSpeed = Math.max(SPEED_MIN, Math.min(SPEED_MAX, next));
  }
</script>

<main class="app-root">
  <!-- ===== Header ===== -->
  <header class="header">
    <div class="brand">
      <span class="logo" aria-hidden="true">
        <svg viewBox="0 0 40 40" width="36" height="36">
          <defs>
            <linearGradient id="logo-grad" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stop-color="#34c75e" />
              <stop offset="100%" stop-color="#0a6e4c" />
            </linearGradient>
          </defs>
          <!-- rounded square base —— Apple app icon proportions (radius ~22% of side) -->
          <rect x="0" y="0" width="40" height="40" rx="9" ry="9" fill="url(#logo-grad)" />
          <!-- GPS lock ring (top right) -->
          <circle cx="29.5" cy="11" r="3.6" fill="none"
                  stroke="#fffceb" stroke-opacity="0.55" stroke-width="1" />
          <!-- check / forward-stride hybrid —— low-left dip then long up-right extension -->
          <path
            d="M 9 25 L 15 31 L 29.5 11"
            fill="none"
            stroke="#ffffff"
            stroke-width="3.2"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
          <!-- GPS lock dot at apex -->
          <circle cx="29.5" cy="11" r="2" fill="#fffceb" />
        </svg>
      </span>
      <div class="brand-text">
        <h1 class="brand-title">fake run</h1>
        <p class="brand-subtitle">西土城路线 · iOS 设备定位测试</p>
      </div>
    </div>

    <span class="status-badge" class:running={isRunning}>
      <span class="status-dot" aria-hidden="true"></span>
      {statusBadgeText}
    </span>
  </header>

  <!-- ===== 4 个 MetricTile + 倒计时控制 ===== -->
  <section class="time-panel">
    <div class="tiles-row">
      <article class="metric-tile">
        <div class="metric-label">现在</div>
        <div class="metric-value">{currentTimeText}</div>
      </article>
      <article class="metric-tile">
        <div class="metric-label">已运行</div>
        <div class="metric-value">{elapsedTimeText || "--:--"}</div>
      </article>
      <article class="metric-tile">
        <div class="metric-label">倒计时</div>
        <div class="metric-value">{remainingTimeText || "--:--"}</div>
      </article>
      <article class="metric-tile">
        <div class="metric-label">配速</div>
        <div class="metric-value">{paceText}</div>
      </article>

      <article class="countdown-tile">
        <div class="countdown-row">
          <span class="countdown-label">提醒</span>
          <span class="countdown-value">{runStore.countdownMinutes} 分钟</span>
        </div>
        <div class="stepper">
          <button
            type="button"
            class="stepper-btn"
            aria-label="减少倒计时"
            onclick={() => onCountdownChange(-5)}
          >−</button>
          <button
            type="button"
            class="stepper-btn"
            aria-label="增加倒计时"
            onclick={() => onCountdownChange(5)}
          >+</button>
        </div>
        <button
          type="button"
          class="link-btn"
          disabled={!isRunning}
          onclick={onResetCountdown}
        >↺ 重置倒计时</button>
      </article>
    </div>

    <!-- 速度行 -->
    <div class="speed-row">
      <span class="speed-label">⚡ 速度</span>
      <input
        type="range"
        class="speed-slider"
        min={SPEED_MIN}
        max={SPEED_MAX}
        step={SPEED_STEP}
        bind:value={runStore.desiredSpeed}
      />
      <div class="speed-stepper">
        <button
          type="button"
          class="stepper-btn"
          aria-label="减速"
          onclick={() => onSpeedStep(-SPEED_STEP)}
        >−</button>
        <span class="speed-value">{runStore.desiredSpeed.toFixed(1)} m/s</span>
        <button
          type="button"
          class="stepper-btn"
          aria-label="加速"
          onclick={() => onSpeedStep(SPEED_STEP)}
        >+</button>
      </div>
      <span class="speed-hint">{isRunning ? "下次开始生效" : "开始时生效"}</span>
    </div>
  </section>

  <!-- ===== 三按钮 ===== -->
  <section class="controls">
    <button
      type="button"
      class="btn btn-primary"
      disabled={isBusy || isRunning}
      onclick={onStart}
    >▶ 开始</button>
    <button
      type="button"
      class="btn"
      disabled={isBusy || !isRunning}
      onclick={onStop}
    >■ 停止</button>
    <button
      type="button"
      class="btn"
      disabled={isBusy}
      onclick={onRefresh}
    >↻ 刷新</button>
    {#if isBusy}
      <span class="spinner" aria-label="处理中"></span>
    {/if}
  </section>

  <!-- ===== 日志面板 ===== -->
  <section class="log-panel">
    <div class="log-head">
      <span class="log-title">日志</span>
      <span class="log-path">mock://state/in-memory</span>
    </div>
    {#if status.lastError}
      <div class="error-line">{status.lastError}</div>
    {/if}
    {#if runStore.frontendError && isTauri()}
      <div class="error-line">前端: {runStore.frontendError}</div>
    {/if}
    <div class="log-body">
      {#if status.recentLog.length === 0}
        <div class="log-empty">（尚无日志，点 ▶ 开始 试试）</div>
      {:else}
        {#each status.recentLog as line, i (i)}
          <div class="log-line">{line}</div>
        {/each}
      {/if}
    </div>
  </section>
</main>
