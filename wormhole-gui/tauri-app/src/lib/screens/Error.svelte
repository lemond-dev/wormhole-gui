<script>
  import { _ } from 'svelte-i18n';
  import Icon from '../Icon.svelte';
  import SimpleHeader from '../components/SimpleHeader.svelte';
  import { lastError, closeReason, reset } from '../store.js';

  // Map error/close reason to a kind for the design's error variants.
  // We pattern-match against both languages so the heuristic still works
  // whether the message arrived from a zh-locked session or an en one.
  $: kind = pickKind($lastError, $closeReason);

  function pickKind(err, reason) {
    const txt = `${err?.message || ''} ${reason || ''}`.toLowerCase();
    if (txt.includes('pake')) return 'pake';
    if (txt.includes('expired') || txt.includes('过期')) return 'expired';
    if (txt.includes('peer') || txt.includes('对方') || txt.includes('lonely')) return 'peer-closed';
    if (txt.includes('network') || txt.includes('rendezvous') || txt.includes('socket') || txt.includes('connection')) return 'network';
    return 'peer-closed';
  }

  $: meta = {
    pake: { icon: 'shield-alert', titleKey: 'error.titlePake', bodyKey: 'error.descPake' },
    network: { icon: 'wifi-off', titleKey: 'error.titleConnection', bodyKey: 'error.descConnection' },
    'peer-closed': { icon: 'log-out', titleKey: 'error.titlePeerClosed', bodyKey: 'error.descPeerClosed' },
    expired: { icon: 'alert-circle', titleKey: 'error.titleExpired', bodyKey: 'error.descExpired' },
  }[kind];

  function back() { reset(); }
  function restart() { reset(); }
</script>

<div class="wm-app">
  <SimpleHeader title="" onBack={back} />
  <div
    class="wm-flowpage"
    style="align-items:center; justify-content:center; text-align:center; padding:40px 32px;"
  >
    <div class="badge">
      <Icon name={meta.icon} size={26} stroke={1.6} />
    </div>
    <h2 style="margin-top:8px; text-align:center; font-size:18px;">{$_(meta.titleKey)}</h2>
    <div class="desc" style="text-align:center; max-width:320px;">{$_(meta.bodyKey)}</div>
    {#if $lastError?.message}
      <div class="raw mono">{$lastError.message}</div>
    {/if}
    <div class="wm-row" style="justify-content:center; margin-top:8px;">
      <button class="wm-btn" on:click={back}>{$_('common.back')}</button>
      <button class="wm-btn primary" on:click={restart}>
        <Icon name="refresh" size={13} /> {$_('error.restart')}
      </button>
    </div>
  </div>
</div>

<style>
  .badge {
    width: 56px;
    height: 56px;
    border-radius: 50%;
    background: var(--surface-2);
    color: var(--text-3);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    margin: 0 auto;
  }
  .raw {
    margin-top: 12px;
    font-size: 11.5px;
    color: var(--text-4);
    max-width: 340px;
    word-break: break-all;
  }
</style>
