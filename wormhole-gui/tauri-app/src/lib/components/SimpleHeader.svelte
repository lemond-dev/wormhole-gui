<script>
  import Icon from '../Icon.svelte';
  export let title = '';
  export let onBack = null;
  export let onSettings = null;
</script>

<div class="wm-topbar">
  <button class="icon-btn" title="返回" on:click={onBack}>
    <Icon name="arrow-right" size={16} stroke={1.75} />
  </button>
  <div style="flex:1; text-align:center; font-weight:600; font-size:14px;">{title}</div>
  <button class="icon-btn" title="设置" on:click={onSettings}>
    <Icon name="settings" size={16} />
  </button>
</div>

<style>
  /* Flip the arrow inside the back button only. */
  button[title='返回'] :global(svg) { transform: rotate(180deg); }
</style>
