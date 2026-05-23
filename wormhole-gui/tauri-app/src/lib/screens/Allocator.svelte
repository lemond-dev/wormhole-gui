<script>
  import { onMount, onDestroy } from 'svelte';
  import { _ } from 'svelte-i18n';
  import { get } from 'svelte/store';
  import SimpleHeader from '../components/SimpleHeader.svelte';
  import CodeBanner from '../components/CodeBanner.svelte';
  import { code, appState, reset, lastError } from '../store.js';
  import { closeSession } from '../ipc.js';

  // 5-min TTL countdown
  const TTL_SECONDS = 5 * 60;
  let remainingSec = TTL_SECONDS;
  let timer;
  let toast = false;
  let toastTimer;

  $: m = String(Math.floor(remainingSec / 60)).padStart(2, '0');
  $: s = String(remainingSec % 60).padStart(2, '0');
  $: urgent = remainingSec <= 60;
  $: timeStr = `${m}:${s}`;

  onMount(() => {
    timer = setInterval(() => {
      remainingSec -= 1;
      if (remainingSec <= 0) {
        clearInterval(timer);
        // Mark this as an "expired" error so the Error screen picks the
        // right wording, then close the backend session.
        lastError.set({ code: 'code_expired', message: get(_)('allocator.ttlExpired') });
        closeSession().catch(() => {});
        appState.set('error');
      }
    }, 1000);
  });

  onDestroy(() => {
    clearInterval(timer);
    clearTimeout(toastTimer);
  });

  async function copyCode() {
    if (!$code) return;
    try {
      await navigator.clipboard.writeText($code);
    } catch {}
    toast = true;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => { toast = false; }, 1400);
  }

  async function cancel() {
    await closeSession();
    reset();
  }
</script>

<div class="wm-app">
  <SimpleHeader title={$_('allocator.title')} onBack={cancel} />
  <div class="wm-flowpage">
    <CodeBanner code={$code || ''} onCopy={copyCode} />
    <div class="desc">
      {$_('allocator.instr1')}<br />
      <strong>{$_('allocator.instr2')}</strong>
    </div>
    <div class="wm-wait">
      <span class="ring"></span>
      {$_('allocator.waiting')}
    </div>
    <div class="countdown" class:urgent>
      {$_('allocator.ttlLabel')}<b>{timeStr}</b>
    </div>
    <div class="wm-mt-auto"></div>
    <button class="wm-btn ghost" on:click={cancel}>{$_('common.cancel')}</button>
  </div>
  {#if toast}
    <div class="wm-toast">{$_('common.copied')}</div>
  {/if}
</div>
