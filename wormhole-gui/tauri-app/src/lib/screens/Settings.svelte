<script>
  import { onMount } from 'svelte';
  import SimpleHeader from '../components/SimpleHeader.svelte';
  import { appState } from '../store.js';
  import { getConfig, setConfig, pickDownloadDir } from '../ipc.js';

  // Read-only display of where the relays live; not configurable in v0.2.
  const MAILBOX_RELAY = 'ws://relay.magic-wormhole.io:4000/v1';
  const TRANSIT_RELAY = 'tcp:transit.magic-wormhole.io:4001';
  const VERSION = '0.2.2';

  let config = null;
  let saving = false;

  onMount(async () => {
    config = await getConfig();
  });

  async function chooseDir() {
    const picked = await pickDownloadDir();
    if (!picked) return;
    config = { ...config, download_dir: picked };
    await persist();
  }

  async function toggleNumericCode(e) {
    config = { ...config, numeric_code: e.currentTarget.checked };
    await persist();
  }

  async function persist() {
    if (!config || saving) return;
    saving = true;
    try {
      await setConfig(config);
    } catch (err) {
      console.error('setConfig failed', err);
    } finally {
      saving = false;
    }
  }

  function back() {
    appState.set('idle');
  }
</script>

<div class="wm-app">
  <SimpleHeader title="设置" onBack={back} showSettings={false} />
  <div class="wm-flowpage wm-settings" style="padding: 18px 22px; gap: 4px;">

    {#if config}
      <div class="field">
        <label>默认下载目录</label>
        <div class="dir-row">
          <input value={config.download_dir} readonly />
          <button class="wm-btn" on:click={chooseDir} disabled={saving}>选择…</button>
        </div>
        <span class="hint">接收的文件会带时间戳避免覆盖。</span>
      </div>

      <div class="field">
        <label>
          <input type="checkbox" checked={false} disabled={true} />
          自动接收文件
        </label>
        <span class="hint">
          暂时禁用 — 当前版本统一使用人工确认，避免恶意载体（.lnk / .hta / .iso / 宏文档等）自动落盘。
        </span>
      </div>

      <div class="field">
        <label>
          <input type="checkbox" checked={config.numeric_code} on:change={toggleNumericCode} disabled={saving} />
          使用数字短码
        </label>
        <span class="hint">
          示例：15-123-456。比英文词稍弱但口播更顺；下次会话生效。
        </span>
      </div>

      <div class="wm-divider"></div>

      <div class="field">
        <label>Mailbox relay</label>
        <input value={MAILBOX_RELAY} readonly />
      </div>
      <div class="field">
        <label>Transit relay</label>
        <input value={TRANSIT_RELAY} readonly />
        <span class="hint">v0.2 使用官方公共 relay。</span>
      </div>

      <div class="wm-divider"></div>

      <div class="version-row">
        <span>Wormhole-GUI v{VERSION}</span>
      </div>
    {:else}
      <div class="hint">加载中…</div>
    {/if}
  </div>
</div>

<style>
  .wm-settings .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 14px;
  }
  .wm-settings .field label {
    font-size: 12px;
    color: var(--text-2);
    font-weight: 500;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .wm-settings .field input[type="text"],
  .wm-settings .field input:not([type]),
  .wm-settings .field input[readonly] {
    appearance: none;
    border: 1px solid var(--border-strong);
    background: var(--surface);
    border-radius: var(--r-sm);
    padding: 8px 10px;
    font: inherit;
    font-size: 13px;
    color: var(--text);
    outline: none;
  }
  .wm-settings .field input:read-only {
    color: var(--text-2);
    background: var(--surface-2);
  }
  .wm-settings .dir-row {
    display: flex;
    gap: 6px;
  }
  .wm-settings .dir-row input { flex: 1; min-width: 0; }
  .wm-settings .dir-row .wm-btn { flex-shrink: 0; padding: 6px 12px; font-size: 12px; }
  .wm-divider {
    height: 1px;
    background: var(--border);
    margin: 8px 0;
  }
  .hint {
    font-size: 11px;
    color: var(--text-3);
  }
  .version-row {
    font-size: 12px;
    color: var(--text-3);
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .version-row a {
    color: var(--brand);
    text-decoration: none;
  }
</style>
