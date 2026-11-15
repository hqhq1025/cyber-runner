<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { clockText } from "../lib/format";

  let open = $state(false);
  let body: HTMLDivElement | undefined = $state();
  let stick = true;
  const last = $derived(app.logs[app.logs.length - 1]);

  $effect(() => {
    void app.logs.length;
    if (open && body && stick) queueMicrotask(() => body && (body.scrollTop = body.scrollHeight));
  });

  async function copy() {
    const text = app.logs
      .map((l) => `${clockText(l.ts_ms)} [${l.level}] ${l.message}`)
      .join("\n");
    try {
      await navigator.clipboard.writeText(`Cyber Runner ${app.version}\n${text}`);
      app.toast("success", "日志已复制");
    } catch {
      app.toast("error", "复制失败");
    }
  }
</script>

<footer class="logbar" class:open>
  <div class="head">
    <button class="toggle" aria-expanded={open} aria-controls="log-body" onclick={() => (open = !open)}>
      <svg class="chev" width="12" height="12" viewBox="0 0 24 24" aria-hidden="true"><path d="M6 15l6-6 6 6" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" /></svg>
      日志
    </button>
    {#if !open && last}
      <span class="latest {last.level}">{clockText(last.ts_ms)} · {last.message}</span>
    {/if}
    <span class="spacer"></span>
    {#if open}
      <button class="btn sm ghost" onclick={copy} disabled={app.logs.length === 0}>复制全部</button>
    {/if}
    <span class="ver">v{app.version}</span>
  </div>
  {#if open}
    <div
      id="log-body"
      class="body"
      bind:this={body}
      onscroll={(e) => {
        const el = e.currentTarget;
        stick = el.scrollHeight - el.scrollTop - el.clientHeight < 8;
      }}
    >
      {#each app.logs as l, i (i)}
        <div class="line {l.level}"><span class="t">{clockText(l.ts_ms)}</span>{l.message}</div>
      {:else}
        <div class="empty">还没有日志</div>
      {/each}
    </div>
  {/if}
</footer>

<style>
  .logbar {
    border-top: 1px solid var(--line);
    background: var(--panel);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 34px;
    padding: 0 12px;
    font-size: 12.5px;
  }
  .toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    border: 0;
    background: none;
    font-weight: 600;
    color: var(--fg-2);
    padding: 4px 6px;
    border-radius: 6px;
  }
  .toggle:hover {
    background: var(--panel-2);
  }
  .chev {
    transition: transform 0.15s;
  }
  .open .chev {
    transform: rotate(180deg);
  }
  .latest {
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .latest.warn {
    color: var(--warn);
  }
  .latest.error {
    color: var(--danger);
  }
  .spacer {
    flex: 1;
  }
  .ver {
    color: var(--muted);
    font-size: 11.5px;
  }
  .body {
    height: 200px;
    overflow: auto;
    padding: 4px 14px 10px;
    font-family: var(--mono);
    font-size: 12px;
    scrollbar-width: thin;
    user-select: text;
  }
  .line {
    padding: 1px 0;
    color: var(--fg-2);
    white-space: pre-wrap;
  }
  .line.warn {
    color: var(--warn);
  }
  .line.error {
    color: var(--danger);
  }
  .t {
    color: var(--muted);
    margin-right: 10px;
  }
  .empty {
    color: var(--muted);
    padding: 8px 0;
  }
</style>
