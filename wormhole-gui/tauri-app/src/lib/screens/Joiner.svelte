<script>
  import Icon from '../Icon.svelte';
  import SimpleHeader from '../components/SimpleHeader.svelte';
  import { appState, lastError } from '../store.js';
  import { startRecv } from '../ipc.js';

  let raw = '';
  let busy = false;
  let invalid = false;

  $: normalized = raw.toLowerCase().replace(/\s+/g, '-').replace(/[^a-z0-9-]/g, '');
  $: valid = /^\d{1,3}-[a-z]+(?:-[a-z]+)+$/.test(normalized);

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
  <SimpleHeader title="接收" onBack={back} />
  <div class="wm-flowpage">
    <h2>输入对方给你的短码</h2>
    <div class="desc">
      格式形如 <span class="mono">26-dinosaur-spaniel</span>。自动小写并加连字符。
    </div>
    <div class="wm-codeinput">
      <input
        bind:value={raw}
        placeholder="nn-word-word"
        on:keydown={handleKey}
        autocomplete="off"
        autocapitalize="none"
        spellcheck="false"
      />
    </div>
    {#if invalid}
      <div style="font-size:12px; color:var(--danger); display:flex; align-items:center; gap:6px;">
        <Icon name="alert-circle" size={13} />
        短码不正确，请检查后重新输入
      </div>
    {/if}
    <button class="wm-btn primary" disabled={!valid || busy} on:click={connect}>
      {busy ? '连接中…' : '连接'}
    </button>
    <div class="desc" style="margin-top:8px;">
      <Icon name="info" size={12} stroke={1.75} />
      连接成功后双方会看到 4 位数字，请通过电话或当面与对方核对。
    </div>
    <div class="wm-mt-auto"></div>
    <button class="wm-btn ghost" on:click={back}>返回</button>
  </div>
</div>

<style>
  .desc :global(svg) { vertical-align: middle; margin-right: 4px; }
</style>
