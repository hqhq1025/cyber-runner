<script lang="ts">
  import { api } from "../lib/api";
  import { app } from "../lib/store.svelte";
  import { deviceName } from "../lib/format";

  let dialog: HTMLDialogElement | undefined = $state();
  const isWin = typeof navigator !== "undefined" && /Windows/i.test(navigator.userAgent);

  export function open() {
    dialog?.showModal();
  }

  async function pickDdi() {
    const p = await api.pickFolder();
    if (p) {
      app.update({ ddiDir: p });
      app.toast("info", "重启 Cyber Runner 后生效");
    }
  }
</script>

<dialog class="modal" bind:this={dialog} aria-labelledby="settings-title" closedby="any">
  <div class="modal-head">
    <h2 id="settings-title">设备与设置</h2>
    <button class="btn icon ghost" aria-label="关闭" onclick={() => dialog?.close()}>✕</button>
  </div>
  <div class="modal-body">
    <section>
      <h3>设备</h3>
      {#if app.devices.error}
        <div class="note bad">
          <b>{app.devices.error.message}</b>
          <p>{app.devices.error.hint}</p>
        </div>
      {:else if app.devices.devices.length === 0}
        <div class="note">没有检测到设备。用数据线连接 iPhone 并解锁。</div>
      {:else}
        <div class="devices" role="radiogroup" aria-label="使用的设备">
          {#each app.devices.devices as d (d.udid)}
            <label class="dev" class:on={app.device?.udid === d.udid}>
              <input
                type="radio"
                name="dev"
                checked={app.device?.udid === d.udid}
                disabled={app.active}
                onchange={() => app.update({ udid: d.udid })}
              />
              <span class="grow">
                <b>{deviceName(d)}</b>
                <span class="meta">iOS {d.iosVersion ?? "?"} · {d.connection === "usb" ? "USB" : "无线"} · {d.trusted ? "已信任" : "未信任"}</span>
              </span>
            </label>
          {/each}
        </div>
      {/if}
      <div class="row">
        <div>
          <b>恢复真实定位</b>
          <p class="meta">手机定位卡在模拟位置时使用（例如上次异常退出）。重启手机也能恢复。</p>
        </div>
        <button
          class="btn"
          disabled={app.active || app.busy || app.devices.devices.length === 0}
          onclick={() => app.restore()}>恢复</button>
      </div>
    </section>

    <section>
      <h3>开发者磁盘镜像</h3>
      <div class="row">
        <div>
          <p class="meta">
            首次使用时自动下载（约 17 MB，自动挑最快的线路，含国内镜像）并缓存。网络不通时，可以选择一个包含
            <code>Image.dmg</code>、<code>Image.dmg.trustcache</code>、<code>BuildManifest.plist</code> 的文件夹。
          </p>
          {#if app.settings.ddiDir}<p class="path">{app.settings.ddiDir}</p>{/if}
        </div>
        <div class="col">
          <button class="btn" onclick={pickDdi}>选择文件夹</button>
          {#if app.settings.ddiDir}
            <button class="btn ghost sm" onclick={() => app.update({ ddiDir: null })}>清除</button>
          {/if}
        </div>
      </div>
    </section>

    <section>
      <h3>第一次使用</h3>
      <ol class="help">
        {#if isWin}
          <li>安装苹果设备驱动：微软商店搜索「Apple Devices」安装（或安装 iTunes），装好后打开一次。<b>不需要</b>管理员权限运行本程序。</li>
        {/if}
        <li>用能传数据的线连接 iPhone（iOS 17 及以上），解锁手机。</li>
        <li>点「开始跑步」，手机弹出「要信任此电脑吗？」时点信任并输入密码。</li>
        <li>如提示开启开发者模式：设置 → 隐私与安全性 → 开发者模式，打开后按提示重启。</li>
        <li>跑完后点「结束」，定位会自动恢复。</li>
      </ol>
    </section>
  </div>
  <div class="modal-foot">
    <span class="ver">Cyber Runner {app.version}</span>
    <button class="btn primary" onclick={() => dialog?.close()}>完成</button>
  </div>
</dialog>

<style>
  dialog {
    width: 560px;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  section + section {
    border-top: 1px solid var(--line);
    padding-top: 14px;
  }
  h3 {
    margin: 0;
    font-size: 13px;
    color: var(--muted);
    font-weight: 600;
  }
  .note {
    font-size: 13px;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--panel-2);
  }
  .note.bad {
    background: var(--danger-soft);
  }
  .note p {
    margin: 4px 0 0;
  }
  .devices {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .dev {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border: 1px solid var(--line);
    border-radius: 10px;
    cursor: pointer;
  }
  .dev.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .grow {
    flex: 1;
    display: flex;
    flex-direction: column;
  }
  .meta {
    font-size: 12px;
    color: var(--muted);
    margin: 2px 0 0;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }
  .col {
    display: flex;
    flex-direction: column;
    gap: 4px;
    align-items: flex-end;
  }
  .path {
    font-family: var(--mono);
    font-size: 11.5px;
    margin: 4px 0 0;
    word-break: break-all;
  }
  code {
    font-family: var(--mono);
    font-size: 11px;
  }
  .help {
    margin: 0;
    padding-left: 18px;
    font-size: 13px;
    color: var(--fg-2);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .ver {
    margin-right: auto;
    align-self: center;
    font-size: 12px;
    color: var(--muted);
  }
</style>
