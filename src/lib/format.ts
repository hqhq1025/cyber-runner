export function paceText(mps: number): string {
  if (!(mps > 0) || !isFinite(mps)) return "--'--\"";
  const total = Math.round(1000 / mps);
  return `${Math.floor(total / 60)}'${String(total % 60).padStart(2, "0")}"`;
}

export function durationText(s: number): string {
  const t = Math.max(0, Math.floor(s));
  const h = Math.floor(t / 3600);
  const m = Math.floor((t % 3600) / 60);
  const sec = t % 60;
  const p = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${p(m)}:${p(sec)}` : `${p(m)}:${p(sec)}`;
}

export function km(m: number, digits = 2): string {
  return (m / 1000).toFixed(digits);
}

export function lengthText(m: number): string {
  return m >= 1000 ? `${(m / 1000).toFixed(2)} km` : `${Math.round(m)} m`;
}

export function clockText(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

export const DATUM_LABEL: Record<string, string> = {
  bd09: "百度 BD-09",
  gcj02: "高德 / 腾讯 GCJ-02",
  wgs84: "GPS WGS-84",
};

export function deviceName(d: { name: string | null; productType: string | null; udid: string }): string {
  return d.name ?? d.productType ?? `设备 ${d.udid.slice(0, 8)}`;
}
