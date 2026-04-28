<script>
  import { onMount, onDestroy } from 'svelte';
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
        lastError.set({ code: 'code_expired', message: '短码已过期 (code expired)' });
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
  <SimpleHeader title="发送" onBack={cancel} />
  <div class="wm-flowpage">
    <CodeBanner code={$code || ''} onCopy={copyCode} />
    <div class="desc">
      把这个短码用电话／当面／Signal 等可信渠道告诉对方。<br />
      <strong>不要在同一渠道既发短码又发内容。</strong>
    </div>
    <div class="wm-wait">
      <span class="ring"></span>
      等待对方连接…
    </div>
    <div class="countdown" class:urgent>
      短码有效期 <b>{timeStr}</b>
    </div>
    <div class="wm-mt-auto"></div>
    <button class="wm-btn ghost" on:click={cancel}>取消</button>
  </div>
  {#if toast}
    <div class="wm-toast">已复制</div>
  {/if}
</div>
