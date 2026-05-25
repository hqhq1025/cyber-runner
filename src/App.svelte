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
  // 配速 = 从 desiredSpeed 派生（在 speed-card 里也用同一份，保证两处永远同步）。
  // Mock 阶段 backend status.paceText 没接真引擎；接进来后如果需要"运行时显示实际配速"
  // 再改成 isRunning ? status.paceText : targetPaceText 二选一。
  // statusBadgeText 依赖下面 isCyberRunning/isPreparing 派生，定义在它们之后

  // ===== 工具函数（与 Rust cyber_runner_core::{duration_text,...} 对齐）=====
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
  // 准备中态：从点 ▶ 到 python 真正开始改 GPS 的过渡阶段。
  // 后端 engine 报 Started 是 python 子进程"被 spawn"那一刻，不代表 tunnel 已建立。
  // 实际典型时间线（参考 main.py + tunnel.tunnel）：
  //   0s:    spawn python (engine: Started)
  //   ~1s:   init done
  //   1-12s: trying to start tunnel + 等 iPhone Developer Mode 通道
  //   12s±:  print "已开始模拟跑步" → GPS 实际开始更新
  // 这段时间按钮显示"准备中..."，过了再切到"已开始赛博跑步"。
  // 用户点停止 / 后端报错 → 立刻清掉。
  const REAL_START_DELAY_MS = 12_000;
  let preparingTimeoutId = $state<number | null>(null);
  let preparingDone = $state(false);

  function clearPreparing() {
    if (preparingTimeoutId !== null) {
      clearTimeout(preparingTimeoutId);
      preparingTimeoutId = null;
    }
    preparingDone = false;
  }

  function onStart() {
    clearPreparing();
    preparingDone = false;
    preparingTimeoutId = window.setTimeout(() => {
      preparingDone = true;
      preparingTimeoutId = null;
    }, REAL_START_DELAY_MS);
    void runStore.start();
  }
  function onStop() {
    clearPreparing();
    void runStore.stop();
  }

  // 后端报错 / 进程结束 → 复位准备中态
  $effect(() => {
    if (!isRunning) clearPreparing();
  });

  // 派生：真正在跑步（绝大多数 UI 用这个判断）
  const isCyberRunning = $derived(isRunning && preparingDone);
  const isPreparing = $derived((isBusy || isRunning) && !preparingDone);

  const statusBadgeText = $derived(
    isCyberRunning ? "运行中" : isPreparing ? "准备中…" : "未运行",
  );

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

  // Target pace derived from selected speed: 1000m / speed → sec/km
  // Returns "MM:SS" (no unit). 调用方需要 /km 后缀自己加。
  function formatPace(speedMps: number): string {
    if (speedMps <= 0) return "--:--";
    const secPerKm = 1000 / speedMps;
    const m = Math.floor(secPerKm / 60);
    const s = Math.round(secPerKm % 60);
    const pad = (n: number) => n.toString().padStart(2, "0");
    return `${pad(m)}:${pad(s)}`;
  }
  const targetPaceText = $derived(formatPace(runStore.desiredSpeed));
  const speedFillPercent = $derived(
    ((runStore.desiredSpeed - SPEED_MIN) / (SPEED_MAX - SPEED_MIN)) * 100,
  );

  // 常用配速预设（学校跑常见档位）
  const SPEED_PRESETS = [
    { label: "散步", value: 1.3 },
    { label: "慢跑", value: 3.0 },
    { label: "标准", value: 4.7 },
    { label: "冲刺", value: 6.5 },
  ];
</script>

