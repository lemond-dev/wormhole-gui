<script>
  import SimpleHeader from '../components/SimpleHeader.svelte';
  import { appState } from '../store.js';

  // v0.1: most settings are read-only display. Persisted settings + relay
  // override come in v0.2.

  // Constants mirror core/src/transfer.rs and core/src/storage.rs defaults.
  const MAILBOX_RELAY = 'ws://relay.magic-wormhole.io:4000/v1';
  const TRANSIT_RELAY = 'tcp:transit.magic-wormhole.io:4001';
  const DOWNLOAD_DIR = '~/Downloads/Wormhole/';
  const VERSION = '0.1.0';

  function back() {
    appState.set('idle');
  }
</script>

<div class="wm-app">
  <SimpleHeader title="设置" onBack={back} showSettings={false} />
  <div class="wm-flowpage wm-settings" style="padding: 18px 22px; gap: 4px;">

    <div class="field">
      <label>默认下载目录</label>
      <input value={DOWNLOAD_DIR} readonly />
      <span class="hint">v0.1 暂不支持自定义；接收的文件会带上时间戳避免覆盖。</span>
    </div>

    <div class="field">
      <label>自动接收</label>
      <div class="seg">
        <button class="on" disabled>始终询问</button>
        <button disabled>{'< 10 MB 自动'}</button>
        <button disabled>始终自动</button>
      </div>
      <span class="hint">v0.1 始终弹确认。</span>
    </div>

    <div class="field">
      <label>语言</label>
      <input value="中文（简体）" readonly />
    </div>

    <div class="field">
      <label>短码字典</label>
      <input value="PGP 英文词表" readonly />
      <span class="hint">v0.1 跟随 magic-wormhole 默认；纯数字短码在 v0.2 加入。</span>
    </div>

    <div class="wm-divider"></div>

    <div class="field">
      <label>Mailbox relay</label>
      <input value={MAILBOX_RELAY} readonly />
    </div>
    <div class="field">
      <label>Transit relay</label>
      <input value={TRANSIT_RELAY} readonly />
      <span class="hint">v0.1 使用官方 relay；自定义在 v0.2 加入。</span>
    </div>

    <div class="wm-divider"></div>

    <div class="version-row">
      <span>Wormhole-GUI v{VERSION}</span>
      <a href="https://github.com/lemond-dev/chat_one" target="_blank" rel="noopener">
        GitHub ↗
      </a>
    </div>
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
  }
  .wm-settings .field input,
  .wm-settings .field select {
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
  .wm-settings .seg {
    display: flex;
    gap: 4px;
    background: var(--surface-2);
    padding: 3px;
    border-radius: var(--r-sm);
  }
  .wm-settings .seg button {
    flex: 1;
    border: 0;
    background: transparent;
    font: inherit;
    font-size: 12px;
    padding: 6px 8px;
    border-radius: 4px;
    cursor: not-allowed;
    color: var(--text-2);
  }
  .wm-settings .seg button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow-1);
    font-weight: 500;
  }
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
