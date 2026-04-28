<script>
  import Icon from '../Icon.svelte';
  export let code = '';
  export let onClose;
</script>

<div class="wm-topbar">
  <span class="status">
    <Icon name="lock" size={12} stroke={2} />
    已加密
  </span>
  <div class="code-pill mono" title={code}>{code}</div>
  <button class="icon-btn danger" title="结束会话" on:click={onClose}>
    <Icon name="log-out" size={16} />
  </button>
</div>
