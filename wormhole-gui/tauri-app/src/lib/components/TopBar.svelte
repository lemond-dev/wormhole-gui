<script>
  import { _ } from 'svelte-i18n';
  import Icon from '../Icon.svelte';
  export let code = '';
  export let elapsedSec = 0;
  export let onClose;

  function formatElapsed(s) {
    const t = Math.max(0, s);
    const h = Math.floor(t / 3600);
    const m = Math.floor((t % 3600) / 60);
    const sec = t % 60;
    const pad = (n) => String(n).padStart(2, '0');
    return h > 0 ? `${h}:${pad(m)}:${pad(sec)}` : `${pad(m)}:${pad(sec)}`;
  }
</script>

<div class="wm-topbar">
  <span class="status">
    <Icon name="lock" size={12} stroke={2} />
    {$_('topbar.encrypted')}
  </span>
  <div class="code-pill" title={code}>
    <span class="label">{$_('topbar.code')}</span>
    <span class="val mono">{code || '—'}</span>
  </div>
  <span class="timer mono" title={$_('topbar.duration')}>
    <Icon name="clock" size={12} stroke={1.75} />
    {formatElapsed(elapsedSec)}
  </span>
  <button class="icon-btn danger" title={$_('topbar.endSession')} on:click={onClose}>
    <Icon name="log-out" size={16} />
  </button>
</div>
