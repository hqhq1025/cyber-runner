// Typed bridge to the Rust backend. Everything the UI calls goes through
// here. Outside Tauri (plain `npm run dev` in a browser) a small in-memory
// mock is used, so the UI can be developed and reviewed without a phone.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { createMock } from "./mock";

export type Datum = "wgs84" | "gcj02" | "bd09";
export type LoopMode = "auto" | "closed" | "outAndBack";
export type Realism = "off" | "low" | "normal" | "high";
export type GoalKind = "none" | "time" | "distance";
export type FinishAction = "hold" | "restore";
export type Phase =
  | "idle"
  | "connecting"
  | "running"
  | "paused"
  | "reconnecting"
  | "finished"
  | "stopping";

export interface LatLng {
  lat: number;
  lng: number;
}

export interface Settings {
  routeId: string;
  speedMps: number;
  realism: Realism;
  loopMode: LoopMode;
  goal: GoalKind;
  goalMinutes: number;
  goalKm: number;
  onFinish: FinishAction;
  udid: string | null;
  ddiDir: string | null;
}

export interface RouteView {
  id: string;
  name: string;
  builtin: boolean;
  sourceDatum: Datum;
  /** One lap (meters) after cleanup, per loop mode. */
  lapClosedM: number;
  lapOutAndBackM: number;
  autoClosed: boolean;
  pointCount: number;
  endGapM: number;
  /** Cleaned line, [lat, lng] pairs */
  preview: [number, number][];
}

export interface RouteDraft {
  text: string;
  suggestedName: string;
  detectedDatum: Datum | null;
}

export interface ErrorInfo {
  code: string;
  message: string;
  hint: string;
}

export interface DeviceInfo {
  udid: string;
  connection: "usb" | "network";
  name: string | null;
  productType: string | null;
  iosVersion: string | null;
  trusted: boolean;
}

export interface DeviceList {
  scanned: boolean;
  devices: DeviceInfo[];
  error: ErrorInfo | null;
}

export interface StopAfter {
  durationS: number | null;
  distanceM: number | null;
}

export interface Snapshot {
  phase: Phase;
  step: string | null;
  device: DeviceInfo | null;
  routeName: string | null;
  lapLengthM: number;
  distanceM: number;
  laps: number;
  lapProgress: number;
  movingS: number;
  speedMps: number;
  targetSpeedMps: number;
  position: LatLng | null;
  stopAfter: StopAfter;
  onFinish: FinishAction;
  goalProgress: number | null;
  error: ErrorInfo | null;
  simulating: boolean;
}

export interface LogLine {
  ts_ms: number;
  level: "info" | "warn" | "error";
  message: string;
}

export interface Bootstrap {
  version: string;
  settings: Settings;
  routes: RouteView[];
  snapshot: Snapshot;
  devices: DeviceList;
  logs: LogLine[];
}

export interface Backend {
  bootstrap(): Promise<Bootstrap>;
  saveSettings(settings: Settings): Promise<Settings>;
  start(): Promise<void>;
  pause(): Promise<void>;
  resume(): Promise<void>;
  stop(): Promise<void>;
  restoreLocation(): Promise<void>;
  listRoutes(): Promise<RouteView[]>;
  readRouteFile(path: string): Promise<RouteDraft>;
  previewRoute(text: string, datum: Datum): Promise<RouteView>;
  createRoute(name: string, text: string, datum: Datum): Promise<RouteView>;
  renameRoute(id: string, name: string): Promise<void>;
  deleteRoute(id: string): Promise<void>;
  pickRouteFile(): Promise<string | null>;
  pickFolder(): Promise<string | null>;
  onSnapshot(cb: (s: Snapshot) => void): Promise<UnlistenFn>;
  onDevices(cb: (d: DeviceList) => void): Promise<UnlistenFn>;
  onLog(cb: (l: LogLine) => void): Promise<UnlistenFn>;
  onFileDrop(cb: (paths: string[]) => void, onHover: (over: boolean) => void): Promise<UnlistenFn>;
}

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function errText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw new Error(errText(e));
  }
}

const tauriBackend: Backend = {
  bootstrap: () => call("bootstrap"),
  saveSettings: (settings) => call("save_settings", { settings }),
  start: () => call("start"),
  pause: () => call("pause"),
  resume: () => call("resume"),
  stop: () => call("stop"),
  restoreLocation: () => call("restore_location"),
  listRoutes: () => call("list_routes"),
  readRouteFile: (path) => call("read_route_file", { path }),
  previewRoute: (text, datum) => call("preview_route", { text, datum }),
  createRoute: (name, text, datum) => call("create_route", { name, text, datum }),
  renameRoute: (id, name) => call("rename_route", { id, name }),
  deleteRoute: (id) => call("delete_route", { id }),
  async pickRouteFile() {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const r = await open({
      multiple: false,
      directory: false,
      title: "选择路线文件",
      filters: [
        { name: "路线", extensions: ["gpx", "kml", "geojson", "json", "txt", "csv"] },
        { name: "所有文件", extensions: ["*"] },
      ],
    });
    return typeof r === "string" ? r : null;
  },
  async pickFolder() {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const r = await open({ directory: true, multiple: false, title: "选择开发者磁盘镜像文件夹" });
    return typeof r === "string" ? r : null;
  },
  onSnapshot: (cb) => listen<Snapshot>("snapshot", (e) => cb(e.payload)),
  onDevices: (cb) => listen<DeviceList>("devices", (e) => cb(e.payload)),
  onLog: (cb) => listen<LogLine>("log", (e) => cb(e.payload)),
  async onFileDrop(cb, onHover) {
    const { getCurrentWebview } = await import("@tauri-apps/api/webview");
    return getCurrentWebview().onDragDropEvent((e) => {
      const p = e.payload;
      if (p.type === "enter" || p.type === "over") onHover(true);
      else if (p.type === "leave") onHover(false);
      else if (p.type === "drop") {
        onHover(false);
        cb(p.paths);
      }
    });
  },
};

export const api: Backend = isTauri() ? tauriBackend : createMock();
