<script>
  import { _ } from 'svelte-i18n';
  import Icon from '../Icon.svelte';
  import { startSend } from '../ipc.js';
  import { appState } from '../store.js';

  function chooseSend() { startSend(); }
  function chooseRecv() { appState.set('joiner-input'); }
  function openSettings() { appState.set('settings'); }
</script>

<div class="wm-app">
  <div class="wm-idle">
    <div class="wordmark-row">
      <div class="wordmark">
        <span class="badge"><Icon name="shield-check" size={13} stroke={2.2} /></span>
        Wormhole
      </div>
      <button class="settings-btn" title={$_('common.settings')} on:click={openSettings}>
        <Icon name="settings" size={16} />
      </button>
    </div>
    <h1 style="margin-top: 28px;">{$_('idle.title')}</h1>
    <div class="lede">
      {$_('idle.desc1')}<br />
      {$_('idle.desc2')}
    </div>
    <div class="choices">
      <button class="wm-bigchoice" on:click={chooseSend}>
        <span class="icon"><Icon name="send" size={18} stroke={2} /></span>
        <span class="title">{$_('idle.send')}</span>
        <span class="sub">{$_('idle.sendSub')}</span>
      </button>
      <button class="wm-bigchoice" on:click={chooseRecv}>
        <span class="icon"><Icon name="download" size={18} stroke={2} /></span>
        <span class="title">{$_('idle.recv')}</span>
        <span class="sub">{$_('idle.recvSub')}</span>
      </button>
    </div>
    <div class="footer-note">
      <Icon name="lock" size={11} />
      {$_('idle.footer')}
    </div>
  </div>
</div>

<style>
  .wordmark-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .settings-btn {
    width: 32px;
    height: 32px;
    border: 0;
    background: transparent;
    color: var(--text-3);
    border-radius: var(--r-sm);
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .settings-btn:hover {
    background: var(--surface-2);
    color: var(--text);
  }
</style>
