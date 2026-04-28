<script>
  import { onMount, onDestroy } from 'svelte';
  import Icon from '../Icon.svelte';
  import { sas, sasLocalConfirmed, appState, reset } from '../store.js';
  import { confirmSas, closeSession } from '../ipc.js';

  // 60-second auto-disconnect from architecture §11.4
  const TTL = 60;
  let remaining = TTL;
  let timer;

  $: mm = String(Math.floor(remaining / 60)).padStart(2, '0');
  $: ss = String(remaining % 60).padStart(2, '0');
  $: digits = ($sas || '----').split('');

  onMount(() => {
    timer = setInterval(() => {
      remaining -= 1;
      if (remaining <= 0) {
        clearInterval(timer);
        confirmSas(false).catch(() => {});
        appState.set('error');
      }
    }, 1000);
  });
  onDestroy(() => clearInterval(timer));

  async function approve() {
    await confirmSas(true);
  }
  async function reject() {
    await confirmSas(false);
    await closeSession();
    reset();
  }
</script>

<div class="wm-app">
  <!-- The SAS dialog renders as a modal over an empty app frame.
       The architecture forbids any send/receive in this state. -->
  <div class="wm-modal-backdrop">
    <div class="wm-modal" style="text-align:center;">
      <div class="badge-icon">
        <Icon name="shield-check" size={20} stroke={2} />
      </div>
      <h3>核对安全数字</h3>
      <p>
        请通过电话或当面与对方核对这 4 位数字。<br />
        <strong style="color:var(--text);">对方屏幕上应显示完全相同的数字。</strong><br />
        这一步在防止有人冒充对方。
      </p>
      <div class="wm-sas">
        {#each digits as d}
          <span class="digit">{d}</span>
        {/each}
      </div>
      <div class="caption">
        {#if !$sasLocalConfirmed}
          剩余 {mm}:{ss} 自动断开
        {:else}
          已确认 · 等待对方核对…
        {/if}
      </div>
      {#if !$sasLocalConfirmed}
        <div class="actions">
          <button class="wm-btn ghost" style="flex:1;" on:click={reject}>
            <Icon name="x" size={14} /> 不一致，断开
          </button>
          <button class="wm-btn primary" style="flex:1;" on:click={approve}>
            <Icon name="check" size={14} /> 一致，继续
          </button>
        </div>
      {:else}
        <div class="waiting">
          <span class="mini-spinner"></span>
          等待对方点击「一致，继续」
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .badge-icon {
    width: 40px;
    height: 40px;
    border-radius: 50%;
    background: var(--brand-soft);
    color: var(--brand);
    margin: 0 auto 12px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .caption {
    font-size: 11.5px;
    color: var(--text-3);
    margin-bottom: 12px;
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
  }
  .waiting {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 8px 0;
    color: var(--text-3);
    font-size: 13px;
  }
  .mini-spinner {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 2px solid var(--surface-3);
    border-top-color: var(--brand);
    animation: wm-spin 0.8s linear infinite;
  }
</style>