<main class="app-root">
  <!-- ===== Header ===== -->
  <header class="header">
    <div class="brand">
      <span class="logo" aria-hidden="true">
        <svg viewBox="0 0 40 40" width="36" height="36">
          <defs>
            <linearGradient id="logo-plate" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stop-color="#0B3D2E" />
              <stop offset="100%" stop-color="#125C44" />
            </linearGradient>
            <linearGradient id="logo-track" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stop-color="#D26049" />
              <stop offset="100%" stop-color="#9C3F28" />
            </linearGradient>
            <linearGradient id="logo-grass" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stop-color="#4FA565" />
              <stop offset="100%" stop-color="#2B7142" />
            </linearGradient>
          </defs>
          <!-- plate -->
          <rect x="0" y="0" width="40" height="40" rx="9" ry="9" fill="url(#logo-plate)" />
          <!-- outer stadium (track surface) — 30×18.75 centered (20,20), hw=15 hh=9.4 -->
          <path d="M 14.4 10.6 L 25.6 10.6 A 9.4 9.4 0 0 1 25.6 29.4 L 14.4 29.4 A 9.4 9.4 0 0 1 14.4 10.6 Z"
                fill="url(#logo-track)" />
          <!-- inner stadium (grass) — track width ~5 → inner hw=10 hh=4.4 -->
          <path d="M 14.4 15.6 L 25.6 15.6 A 4.4 4.4 0 0 1 25.6 24.4 L 14.4 24.4 A 4.4 4.4 0 0 1 14.4 15.6 Z"
                fill="url(#logo-grass)" />
          <!-- track kerb lines -->
          <path d="M 14.4 10.6 L 25.6 10.6 A 9.4 9.4 0 0 1 25.6 29.4 L 14.4 29.4 A 9.4 9.4 0 0 1 14.4 10.6 Z"
                fill="none" stroke="#FFFFFF" stroke-opacity="0.55" stroke-width="0.4" />
          <path d="M 14.4 15.6 L 25.6 15.6 A 4.4 4.4 0 0 1 25.6 24.4 L 14.4 24.4 A 4.4 4.4 0 0 1 14.4 15.6 Z"
                fill="none" stroke="#FFFFFF" stroke-opacity="0.55" stroke-width="0.4" />
          <!-- route trace (70% CCW, at mid-track r=7) -->
          <path d="M 20 27 L 25.6 27 A 7 7 0 0 0 25.6 13 L 14.4 13 A 7 7 0 0 0 8.3 16.9"
                fill="none" stroke="#FFFFFF" stroke-width="1.15" stroke-linecap="round" />
          <!-- endpoint dot + halo -->
          <circle cx="8.3" cy="16.9" r="2.2" fill="#C8FF3D" fill-opacity="0.45" />
          <circle cx="8.3" cy="16.9" r="1.3" fill="#C8FF3D" />
        </svg>
      </span>
      <div class="brand-text">
        <h1 class="brand-title">Cyber Runner</h1>
        <p class="brand-subtitle">赛博校园跑 · iOS 设备定位模拟</p>
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
        <div class="metric-label">已运行</div>
        <div class="metric-value">{elapsedTimeText || "--:--"}</div>
      </article>
      <article class="metric-tile">
        <div class="metric-label">倒计时</div>
        <div class="metric-value">{remainingTimeText || "--:--"}</div>
      </article>
      <article class="metric-tile">
        <div class="metric-label">配速</div>
        <div class="metric-value">{targetPaceText}</div>
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
  </section>

  <!-- ===== 主区：左边速度+控制 / 右边日志 ===== -->
  <div class="main-grid">

    <!-- 速度卡片 —— 大数字 + 配速换算 + 填充 slider + 预设档位 -->
    <article class="speed-card">
      <div class="speed-card-header">
        <span class="speed-card-title">
          <span class="speed-icon" aria-hidden="true">⚡</span>
          速度
        </span>
        <span class="speed-hint">{isRunning ? "下次开始生效" : "开始时生效"}</span>
      </div>

      <div class="speed-main">
        <div class="speed-display">
          <span class="speed-value-big">{runStore.desiredSpeed.toFixed(1)}</span>
          <span class="speed-unit-stack">
            <span class="speed-unit">m/s</span>
            <span class="speed-pace">≈ {targetPaceText} /km</span>
          </span>
        </div>
        <div class="speed-stepper">
          <button
            type="button"
            class="stepper-btn"
            aria-label="减速"
            onclick={() => onSpeedStep(-SPEED_STEP)}
          >−</button>
          <button
            type="button"
            class="stepper-btn"
            aria-label="加速"
            onclick={() => onSpeedStep(SPEED_STEP)}
          >+</button>
        </div>
      </div>

      <input
        type="range"
        class="speed-slider"
        style="--fill: {speedFillPercent}%"
        min={SPEED_MIN}
        max={SPEED_MAX}
        step={SPEED_STEP}
        bind:value={runStore.desiredSpeed}
      />

      <div class="speed-presets">
        {#each SPEED_PRESETS as preset}
          <button
            type="button"
            class="preset-chip"
            class:active={Math.abs(runStore.desiredSpeed - preset.value) < 0.05}
            onclick={() => (runStore.desiredSpeed = preset.value)}
          >
            <span class="preset-label">{preset.label}</span>
            <span class="preset-num">{preset.value.toFixed(1)}</span>
          </button>
        {/each}
      </div>

      <!-- ===== 三按钮（嵌在 speed-card 底部，操作组连续）===== -->
      <section class="controls">
        <button
          type="button"
          class="btn btn-primary"
          class:cyber-running={isCyberRunning}
          disabled={isBusy || isRunning}
          onclick={onStart}
        >{isCyberRunning ? "✓ 已开始赛博跑步" : isPreparing ? "准备中…" : "▶ 开始"}</button>
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
        {#if isBusy || isPreparing}
          <span class="spinner" aria-label="处理中"></span>
        {/if}
      </section>
    </article>

  <!-- ===== 日志面板 ===== -->
  <section class="log-panel">
    <div class="log-head">
      <span class="log-title">日志</span>
      <span class="log-path">~/Library/Application Support/CyberRunner/run.log</span>
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
  </div><!-- /main-grid -->
</main>
