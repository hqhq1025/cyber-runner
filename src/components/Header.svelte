<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { deviceName } from "../lib/format";

  let { onsettings }: { onsettings: () => void } = $props();

  const status = $derived.by(() => {
    const d = app.devices;
    if (!d.scanned) return { tone: "muted", text: "正在查找设备…", sub: "" };
    if (d.error) return { tone: "danger", text: "缺少苹果设备服务", sub: "点这里查看解决办法" };
    const dev = app.snapshot.device ?? app.device;
    if (!dev || d.devices.length === 0) return { tone: "warn", text: "未连接手机", sub: "用数据线连接 iPhone" };
    const extra = d.devices.length > 1 ? ` · 共 ${d.devices.length} 台` : "";
    if (!dev.trusted) return { tone: "warn", text: deviceName(dev), sub: `等待信任此电脑${extra}` };
    return {
      tone: "ok",
      text: deviceName(dev),
      sub: `iOS ${dev.iosVersion ?? "?"} · ${dev.connection === "usb" ? "USB" : "无线"}${extra}`,
    };
  });
</script>

<header class="header">
  <div class="brand">
    <svg class="logo" viewBox="0 0 40 40" width="30" height="30" aria-hidden="true">
      <rect width="40" height="40" rx="10" fill="#0f3d2a" />
      <path d="M14 11h12a9 9 0 0 1 0 18H14a9 9 0 0 1 0-18Z" fill="#d2603f" />
      <path d="M14 16h12a4 4 0 0 1 0 8H14a4 4 0 0 1 0-8Z" fill="#3f9a5b" />
      <circle cx="27.5" cy="12.5" r="2.6" fill="#c8ff3d" />
    </svg>
    <div>
      <div class="title">Cyber Runner</div>
      <div class="subtitle">iPhone 定位模拟 · 校园跑</div>
    </div>
  </div>

  <div class="right">
    <button class="device {status.tone}" onclick={onsettings} title="设备与设置">
      <span class="dot" aria-hidden="true"></span>
      <span class="dtext">
        <span class="dname">{status.text}</span>
        {#if status.sub}<span class="dsub">{status.sub}</span>{/if}
      </span>
    </button>
    <button class="btn icon ghost" onclick={onsettings} aria-label="设置">
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <circle cx="12" cy="12" r="3" />
        <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1Z" />
      </svg>
    </button>
  </div>
</header>

<style>
  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 18px;
    border-bottom: 1px solid var(--line);
    background: var(--panel);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .title {
    font-weight: 700;
    font-size: 15px;
    letter-spacing: -0.01em;
  }
  .subtitle {
    font-size: 12px;
    color: var(--muted);
  }
  .right {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .device {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 40px;
    padding: 0 14px 0 12px;
    border-radius: 12px;
    border: 1px solid var(--line);
    background: var(--panel-2);
    text-align: left;
  }
  .device:hover {
    border-color: var(--line-strong);
  }
  .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--muted);
    flex: none;
  }
  .ok .dot {
    background: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  .warn .dot {
    background: var(--warn);
    box-shadow: 0 0 0 3px var(--warn-soft);
  }
  .danger .dot {
    background: var(--danger);
    box-shadow: 0 0 0 3px var(--danger-soft);
  }
  .dtext {
    display: flex;
    flex-direction: column;
    line-height: 1.2;
  }
  .dname {
    font-weight: 600;
    font-size: 13px;
  }
  .dsub {
    font-size: 11.5px;
    color: var(--muted);
  }
</style>
