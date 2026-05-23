<script>
  import { _ } from 'svelte-i18n';
  import Icon from '../Icon.svelte';
  import SimpleHeader from '../components/SimpleHeader.svelte';
  import { appState, lastError } from '../store.js';
  import { startRecv } from '../ipc.js';

  let raw = '';
  let busy = false;
  let invalid = false;

  $: normalized = raw.toLowerCase().replace(/\s+/g, '-').replace(/[^a-z0-9-]/g, '');
  // Accept both numeric (15-379-248) and PGP-word (26-dinosaur-spaniel)
  // codes; magic-wormhole's relay enforces the actual nameplate format.
  $: valid = /^[a-z0-9]+(?:-[a-z0-9]+)+$/.test(normalized);

  async function connect() {
    if (!valid || busy) return;
    busy = true;
    invalid = false;
    try {
      await startRecv(normalized);
    } catch (e) {
      invalid = true;
      lastError.set({ code: 'invalid_code', message: String(e) });
    } finally {
      busy = false;
    }
  }

  function back() {
    appState.set('idle');
  }

  function handleKey(e) {
    if (e.key === 'Enter') connect();
  }
</script>

<div class="wm-app">
  <SimpleHeader title={$_('joiner.title')} onBack={back} />
  <div class="wm-flowpage">
    <h2>{$_('joiner.subtitle')}</h2>
    <div class="desc">
      {$_('joiner.formatHint')}<span class="mono">15-123-456</span> / <span class="mono">26-dinosaur-spaniel</span>{$_('joiner.formatHintTail')}
    </div>
    <div class="wm-codeinput">
      <input
        bind:value={raw}
        placeholder="15-123-456"
        on:keydown={handleKey}
        autocomplete="off"
        autocapitalize="none"
        spellcheck="false"
      />
    </div>
    {#if invalid}
      <div style="font-size:12px; color:var(--danger); display:flex; align-items:center; gap:6px;">
        <Icon name="alert-circle" size={13} />
        {$_('joiner.invalidCode')}
      </div>
    {/if}
    <button class="wm-btn primary" disabled={!valid || busy} on:click={connect}>
      {busy ? $_('joiner.connecting') : $_('joiner.connect')}
    </button>
    <div class="wm-mt-auto"></div>
    <button class="wm-btn ghost" on:click={back}>{$_('common.back')}</button>
  </div>
</div>

<style>
  .desc :global(svg) { vertical-align: middle; margin-right: 4px; }
</style>
