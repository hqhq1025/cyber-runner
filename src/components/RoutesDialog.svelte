<script lang="ts">
  import { api } from "../lib/api";
  import { app } from "../lib/store.svelte";
  import { lengthText } from "../lib/format";

  let dialog: HTMLDialogElement | undefined = $state();
  let editing = $state<string | null>(null);
  let draft = $state("");
  let confirmId = $state<string | null>(null);

  export function open() {
    editing = null;
    confirmId = null;
    dialog?.showModal();
  }

  async function rename(id: string) {
    try {
      await api.renameRoute(id, draft);
      await app.reloadRoutes();
      editing = null;
    } catch (e) {
      app.toast("error", (e as Error).message);
    }
  }

  async function remove(id: string) {
    try {
      await api.deleteRoute(id);
      await app.reloadRoutes();
      if (app.settings.routeId === id) app.update({ routeId: app.routes[0]?.id ?? "" });
      confirmId = null;
    } catch (e) {
      app.toast("error", (e as Error).message);
    }
  }
</script>

<dialog class="modal" bind:this={dialog} aria-labelledby="routes-title" closedby="any">
  <div class="modal-head">
    <h2 id="routes-title">管理路线</h2>
    <button class="btn icon ghost" aria-label="关闭" onclick={() => dialog?.close()}>✕</button>
  </div>
  <div class="modal-body">
    <ul class="list">
      {#each app.routes as r (r.id)}
        <li>
          {#if editing === r.id}
            <input
              class="input grow"
              bind:value={draft}
              maxlength="40"
              aria-label="新名称"
              onkeydown={(e) => {
                if (e.key === "Enter") void rename(r.id);
                if (e.key === "Escape") {
                  e.preventDefault();
                  editing = null;
                }
              }}
            />
            <button class="btn sm primary" onclick={() => rename(r.id)}>保存</button>
            <button class="btn sm" onclick={() => (editing = null)}>取消</button>
          {:else}
            <div class="grow">
              <div class="name">{r.name}{#if r.builtin}<span class="tag">内置</span>{/if}</div>
              <div class="meta">{lengthText(r.autoClosed ? r.lapClosedM : r.lapOutAndBackM)}/圈 · {r.pointCount} 个点</div>
            </div>
            {#if !r.builtin}
              {#if confirmId === r.id}
                <span class="ask">删除？</span>
                <button class="btn sm danger" onclick={() => remove(r.id)}>删除</button>
                <button class="btn sm" onclick={() => (confirmId = null)}>保留</button>
              {:else}
                <button class="btn sm ghost" onclick={() => { editing = r.id; draft = r.name; }}>改名</button>
                <button class="btn sm ghost danger" onclick={() => (confirmId = r.id)}>删除</button>
              {/if}
            {/if}
          {/if}
        </li>
      {/each}
    </ul>
  </div>
</dialog>

<style>
  dialog {
    width: 520px;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
  }
  li {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 2px;
    border-bottom: 1px solid var(--line);
  }
  li:last-child {
    border-bottom: 0;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .name {
    font-weight: 550;
  }
  .tag {
    font-size: 11px;
    margin-left: 8px;
    padding: 1px 6px;
    border-radius: 6px;
    background: var(--panel-2);
    color: var(--muted);
    font-weight: 500;
  }
  .meta {
    font-size: 12px;
    color: var(--muted);
  }
  .ask {
    font-size: 12.5px;
    color: var(--danger);
  }
</style>
