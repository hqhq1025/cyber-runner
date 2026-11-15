<script lang="ts">
  // One-line "what's missing" prompt: only the first unmet step.
  // Developer mode and the disk image are checked (and explained) when a run starts.
  import { app } from "../lib/store.svelte";
  import { isTauri } from "../lib/api";

  const isWin = typeof navigator !== "undefined" && /Windows/i.test(navigator.userAgent);

  const need = $derived.by(() => {
    const d = app.devices;
    if (d.error) {
      return {
        bad: true,
        title: "缺少苹果设备服务",
        hint: isWin ? "从微软商店安装「Apple Devices」（或 iTunes），装好后打开一次。" : d.error.hint,
      };
    }
    if (!app.device) {
      return { bad: false, title: "连接手机", hint: "用能传数据的线连上 iPhone，并解锁屏幕。" };
    }
    return null;
  });
</script>

{#if need}
  <div class="need" class:bad={need.bad} role="status">
    <strong>{need.title}</strong>
    <span>{need.hint}</span>
  </div>
{/if}
{#if !isTauri()}
  <p class="demo">浏览器预览模式（演示数据）</p>
{/if}

<style>
  .need {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--warn-soft);
    border: 1px solid color-mix(in srgb, var(--warn) 30%, transparent);
    font-size: 13px;
    color: var(--fg-2);
  }
  .need strong {
    font-size: 14px;
    color: var(--warn);
  }
  .need.bad {
    background: var(--danger-soft);
    border-color: color-mix(in srgb, var(--danger) 30%, transparent);
  }
  .need.bad strong {
    color: var(--danger);
  }
  .demo {
    margin: 0;
    font-size: 11.5px;
    color: var(--warn);
  }
</style>
