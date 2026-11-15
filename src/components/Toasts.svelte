<script lang="ts">
  import { app } from "../lib/store.svelte";
</script>

<div class="toasts" role="region" aria-label="通知" aria-live="polite">
  {#each app.toasts as t (t.id)}
    <div class="toast {t.kind}">
      <span>{t.text}</span>
      <button class="x" aria-label="关闭通知" onclick={() => app.dismiss(t.id)}>×</button>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: 16px;
    bottom: 48px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 50;
    max-width: 380px;
  }
  .toast {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 12px;
    border-radius: 12px;
    background: var(--panel);
    border: 1px solid var(--line-strong);
    box-shadow: var(--shadow-lg);
    font-size: 13px;
    animation: in 0.18s ease-out;
  }
  .toast.error {
    border-color: color-mix(in srgb, var(--danger) 45%, transparent);
  }
  .toast.success {
    border-color: color-mix(in srgb, var(--accent) 45%, transparent);
  }
  .toast span {
    flex: 1;
  }
  .x {
    border: 0;
    background: none;
    font-size: 16px;
    line-height: 1;
    color: var(--muted);
    padding: 0 2px;
  }
  @keyframes in {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
  }
</style>
