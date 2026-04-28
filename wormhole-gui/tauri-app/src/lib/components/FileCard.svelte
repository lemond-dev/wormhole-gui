<script>
  import Icon from '../Icon.svelte';
  import { acceptFile, rejectFile, cancelFile } from '../ipc.js';

  /** Message object as stored in store.messages. */
  export let m;

  function fmtBytes(n) {
    if (n == null) return '';
    if (n >= 1024 ** 3) return (n / 1024 ** 3).toFixed(2) + ' GB';
    if (n >= 1024 ** 2) return (n / 1024 ** 2).toFixed(1) + ' MB';
    if (n >= 1024) return Math.round(n / 1024) + ' KB';
    return n + ' B';
  }

  $: pct = m.size > 0 ? Math.min(100, Math.round((m.bytes || 0) / m.size * 100)) : 0;

  // MIME / ext heuristic for danger flag and icon.
  $: ext = (m.name || '').split('.').pop().toLowerCase();
  $: dangerous = ['exe', 'msi', 'bat', 'cmd', 'com', 'scr', 'ps1'].includes(ext)
    || (m.mime || '').includes('msdownload');
  $: iconName = ['png','jpg','jpeg','gif','webp','heic'].includes(ext) ? 'image'
    : ['pdf','doc','docx','txt','md','log','json'].includes(ext) ? 'file-text'
    : 'file';

  function onAccept() { acceptFile(m.id); }
  function onReject() { rejectFile(m.id, 'user_reject'); }
  function onCancel() { cancelFile(m.id); }
</script>

<div
  class="wm-card"
  class:self={m.side === 'self'}
  class:peer={m.side === 'peer'}
  class:warn-card={m.state === 'offer'}
>
  <div class="head">
    <span class="ftype" class:danger={dangerous} class:ok={m.state === 'received' || m.state === 'sent'}>
      <Icon name={dangerous ? 'alert-triangle' : iconName} size={18} />
    </span>
    <div style="flex:1; min-width:0;">
      <div class="name">{m.name}</div>
      <div class="sub">
        {fmtBytes(m.size)}
        {#if m.mime}<span style="color:var(--text-4); margin-left:6px;">· {m.mime}</span>{/if}
      </div>
    </div>
    {#if m.state === 'sent' || m.state === 'received'}
      <Icon name="check-double" size={16} stroke={1.75} />
    {/if}
  </div>

  {#if dangerous && m.state === 'offer'}
    <div class="danger-banner">
      <Icon name="alert-triangle" size={13} stroke={2} />
      这是可执行文件，仅在你确认信任发送方时接收
    </div>
  {/if}

  {#if m.state === 'offer'}
    <div style="font-size:12px; color:var(--warn-ink);">对方想发送一个文件</div>
    <div class="actions">
      <button class="wm-btn ghost" on:click={onReject}>拒绝</button>
      <button class="wm-btn {dangerous ? 'danger' : 'primary'}" on:click={onAccept}>
        {dangerous ? '仍然接收' : '接收'}
      </button>
    </div>
  {:else if m.state === 'awaiting'}
    <div style="font-size:12px; color:var(--text-3);">已发送给对方，等待接受…</div>
    <div class="actions">
      <button class="wm-btn ghost" on:click={onCancel}>
        <Icon name="x" size={12} /> 取消
      </button>
    </div>
  {:else if m.state === 'sending' || m.state === 'receiving'}
    <div class="progress"><i style:width="{pct}%"></i></div>
    <div class="wm-row" style="justify-content:space-between;">
      <span class="sub">{pct}% · {fmtBytes(m.bytes || 0)}</span>
      <button class="wm-btn ghost" style="padding:4px 10px; font-size:12px;" on:click={onCancel}>
        <Icon name="x" size={12} /> 取消
      </button>
    </div>
  {:else if m.state === 'received'}
    <div class="actions">
      <span class="sub" style="flex:1;">已保存</span>
    </div>
  {:else if m.state === 'sent'}
    <div style="font-size:12px; color:var(--brand-ink);">已送达</div>
  {:else if m.state === 'failed'}
    <div class="error-line">
      <Icon name="alert-circle" size={13} />
      {m.error || '传输失败'}
    </div>
  {:else if m.state === 'cancelled'}
    <div style="font-size:12px; color:var(--text-3);">已取消</div>
  {/if}
</div>

<style>
  .danger-banner {
    font-size: 12px;
    color: var(--danger-ink);
    display: flex;
    align-items: center;
    gap: 6px;
    background: var(--danger-soft);
    padding: 6px 8px;
    border-radius: 6px;
  }
  .danger-banner :global(svg) { color: var(--danger); }
  .error-line {
    font-size: 12px;
    color: var(--danger);
    display: flex;
    gap: 6px;
    align-items: center;
  }
</style>
