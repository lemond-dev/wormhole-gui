<script>
  import { onMount } from 'svelte';
  import SimpleHeader from '../components/SimpleHeader.svelte';
  import { appState } from '../store.js';
  import { getConfig, setConfig, pickDownloadDir, triggerUpdateCheck } from '../ipc.js';
  import { updateState } from '../store.js';

  const DEFAULT_MAILBOX_RELAY = 'wss://mailbox.mw.leastauthority.com/v1';
  const DEFAULT_TRANSIT_RELAY = 'relay.mw.leastauthority.com:4001';
  const VERSION = '0.3.0';

  let config = null;
  let saving = false;
  // Snapshot of the relay values at mount time. We compare the live edits to
  // these to know whether the user needs to restart for the change to take
  // effect (the running session loop captures relays at spawn time).
  // `loaded` gates the reactive comparison so we never flash the dirty
  // warning during the brief window between `config = ...` and the snapshot
  // assignments below.
  let loaded = false;
  let initialMailbox = '';
  let initialTransit = '';

  // Empty string is back-end-normalized to the built-in default (see
  // ConfigState::mailbox_relay / ::transit_relay in config.rs). The dirty
  // check has to do the same normalization, otherwise clearing a field that
  // started as "" → typing the default URL → "恢复默认" cycle would flag
  // pseudo-changes that the running session would not actually observe.
  function effective(v, fallback) {
    return v && v.trim() !== '' ? v : fallback;
  }

  $: relaysDirty =
    loaded &&
    (effective(config.mailbox_relay, DEFAULT_MAILBOX_RELAY) !==
      effective(initialMailbox, DEFAULT_MAILBOX_RELAY) ||
      effective(config.transit_relay, DEFAULT_TRANSIT_RELAY) !==
        effective(initialTransit, DEFAULT_TRANSIT_RELAY));

  onMount(async () => {
    const loaded_cfg = await getConfig();
    initialMailbox = loaded_cfg.mailbox_relay;
    initialTransit = loaded_cfg.transit_relay;
    config = loaded_cfg;
    loaded = true;
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

  async function onRelayBlur() {
    // Persist on blur (not every keystroke) so partial URLs don't churn disk.
    await persist();
  }

  function resetRelays() {
    config = {
      ...config,
      mailbox_relay: DEFAULT_MAILBOX_RELAY,
      transit_relay: DEFAULT_TRANSIT_RELAY,
    };
    persist();
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

  // Manual update check. Routes through the shared helper so the banner
  // (managed by App.svelte) renders any outcome — found / error.
  // `silent: false` makes errors visible and bypasses the per-session
  // dismissal so the user always gets feedback from a manual click.
  let checking = false;
  let manualNoUpdateAt = 0; // ms timestamp; drives the "已是最新" toast
  async function onCheckUpdate() {
    if (checking) return;
    checking = true;
    try {
      const before = $updateState;
      await triggerUpdateCheck({ silent: false });
      // If state stayed null, no update was found → show the toast.
      if (!$updateState && !before) {
        manualNoUpdateAt = Date.now();
      }
    } finally {
      checking = false;
    }
  }

  $: showNoUpdateToast =
    manualNoUpdateAt > 0 && Date.now() - manualNoUpdateAt < 3000;
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
        <input
          bind:value={config.mailbox_relay}
          on:blur={onRelayBlur}
          placeholder={DEFAULT_MAILBOX_RELAY}
          spellcheck="false"
          autocomplete="off"
        />
      </div>
      <div class="field">
        <label>Transit relay</label>
        <input
          bind:value={config.transit_relay}
          on:blur={onRelayBlur}
          placeholder={DEFAULT_TRANSIT_RELAY}
          spellcheck="false"
          autocomplete="off"
        />
        <span class="hint">留空则使用默认值。两端必须配相同的 mailbox 才能相遇。</span>
      </div>
      <div class="relay-actions">
        <button class="wm-btn-link" on:click={resetRelays} disabled={saving}>恢复默认</button>
        {#if relaysDirty}
          <span class="restart-warn">⚠ 已修改，重启软件后生效</span>
        {/if}
      </div>

      <div class="wm-divider"></div>

      <div class="version-row">
        <span>wormhole-gui v{VERSION}</span>
        <button class="wm-btn-link" on:click={onCheckUpdate} disabled={checking}>
          {checking ? '检查中…' : '检查更新'}
        </button>
      </div>
      {#if showNoUpdateToast}
        <div class="hint" style="text-align: right; color: var(--text-2);">已是最新版本</div>
      {/if}
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
  .relay-actions {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 8px;
    min-height: 22px;
  }
  .wm-btn-link {
    background: none;
    border: none;
    color: var(--brand);
    cursor: pointer;
    font-size: 12px;
    padding: 0;
  }
  .wm-btn-link:disabled {
    color: var(--text-3);
    cursor: default;
  }
  .restart-warn {
    font-size: 11px;
    color: #c97d27;
  }
  .version-row {
    font-size: 12px;
    color: var(--text-3);
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
</style>
