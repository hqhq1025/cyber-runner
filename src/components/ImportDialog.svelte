<script lang="ts">
  import { api, type Datum, type RouteView } from "../lib/api";
  import { app } from "../lib/store.svelte";
  import { DATUM_LABEL, lengthText } from "../lib/format";
  import RouteMap from "./RouteMap.svelte";
  import Segmented from "./Segmented.svelte";

  let dialog: HTMLDialogElement | undefined = $state();
  let tab = $state<"file" | "paste">("file");
  let text = $state("");
  let name = $state("");
  let fileName = $state<string | null>(null);
  let detected = $state<Datum | null>(null);
  let datum = $state<Datum>("bd09");
  let preview = $state<RouteView | null>(null);
  let error = $state<string | null>(null);
  let saving = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  export function open(path?: string) {
    tab = "file";
    text = "";
    name = "";
    fileName = null;
    detected = null;
    datum = "bd09";
    preview = null;
    error = null;
    dialog?.showModal();
    if (path) void loadFile(path);
  }

  async function loadFile(path: string) {
    error = null;
    try {
      const d = await api.readRouteFile(path);
      text = d.text;
      name = name || d.suggestedName;
      detected = d.detectedDatum;
      fileName = path.split(/[\\/]/).pop() ?? path;
      if (d.detectedDatum) datum = d.detectedDatum;
      refresh(0);
    } catch (e) {
      error = (e as Error).message;
      preview = null;
    }
  }

  async function pick() {
    const p = await api.pickRouteFile();
    if (p) await loadFile(p);
  }

  function refresh(delay = 300) {
    clearTimeout(timer);
    timer = setTimeout(async () => {
      if (!text.trim()) {
        preview = null;
        error = null;
        return;
      }
      try {
        preview = await api.previewRoute(text, datum);
        error = null;
      } catch (e) {
        preview = null;
        error = (e as Error).message;
      }
    }, delay);
  }

  async function save() {
    if (!preview) return;
    saving = true;
    try {
      const r = await api.createRoute(name, text, datum);
      await app.reloadRoutes();
      app.update({ routeId: r.id });
      app.toast("success", `已导入「${r.name}」`);
      dialog?.close();
    } catch (e) {
      error = (e as Error).message;
    } finally {
      saving = false;
    }
  }

  // Drag & drop anywhere on the window opens this dialog with the file.
  $effect(() => {
    const p = app.pendingImportPath;
    if (!p) return;
    app.pendingImportPath = null;
    if (app.active) {
      app.toast("info", "跑步中不能导入路线");
      return;
    }
    if (dialog?.open) void loadFile(p);
    else open(p);
  });
</script>

<dialog class="modal" bind:this={dialog} aria-labelledby="import-title" closedby="any">
  <div class="modal-head">
    <h2 id="import-title">导入路线</h2>
    <button class="btn icon ghost" aria-label="关闭" onclick={() => dialog?.close()}>✕</button>
  </div>
  <div class="modal-body layout">
    <div class="left">
      <Segmented
        label="来源"
        value={tab}
        options={[
          { value: "file", label: "文件" },
          { value: "paste", label: "粘贴坐标" },
        ]}
        onchange={(v) => (tab = v)}
      />
      {#if tab === "file"}
        <button class="drop" onclick={pick}>
          {#if fileName}
            <strong>{fileName}</strong>
            <span>点此换一个文件</span>
          {:else}
            <strong>选择文件，或把文件拖到窗口里</strong>
            <span>GPX · KML · GeoJSON · 旧版 txt · 经纬度文本</span>
          {/if}
        </button>
      {:else}
        <label class="field">
          <span>坐标（每行一个「经度,纬度」，或 JSON）</span>
          <textarea
            class="input"
            rows="7"
            placeholder={"116.366553,39.967062\n116.366522,39.967051\n…"}
            bind:value={text}
            oninput={() => {
              detected = null;
              refresh();
            }}
          ></textarea>
        </label>
      {/if}

      <label class="field">
        <span>名称</span>
        <input class="input" bind:value={name} maxlength="40" placeholder="例如：东区操场" />
      </label>

      <div class="field">
        <span>坐标系</span>
        {#if detected}
          <div class="fixed">文件格式已注明：{DATUM_LABEL[detected]}</div>
        {:else}
          <Segmented
            label="坐标系"
            value={datum}
            options={[
              { value: "bd09" as Datum, label: "百度", title: "百度地图拾取的坐标" },
              { value: "gcj02" as Datum, label: "高德/腾讯", title: "高德、腾讯地图拾取的坐标" },
              { value: "wgs84" as Datum, label: "GPS", title: "手表/GPS 记录的原始坐标" },
            ]}
            onchange={(v) => {
              datum = v;
              refresh(0);
            }}
          />
          <span class="hint">选错会整体偏移几百米。用百度地图「坐标拾取器」取的点选「百度」。</span>
        {/if}
      </div>
    </div>

    <div class="right">
      <div class="pv">
        {#if preview}
          <RouteMap points={preview.preview} closed={preview.autoClosed} />
        {:else}
          <div class="pv-empty">{error ?? "这里会显示路线预览"}</div>
        {/if}
      </div>
      {#if preview}
        <div class="pvfacts">
          <span>每圈 <b class="num">{lengthText(preview.autoClosed ? preview.lapClosedM : preview.lapOutAndBackM)}</b></span>
          <span>{preview.pointCount} 个点</span>
          <span>{preview.autoClosed ? "首尾相接，按绕圈跑" : `首尾相距 ${Math.round(preview.endGapM)} m，默认往返跑`}</span>
        </div>
      {/if}
    </div>
  </div>
  <div class="modal-foot">
    {#if error && preview === null && text}
      <span class="err">{error}</span>
    {/if}
    <button class="btn" onclick={() => dialog?.close()}>取消</button>
    <button class="btn primary" disabled={!preview || saving} onclick={save}>保存并使用</button>
  </div>
</dialog>

<style>
  dialog {
    width: 820px;
  }
  .layout {
    display: grid;
    grid-template-columns: 330px 1fr;
    gap: 18px;
  }
  .left {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .drop {
    display: flex;
    flex-direction: column;
    gap: 4px;
    align-items: center;
    justify-content: center;
    min-height: 120px;
    border: 1.5px dashed var(--line-strong);
    border-radius: 12px;
    background: var(--panel-2);
    padding: 12px;
    text-align: center;
  }
  .drop:hover {
    border-color: var(--accent);
  }
  .drop strong {
    font-size: 13.5px;
    word-break: break-all;
  }
  .drop span {
    font-size: 12px;
    color: var(--muted);
  }
  .fixed {
    font-size: 13px;
    padding: 7px 10px;
    border-radius: 9px;
    background: var(--accent-soft);
  }
  .hint {
    font-size: 11.5px;
    color: var(--muted);
    font-weight: 400 !important;
  }
  .right {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .pv {
    height: 340px;
    border-radius: 12px;
    overflow: hidden;
    border: 1px solid var(--line);
  }
  .pv-empty {
    display: grid;
    place-items: center;
    height: 100%;
    color: var(--muted);
    background: var(--map-bg);
    padding: 20px;
    text-align: center;
  }
  .pvfacts {
    display: flex;
    gap: 14px;
    font-size: 12.5px;
    color: var(--fg-2);
  }
  .err {
    margin-right: auto;
    color: var(--danger);
    font-size: 12.5px;
    align-self: center;
  }
</style>
