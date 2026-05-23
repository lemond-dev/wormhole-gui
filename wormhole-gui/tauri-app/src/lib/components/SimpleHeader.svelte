<script>
  import { _ } from 'svelte-i18n';
  import Icon from '../Icon.svelte';
  import { appState } from '../store.js';

  export let title = '';
  export let onBack = null;
  /** Whether to show the settings cog. Hidden on the Settings screen itself. */
  export let showSettings = true;

  function openSettings() {
    appState.set('settings');
  }
</script>

<div class="wm-topbar">
  <button class="icon-btn back-btn" title={$_('simpleHeader.backTitle')} on:click={onBack}>
    <Icon name="arrow-right" size={16} stroke={1.75} />
  </button>
  <div style="flex:1; text-align:center; font-weight:600; font-size:14px;">{title}</div>
  {#if showSettings}
    <button class="icon-btn" title={$_('simpleHeader.settingsTitle')} on:click={openSettings}>
      <Icon name="settings" size={16} />
    </button>
  {:else}
    <span style="width:30px;"></span>
  {/if}
</div>

<style>
  /* Flip the arrow inside the back button only. CSS class instead of an
     attribute selector so the rule survives the back-button title being
     translated. */
  .back-btn :global(svg) { transform: rotate(180deg); }
</style>
