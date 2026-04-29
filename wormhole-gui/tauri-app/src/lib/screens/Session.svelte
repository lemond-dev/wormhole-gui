<script>
  import { tick, onMount, onDestroy } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import { listen } from '@tauri-apps/api/event';
  import Icon from '../Icon.svelte';
  import TopBar from '../components/TopBar.svelte';
  import Bubble from '../components/Bubble.svelte';
  import Composer from '../components/Composer.svelte';
  import FileCard from '../components/FileCard.svelte';
  import { code, messages } from '../store.js';
  import { sendText, sendFile, closeSession } from '../ipc.js';

  let timelineEl;
  let showCloseConfirm = false;
  let showDrop = false;
  let dropUnlistens = [];
  let elapsedSec = 0;
  let tickHandle;

  // Auto-scroll on new messages. Subscribe explicitly instead of using a
  // `$:` block — referencing `timelineEl` (bind:this) inside a reactive
  // block triggers infinite re-runs in Svelte 4 because safe_not_equal
  // treats DOM refs as always-changing.
  let unsubMessages;
  onMount(async () => {
    unsubMessages = messages.subscribe(() => {
      tick().then(() => {
        if (timelineEl) timelineEl.scrollTop = timelineEl.scrollHeight;
      });
    });
    const start = Date.now();
    tickHandle = setInterval(() => {
      const next = Math.floor((Date.now() - start) / 1000);
      if (next !== elapsedSec) elapsedSec = next;
    }, 1000);

    // Tauri 2 webview drag-drop. Events fire on the window; the runtime
    // suppresses the browser's native drop, so the textarea won't receive
    // the file as text.
    try {
      const u1 = await listen('tauri://drag-enter', () => { showDrop = true; });
      const u2 = await listen('tauri://drag-over',  () => { showDrop = true; });
      const u3 = await listen('tauri://drag-leave', () => { showDrop = false; });
      const u4 = await listen('tauri://drag-drop', async (e) => {
        showDrop = false;
        const paths = e.payload?.paths || [];
        for (const p of paths) {
          try { await sendFile(p); } catch (err) { console.error('sendFile', err); }
        }
      });
      dropUnlistens = [u1, u2, u3, u4];
    } catch (err) {
      console.warn('drag-drop listeners unavailable', err);
    }
  });
  onDestroy(() => {
    if (unsubMessages) unsubMessages();
    if (tickHandle) clearInterval(tickHandle);
    dropUnlistens.forEach((u) => { try { u(); } catch {} });
  });

  async function onSend(text) { await sendText(text); }

  async function pickFile() {
    try {
      const selected = await open({ multiple: false, directory: false });
      if (selected) await sendFile(selected);
    } catch (err) {
      console.error('pickFile', err);
    }
  }

  function fmt(ts) {
    if (!ts) return '';
    const d = new Date(ts);
    return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  }

  function askClose() { showCloseConfirm = true; }
  async function confirmClose() {
    showCloseConfirm = false;
    await closeSession();
  }
  function cancelClose() { showCloseConfirm = false; }

  $: hasInProgress = $messages.some(
    (m) => m.kind === 'file' && (m.state === 'sending' || m.state === 'receiving')
  );
</script>

<div class="wm-app">
  <TopBar code={$code || ''} {elapsedSec} onClose={askClose} />
  <div class="wm-timeline" bind:this={timelineEl}>
    {#each $messages as m (m.id)}
      {#if m.kind === 'system'}
        <div class="wm-system">
          <Icon name="shield-check" size={11} stroke={1.75} />
          {m.content}{m.ts ? ` · ${fmt(m.ts)}` : ''}
        </div>
      {:else if m.kind === 'text'}
        <Bubble side={m.side} text={m.content} time={fmt(m.ts)} status={m.status || ''} />
      {:else if m.kind === 'file'}
        <FileCard {m} />
      {/if}
    {/each}
  </div>

  <Composer
    onSend={onSend}
    onAttach={pickFile}
    placeholder={hasInProgress ? '输入消息（传输进行中）…' : '输入消息或拖入文件…'}
  />

  {#if showDrop}
    <div class="wm-drop-overlay">
      <Icon name="download" size={28} stroke={1.6} />
      <div>松手以发送</div>
    </div>
  {/if}

  {#if showCloseConfirm}
    <div class="wm-modal-backdrop">
      <div class="wm-modal">
        <h3>结束本次会话？</h3>
        <p>会话关闭后无法恢复，所有未完成的传输都会丢失。</p>
        <div class="actions">
          <button class="wm-btn ghost" style="flex:1;" on:click={cancelClose}>继续会话</button>
          <button class="wm-btn danger" style="flex:1;" on:click={confirmClose}>结束会话</button>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .wm-system :global(svg) { vertical-align: -1px; margin-right: 4px; }
</style>
