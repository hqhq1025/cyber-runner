// In-browser stand-in for the Rust backend (used only outside Tauri, for
// UI development). Simulates a phone that is plugged in and a run.

import type {
  Backend,
  Bootstrap,
  Datum,
  DeviceList,
  LogLine,
  RouteView,
  Settings,
  Snapshot,
} from "./api";

function oval(cLat: number, cLng: number, rx: number, ry: number, n = 80): [number, number][] {
  const mLat = 111_195;
  const mLng = mLat * Math.cos((cLat * Math.PI) / 180);
  const pts: [number, number][] = [];
  for (let i = 0; i <= n; i++) {
    const t = (i / n) * Math.PI * 2;
    // stadium-ish shape
    const x = rx * Math.sign(Math.cos(t)) * Math.min(1, Math.abs(Math.cos(t)) * 1.6);
    const y = ry * Math.sin(t);
    pts.push([cLat + y / mLat, cLng + x / mLng]);
  }
  return pts;
}

function lengthOf(p: [number, number][]): number {
  let d = 0;
  for (let i = 1; i < p.length; i++) {
    const dy = (p[i][0] - p[i - 1][0]) * 111_195;
    const dx = (p[i][1] - p[i - 1][1]) * 111_195 * Math.cos((p[i][0] * Math.PI) / 180);
    d += Math.hypot(dx, dy);
  }
  return d;
}

