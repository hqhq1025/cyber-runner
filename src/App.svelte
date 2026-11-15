<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "./lib/store.svelte";
  import Header from "./components/Header.svelte";
  import MapCard from "./components/MapCard.svelte";
  import RunPanel from "./components/RunPanel.svelte";
  import PlanPanel from "./components/PlanPanel.svelte";
  import LogBar from "./components/LogBar.svelte";
  import Toasts from "./components/Toasts.svelte";
  import ImportDialog from "./components/ImportDialog.svelte";
  import RoutesDialog from "./components/RoutesDialog.svelte";
  import SettingsDialog from "./components/SettingsDialog.svelte";

  let importDlg: ImportDialog | undefined = $state();
  let routesDlg: RoutesDialog | undefined = $state();
  let settingsDlg: SettingsDialog | undefined = $state();
  let fatal = $state<string | null>(null);

  onMount(() => {
    app.init().catch((e) => (fatal = String(e?.message ?? e)));
  });
</script>

{#if fatal}
  <div class="fatal">
    <h1>启动失败</h1>
    <p>{fatal}</p>
  </div>
{:else if app.ready}
  <div class="shell">
    <Header onsettings={() => settingsDlg?.open()} />
    {#if app.phase === "idle" && app.snapshot.simulating}
      <div class="banner" role="status">
        手机可能仍停在模拟位置（上次没有正常恢复）。
        <button class="btn sm" disabled={app.busy || app.devices.devices.length === 0} onclick={() => app.restore()}>
          恢复真实定位
        </button>
      </div>
    {/if}
    <main class="grid">
      <MapCard onimport={() => importDlg?.open()} onmanage={() => routesDlg?.open()} />
      <aside class="side">
        <RunPanel />
        <PlanPanel />
      </aside>
    </main>
    <LogBar />
  </div>
  <Toasts />
  <ImportDialog bind:this={importDlg} />
  <RoutesDialog bind:this={routesDlg} />
  <SettingsDialog bind:this={settingsDlg} />
  {#if app.dropHover && !app.active}
    <div class="dropzone" aria-hidden="true"><div>松开导入路线</div></div>
  {/if}
{/if}

<style>
  .shell {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .grid {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 372px;
    gap: 14px;
    padding: 14px;
  }
  .side {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-height: 0;
    overflow: auto;
    scrollbar-width: thin;
    padding-bottom: 2px;
  }
  .banner {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 8px 14px;
    background: var(--warn-soft);
    color: var(--fg);
    font-size: 13px;
    border-bottom: 1px solid var(--line);
  }
  .dropzone {
    position: fixed;
    inset: 10px;
    border: 2px dashed var(--accent);
    border-radius: 18px;
    background: color-mix(in srgb, var(--bg) 80%, transparent);
    display: grid;
    place-items: center;
    z-index: 40;
    font-size: 18px;
    font-weight: 600;
    color: var(--accent);
    pointer-events: none;
  }
  .fatal {
    padding: 40px;
  }
</style>
