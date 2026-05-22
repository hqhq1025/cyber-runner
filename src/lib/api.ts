// 与 Tauri Rust 端的 IPC 封装。
// 所有 `invoke` 调用都集中在这里，方便：
//   1) 类型化返回值
//   2) 错误统一转 string
//   3) 后续切换接口时只改一处

import { invoke } from "@tauri-apps/api/core";

/** 与 `src-tauri/src/state.rs::Status` 字段对齐（serde camelCase）。 */
export type Status = {
  isRunning: boolean;
  elapsedSeconds: number;
  remainingSeconds: number;
  speedMps: number;
  paceText: string;
  recentLog: string[];
  lastError: string | null;
  serverNowRfc3339: string;
};

/** 速度合法区间，UI Slider/Stepper 应该用这个。 */
export const SPEED_MIN = 0.5;
export const SPEED_MAX = 8.0;
export const SPEED_STEP = 0.1;

/** 是否在 Tauri 环境（dev 浏览器或 vite preview 时可能不是）。 */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function start(speed: number): Promise<string> {
  return await invoke<string>("start", { speed });
}

export async function stop(): Promise<string> {
  return await invoke<string>("stop");
}

export async function cleanup(): Promise<string> {
  return await invoke<string>("cleanup");
}

export async function fetchStatus(): Promise<Status> {
  return await invoke<Status>("status");
}
