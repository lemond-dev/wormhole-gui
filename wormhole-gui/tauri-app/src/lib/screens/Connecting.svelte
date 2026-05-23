<script>
  import { _ } from 'svelte-i18n';
  import Icon from '../Icon.svelte';
  import SimpleHeader from '../components/SimpleHeader.svelte';
  import { closeSession } from '../ipc.js';
  import { reset } from '../store.js';

  async function cancel() {
    await closeSession();
    reset();
  }
</script>

<div class="wm-app">
  <SimpleHeader title={$_('connecting.title')} onBack={cancel} />
  <div
    class="wm-flowpage"
    style="align-items:center; justify-content:center; text-align:center; gap:18px; padding:40px 32px;"
  >
    <div class="spinner-wrap">
      <div class="spinner"></div>
      <div class="spinner-icon">
        <Icon name="lock" size={20} stroke={2} />
      </div>
    </div>
    <div style="font-size:15px; font-weight:500;">{$_('connecting.negotiating')}</div>
    <div class="desc" style="text-align:center; max-width:280px;">
      {$_('connecting.desc')}
    </div>
    <div class="wm-mt-auto"></div>
    <button class="wm-btn ghost" on:click={cancel}>{$_('common.cancel')}</button>
  </div>
</div>

<style>
  .spinner-wrap {
    width: 56px;
    height: 56px;
    position: relative;
    margin: 0 auto;
  }
  .spinner {
    position: absolute;
    inset: 0;
    border-radius: 50%;
    border: 3px solid var(--surface-3);
    border-top-color: var(--brand);
    animation: wm-spin 0.9s linear infinite;
  }
  .spinner-icon {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--brand);
  }
</style>
