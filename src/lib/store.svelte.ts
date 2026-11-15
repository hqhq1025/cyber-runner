// Single app store (Svelte 5 runes). The backend pushes snapshots, device
// lists and log lines; settings are edited locally and saved debounced.

import {
  api,
  type DeviceList,
  type LogLine,
  type RouteView,
  type Settings,
  type Snapshot,
} from "./api";
import { chime, ensureNotificationPermission } from "./alert";

export type Toast = { id: number; kind: "info" | "error" | "success"; text: string };

const LOG_LIMIT = 400;

class AppStore {
  ready = $state(false);
  version = $state("");
  settings = $state<Settings>(null as unknown as Settings);
  routes = $state<RouteView[]>([]);
  snapshot = $state<Snapshot>(null as unknown as Snapshot);
  devices = $state<DeviceList>({ scanned: false, devices: [], error: null });
  logs = $state<LogLine[]>([]);
  toasts = $state<Toast[]>([]);
  /** A command is in flight (start / stop / restore). */
  busy = $state(false);
  dropHover = $state(false);
  /** Set by drag & drop; the import dialog picks it up. */
  pendingImportPath = $state<string | null>(null);

  #saveTimer: ReturnType<typeof setTimeout> | undefined;
  #toastId = 0;

  get phase() {
    return this.snapshot?.phase ?? "idle";
  }
  get active() {
    return this.phase !== "idle";
  }
  get selectedRoute(): RouteView | undefined {
    return this.routes.find((r) => r.id === this.settings?.routeId) ?? this.routes[0];
  }
  get device() {
    const list = this.devices.devices;
    return list.find((d) => d.udid === this.settings?.udid) ?? list[0] ?? null;
  }

  async init() {
    const b = await api.bootstrap();
    this.version = b.version;
    this.settings = b.settings;
    this.routes = b.routes;
    this.snapshot = b.snapshot;
    this.devices = b.devices;
    this.logs = b.logs;
    if (!this.routes.some((r) => r.id === this.settings.routeId) && this.routes[0]) {
      this.settings.routeId = this.routes[0].id;
    }
    await api.onSnapshot((s) => {
      const prev = this.snapshot?.phase;
      this.snapshot = s;
      if (prev && prev !== "idle" && s.phase === "idle" && s.error) {
        this.toast("error", s.error.message);
      }
      if (prev && prev !== "finished" && s.phase === "finished") {
        chime();
        this.toast("success", `跑完了：${(s.distanceM / 1000).toFixed(2)} km`);
      }
    });
    await api.onDevices((d) => (this.devices = d));
    await api.onLog((l) => {
      this.logs.push(l);
      if (this.logs.length > LOG_LIMIT) this.logs.splice(0, this.logs.length - LOG_LIMIT);
    });
    await api.onFileDrop(
      (paths) => {
        if (paths[0]) this.pendingImportPath = paths[0];
      },
      (over) => (this.dropHover = over),
    );
    this.ready = true;
    void ensureNotificationPermission();
  }

  /** Apply a settings change; persisted (and pushed live) after 250 ms. */
  update(patch: Partial<Settings>) {
    Object.assign(this.settings, patch);
    clearTimeout(this.#saveTimer);
    this.#saveTimer = setTimeout(() => void this.flush(), 250);
  }

  async flush() {
    clearTimeout(this.#saveTimer);
    try {
      await api.saveSettings($state.snapshot(this.settings));
    } catch (e) {
      this.toast("error", String((e as Error).message ?? e));
    }
  }

  async #run(fn: () => Promise<void>, ok?: string) {
    if (this.busy) return;
    this.busy = true;
    try {
      await fn();
      if (ok) this.toast("success", ok);
    } catch (e) {
      this.toast("error", (e as Error).message ?? String(e));
    } finally {
      this.busy = false;
    }
  }

  start() {
    return this.#run(async () => {
      await this.flush();
      await api.start();
    });
  }
  pause() {
    return api.pause();
  }
  resume() {
    return api.resume();
  }
  stop() {
    return this.#run(() => api.stop());
  }
  restore() {
    return this.#run(() => api.restoreLocation(), "已恢复真实定位");
  }

  async reloadRoutes() {
    this.routes = await api.listRoutes();
  }

  toast(kind: Toast["kind"], text: string) {
    const id = ++this.#toastId;
    this.toasts.push({ id, kind, text });
    setTimeout(() => this.dismiss(id), kind === "error" ? 8000 : 4000);
  }
  dismiss(id: number) {
    const i = this.toasts.findIndex((t) => t.id === id);
    if (i >= 0) this.toasts.splice(i, 1);
  }
}

export const app = new AppStore();