export function createMock(): Backend {
  const xtc = oval(39.9608, 116.3576, 95, 45);
  const sh = oval(40.1567, 116.2845, 110, 60);
  let routes: RouteView[] = [
    { id: "builtin:bupt-xitucheng", name: "北邮 · 西土城操场", builtin: true, sourceDatum: "bd09", lapClosedM: lengthOf(xtc), lapOutAndBackM: 2 * lengthOf(xtc), autoClosed: true, pointCount: xtc.length, endGapM: 0, preview: xtc },
    { id: "builtin:bupt-shahe", name: "北邮 · 沙河操场", builtin: true, sourceDatum: "bd09", lapClosedM: lengthOf(sh), lapOutAndBackM: 2 * lengthOf(sh), autoClosed: true, pointCount: sh.length, endGapM: 0, preview: sh },
  ];
  let settings: Settings = {
    routeId: "builtin:bupt-xitucheng",
    speedMps: 3.2,
    realism: "normal",
    loopMode: "auto",
    goal: "distance",
    goalKm: 3,
    goalMinutes: 20,
    onFinish: "hold",
    udid: null,
    ddiDir: null,
  };
  const idle: Snapshot = {
    phase: "idle", step: null, device: null, routeName: null, lapLengthM: 0, distanceM: 0, laps: 0,
    lapProgress: 0, movingS: 0, speedMps: 0, targetSpeedMps: 0, position: null,
    stopAfter: { durationS: null, distanceM: null }, onFinish: "hold", goalProgress: null, error: null, simulating: false,
  };
  let snap: Snapshot = { ...idle };
  let devices: DeviceList = { scanned: false, devices: [], error: null };
  const params = new URLSearchParams(location.search);
  const snapCbs: ((s: Snapshot) => void)[] = [];
  const devCbs: ((d: DeviceList) => void)[] = [];
  const logCbs: ((l: LogLine) => void)[] = [];
  const logs: LogLine[] = [];
  const log = (message: string, level: LogLine["level"] = "info") => {
    const l = { ts_ms: Date.now(), level, message };
    logs.push(l);
    logCbs.forEach((c) => c(l));
  };
  const emit = () => snapCbs.forEach((c) => c(structuredClone(snap)));
  setTimeout(() => {
    devices = params.has("nodevice")
      ? { scanned: true, devices: [], error: null }
      : {
          scanned: true,
          error: null,
          devices: [{ udid: "00008110-001A2B3C4D5E", connection: "usb", name: "小明的 iPhone", productType: "iPhone15,2", iosVersion: "18.2", trusted: true }],
        };
    devCbs.forEach((c) => c(devices));
  }, 400);

  let timer: number | undefined;
  let paused = false;
  const stopAfter = () =>
    settings.goal === "time"
      ? { durationS: settings.goalMinutes * 60, distanceM: null }
      : settings.goal === "distance"
        ? { durationS: null, distanceM: settings.goalKm * 1000 }
        : { durationS: null, distanceM: null };

  function posAt(r: RouteView, frac: number) {
    // Walk the polyline by length, like the real engine.
    const p = r.preview;
    let left = frac * lengthOf(p);
    for (let i = 1; i < p.length; i++) {
      const seg = lengthOf([p[i - 1], p[i]]);
      if (left <= seg || i === p.length - 1) {
        const t = seg > 0 ? Math.min(1, left / seg) : 0;
        return { lat: p[i - 1][0] + (p[i][0] - p[i - 1][0]) * t, lng: p[i - 1][1] + (p[i][1] - p[i - 1][1]) * t };
      }
      left -= seg;
    }
    return { lat: p[0][0], lng: p[0][1] };
  }

  // `?autostart` begins a demo run right away (used for screenshots).
  if (params.has("autostart")) setTimeout(() => void backend.start(), 700);

  const b: Bootstrap = { version: "0.2.0-mock", settings, routes, snapshot: snap, devices, logs };
  const backend: Backend = {
    async bootstrap() {
      return structuredClone({ ...b, settings, routes, snapshot: snap, devices, logs });
    },
    async saveSettings(s) {
      settings = { ...s };
      if (timer) {
        snap.targetSpeedMps = s.speedMps;
        snap.stopAfter = stopAfter();
        snap.onFinish = s.onFinish;
      }
      return settings;
    },
    async start() {
      if (params.has("fail")) {
        snap = { ...idle, phase: "connecting", step: "检查设备状态…" };
        emit();
        await new Promise((r) => setTimeout(r, 900));
        snap = { ...idle, error: { code: "developer_mode", message: "未开启开发者模式", hint: "打开 设置 → 隐私与安全性 → 开发者模式（已为你唤出这个开关），按提示重启并输入密码，然后再点开始。" } };
        log("未开启开发者模式", "error");
        emit();
        return;
      }
      const r = routes.find((x) => x.id === settings.routeId)!;
      snap = { ...idle, phase: "connecting", step: "查找设备…", routeName: r.name, lapLengthM: r.lapClosedM, targetSpeedMps: settings.speedMps, stopAfter: stopAfter(), onFinish: settings.onFinish };
      emit();
      const steps = ["检查设备状态…", "检查开发者磁盘镜像…", "建立定位通道…"];
      for (const s of steps) {
        await new Promise((res) => setTimeout(res, 450));
        snap.step = s;
        log(s);
        emit();
      }
      snap.device = devices.devices[0] ?? null;
      log("已连接 小明的 iPhone（iOS 18.2），用时 1.4 秒");
      paused = false;
      const speedup = Number(params.get("speedup") ?? 20);
      let last = performance.now();
      timer = window.setInterval(() => {
        const now = performance.now();
        const dt = ((now - last) / 1000) * speedup;
        last = now;
        const finished = snap.phase === "finished";
        if (!paused && !finished) {
          const v = snap.targetSpeedMps * (1 + 0.03 * Math.sin(now / 900));
          snap.distanceM += v * dt;
          snap.movingS += dt;
          snap.speedMps = v;
        } else snap.speedMps = 0;
        snap.laps = Math.floor(snap.distanceM / r.lapClosedM);
        snap.lapProgress = (snap.distanceM % r.lapClosedM) / r.lapClosedM;
        snap.position = posAt(r, snap.lapProgress);
        const sa = snap.stopAfter;
        const g = sa.distanceM ? snap.distanceM / sa.distanceM : sa.durationS ? snap.movingS / sa.durationS : null;
        snap.goalProgress = g == null ? null : Math.min(1, g);
        snap.phase = g != null && g >= 1 ? "finished" : paused ? "paused" : "running";
        snap.simulating = true;
        snap.step = null;
        emit();
      }, 250);
    },
    async pause() { paused = true; },
    async resume() { paused = false; if (snap.phase === "finished") { snap.stopAfter = { durationS: null, distanceM: null }; } },
    async stop() {
      clearInterval(timer);
      timer = undefined;
      snap = { ...snap, phase: "stopping", step: "正在恢复真实定位…" };
      emit();
      await new Promise((r) => setTimeout(r, 500));
      log("已恢复真实定位");
      snap = { ...snap, phase: "idle", step: null, simulating: false, speedMps: 0 };
      emit();
    },
    async restoreLocation() { await new Promise((r) => setTimeout(r, 600)); log("已恢复真实定位"); },
    async listRoutes() { return routes; },
    async readRouteFile(path) { return { text: "116.3600,39.9600\n116.3620,39.9600\n116.3620,39.9612\n116.3600,39.9612\n116.3600,39.9601", suggestedName: path.split("/").pop()!.replace(/\.\w+$/, ""), detectedDatum: null }; },
    async previewRoute(text: string, _d: Datum) {
      const pts = text.trim().split(/\n/).map((l) => l.split(/[,\s]+/).map(Number)).filter((a) => a.length >= 2 && !a.some(isNaN)).map(([a, b]) => [b, a] as [number, number]);
      if (pts.length < 2) throw new Error("有效点太少（需要至少 2 个不重合的点）");
      return { id: "preview", name: "预览", builtin: false, sourceDatum: _d, lapClosedM: lengthOf(pts), lapOutAndBackM: 2 * lengthOf(pts), autoClosed: true, pointCount: pts.length, endGapM: 5, preview: pts };
    },
    async createRoute(name, text, datum) {
      const v = await this.previewRoute(text, datum);
      const r = { ...v, id: `user:${Date.now()}`, name: name || "导入的路线" };
      routes = [...routes, r];
      return r;
    },
    async renameRoute(id, name) { routes = routes.map((r) => (r.id === id ? { ...r, name } : r)); },
    async deleteRoute(id) { routes = routes.filter((r) => r.id !== id); },
    async pickRouteFile() { return "/Users/me/Downloads/操场.gpx"; },
    async pickFolder() { return "/Users/me/DDI"; },
    async onSnapshot(cb) { snapCbs.push(cb); return () => {}; },
    async onDevices(cb) { devCbs.push(cb); return () => {}; },
    async onLog(cb) { logCbs.push(cb); return () => {}; },
    async onFileDrop() { return () => {}; },
  };
  return backend;
}
