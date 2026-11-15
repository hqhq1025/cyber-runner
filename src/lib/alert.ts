// "You're done" alert: a short chime in the app, plus asking for permission
// to show the system notification the backend sends on finish.

import { isTauri } from "./api";

let ctx: AudioContext | null = null;

/** Three rising notes, ~1 s. Works without any audio file. */
export function chime() {
  try {
    ctx ??= new AudioContext();
    const t0 = ctx.currentTime + 0.05;
    [880, 1108.7, 1318.5].forEach((f, i) => {
      const o = ctx!.createOscillator();
      const g = ctx!.createGain();
      o.type = "sine";
      o.frequency.value = f;
      const t = t0 + i * 0.18;
      g.gain.setValueAtTime(0.0001, t);
      g.gain.exponentialRampToValueAtTime(0.25, t + 0.02);
      g.gain.exponentialRampToValueAtTime(0.0001, t + 0.6);
      o.connect(g).connect(ctx!.destination);
      o.start(t);
      o.stop(t + 0.65);
    });
  } catch {
    /* no audio device: the notification still shows */
  }
}

/** Ask once, early, so the finish notification isn't silently dropped. */
export async function ensureNotificationPermission() {
  if (!isTauri()) return;
  try {
    const n = await import("@tauri-apps/plugin-notification");
    if (!(await n.isPermissionGranted())) await n.requestPermission();
  } catch {
    /* ignore: in-app alert still works */
  }
}

/** Unlock audio on the first user gesture (WebKit starts it suspended). */
export function primeAudio() {
  try {
    ctx ??= new AudioContext();
    if (ctx.state === "suspended") void ctx.resume();
  } catch {
    /* ignore */
  }
}
