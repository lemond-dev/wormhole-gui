<script>
  import { tick } from 'svelte';
  import Icon from '../Icon.svelte';
  import TopBar from '../components/TopBar.svelte';
  import Bubble from '../components/Bubble.svelte';
  import Composer from '../components/Composer.svelte';
  import { code, messages } from '../store.js';
  import { sendText, closeSession } from '../ipc.js';

  let timelineEl;
  let showCloseConfirm = false;

  // Auto-scroll to bottom on new message.
  $: if ($messages && timelineEl) {
    tick().then(() => {
      timelineEl.scrollTop = timelineEl.scrollHeight;
    });
  }

  async function onSend(text) {
    await sendText(text);
  }

  function fmt(ts) {
    if (!ts) return '';
    const d = new Date(ts);
    const hh = String(d.getHours()).padStart(2, '0');
    const mm = String(d.getMinutes()).padStart(2, '0');
    return `${hh}:${mm}`;
  }

  function askClose() { showCloseConfirm = true; }
  async function confirmClose() {
    showCloseConfirm = false;
    await closeSession();
  }
  function cancelClose() { showCloseConfirm = false; }
</script>

<div class="wm-app">
  <TopBar code={$code || ''} onClose={askClose} />
  <div class="wm-timeline" bind:this={timelineEl}>
    {#each $messages as m (m.id)}
      {#if m.kind === 'system'}
        <div class="wm-system">
          <Icon name="shield-check" size={11} stroke={1.75} />
          {m.content}{m.ts ? ` · ${fmt(m.ts)}` : ''}
        </div>
      {:else if m.kind === 'text'}
        <Bubble side={m.side} text={m.content} time={fmt(m.ts)} status={m.status || ''} />
      {/if}
    {/each}
  </div>
  <Composer onSend={onSend} placeholder="输入消息…" />

  {#if showCloseConfirm}
    <div class="wm-modal-backdrop">
      <div class="wm-modal">
        <h3>结束本次会话？</h3>
        <p>会话关闭后无法恢复，所有未完成的内容都会丢失。</p>
        <div class="actions">
          <button class="wm-btn ghost" style="flex:1;" on:click={cancelClose}>继续会话</button>
          <button class="wm-btn danger" style="flex:1;" on:click={confirmClose}>结束会话</button>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .wm-system :global(svg) {
    vertical-align: -1px;
    margin-right: 4px;
  }
</style>
