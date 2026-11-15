<script lang="ts">
  // First-run checklist: tells the user exactly what is missing.
  import { app } from "../lib/store.svelte";
  import { isTauri } from "../lib/api";

  const isWin = typeof navigator !== "undefined" && /Windows/i.test(navigator.userAgent);

  type Step = { title: string; state: "ok" | "todo" | "bad" | "auto"; hint?: string };
  const steps = $derived.by((): Step[] => {
    const d = app.devices;
    const dev = app.device;
    const driver: Step = d.error
      ? {
          title: "苹果设备服务",
          state: "bad",
          hint: isWin
            ? "从微软商店安装「Apple Devices」（或 iTunes），装好后打开一次。"
            : d.error.hint,
        }
      : { title: "苹果设备服务", state: d.scanned ? "ok" : "todo" };
    const phone: Step = dev
      ? { title: "连接手机", state: "ok" }
      : { title: "连接手机", state: "todo", hint: "用能传数据的线连接 iPhone，并解锁屏幕。" };
    const trust: Step = !dev
      ? { title: "信任此电脑", state: "todo" }
      : dev.trusted
        ? { title: "信任此电脑", state: "ok" }
        : { title: "信任此电脑", state: "todo", hint: "点开始后手机会弹窗，点「信任」并输入锁屏密码。" };
    return [
      driver,
      phone,
      trust,
      { title: "开发者模式", state: "auto", hint: "iOS 17+ 需要开启：设置 → 隐私与安全性 → 开发者模式。" },
      { title: "开发者磁盘镜像", state: "auto", hint: "首次使用自动下载并挂载（约 17 MB）。" },
    ];
  });
  const allOk = $derived(steps.slice(0, 3).every((s) => s.state === "ok"));
</script>

<ol class="steps" aria-label="准备步骤">
  {#each steps as st, i (st.title)}
    {#if !allOk || i < 3}
      <li class={st.state}>
        <span class="mark" aria-hidden="true">
          {#if st.state === "ok"}✓{:else if st.state === "bad"}!{:else}{i + 1}{/if}
        </span>
        <span class="body">
          <span class="t">{st.title}{#if st.state === "auto"}<em>开始时检查</em>{/if}</span>
          {#if st.hint && st.state !== "ok" && (st.state !== "auto" || !allOk)}
            <span class="h">{st.hint}</span>
          {/if}
        </span>
      </li>
    {/if}
  {/each}
</ol>
{#if !isTauri()}
  <p class="demo">浏览器预览模式（演示数据）</p>
{/if}

<style>
  .steps {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  li {
    display: flex;
    gap: 10px;
    align-items: flex-start;
  }
  .mark {
    flex: none;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    font-size: 11px;
    font-weight: 700;
    background: var(--panel-2);
    border: 1px solid var(--line-strong);
    color: var(--muted);
  }
  .ok .mark {
    background: var(--accent);
    border-color: transparent;
    color: var(--accent-fg);
  }
  .bad .mark {
    background: var(--danger);
    border-color: transparent;
    color: #fff;
  }
  .body {
    display: flex;
    flex-direction: column;
  }
  .t {
    font-size: 13px;
    font-weight: 550;
  }
  .ok .t {
    color: var(--fg-2);
    font-weight: 450;
  }
  .t em {
    font-style: normal;
    font-size: 11px;
    color: var(--muted);
    margin-left: 6px;
  }
  .h {
    font-size: 12px;
    color: var(--fg-2);
  }
  .bad .h {
    color: var(--danger);
  }
  .demo {
    margin: 0;
    font-size: 11.5px;
    color: var(--warn);
  }
</style>
