// 全局 runes-based store —— 每 1s 轮询 `status()`，并暴露便捷的命令封装。
// 用 class + `$state` 是 Svelte 5 推荐的可重用 store 写法（替代旧 writable）。

import * as api from "./api";

export type { Status } from "./api";

class RunStore {
  // 当前状态快照；初始用一个 placeholder 避免组件首次渲染时 null check 泛滥。
  status = $state<api.Status>({
    isRunning: false,
    elapsedSeconds: 0,
    remainingSeconds: 30 * 60,
    speedMps: 4.7,
    paceText: "--:-- /km",
    recentLog: [],
    lastError: null,
    serverNowRfc3339: new Date().toISOString(),
  });

  /** 当前是否有命令在飞行中（用于禁用按钮）。 */
  busy = $state(false);
  /** 最近一次前端侧错误（network / IPC 抛出的），与 backend `lastError` 不同。 */
  frontendError = $state<string | null>(null);
  /** UI 侧的速度选择（输入控件直接绑这个，避免 round-trip 卡顿）。 */
  desiredSpeed = $state(4.7);
  /** UI 侧的倒计时偏好（mock 暂不送到后端，只是显示）。 */
  countdownMinutes = $state(30);

  /** 立即拉一次最新状态。 */
  async refresh(): Promise<void> {
    try {
      const next = await api.fetchStatus();
      this.status = next;
      this.frontendError = null;
    } catch (err) {
      this.frontendError = String(err);
    }
  }

  async start(): Promise<void> {
    if (this.busy) return;
    this.busy = true;
    try {
      await api.start(this.desiredSpeed);
      await this.refresh();
    } catch (err) {
      this.frontendError = String(err);
    } finally {
      this.busy = false;
    }
  }

  async stop(): Promise<void> {
    if (this.busy) return;
    this.busy = true;
    try {
      await api.stop();
      await this.refresh();
    } catch (err) {
      this.frontendError = String(err);
    } finally {
      this.busy = false;
    }
  }

  async cleanup(): Promise<void> {
    if (this.busy) return;
    this.busy = true;
    try {
      await api.cleanup();
      await this.refresh();
    } catch (err) {
      this.frontendError = String(err);
    } finally {
      this.busy = false;
    }
  }

  /** 启动 1s 轮询；返回清理函数，配合 `$effect` 使用。 */
  startPolling(intervalMs = 1000): () => void {
    // 立即跑一次再开始 interval，UI 不用等 1s 才有数据
    void this.refresh();
    const id = setInterval(() => {
      void this.refresh();
    }, intervalMs);
    return () => clearInterval(id);
  }
}

/** 单例 store，组件直接 import 用。 */
export const runStore = new RunStore();
