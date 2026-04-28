<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';

  // Phase 0/1 placeholder UI. Real UI lands in Phase 2 from cc design.
  let log = [];
  let codeInput = '';
  let mySas = '';

  function append(line) {
    log = [...log, `${new Date().toLocaleTimeString()} ${line}`];
  }

  onMount(() => {
    const unlisteners = [];
    listen('session:code', (e) => append(`[code] ${e.payload.code}`)).then(u => unlisteners.push(u));
    listen('session:sas_ready', (e) => { mySas = e.payload.sas; append(`[sas] ${e.payload.sas}`); }).then(u => unlisteners.push(u));
    listen('session:connected', () => append('[connected]')).then(u => unlisteners.push(u));
    listen('session:closed', (e) => append(`[closed] ${e.payload.reason}`)).then(u => unlisteners.push(u));
    listen('msg:text', (e) => append(`[text from peer] ${e.payload.content}`)).then(u => unlisteners.push(u));
    listen('msg:text_sent', (e) => append(`[text sent] ${e.payload.content}`)).then(u => unlisteners.push(u));
    listen('error', (e) => append(`[error] ${e.payload.code}: ${e.payload.message}`)).then(u => unlisteners.push(u));
    return () => unlisteners.forEach(u => u());
  });

  async function startSend() {
    try { await invoke('start_session', { mode: 'send', code: null }); append('start_session(send) ok'); }
    catch (e) { append(`start_session error: ${e}`); }
  }
  async function startRecv() {
    if (!codeInput.trim()) { append('请输入短码'); return; }
    try { await invoke('start_session', { mode: 'recv', code: codeInput.trim() }); append('start_session(recv) ok'); }
    catch (e) { append(`start_session error: ${e}`); }
  }
  async function confirmSas(matches) {
    try { await invoke('confirm_sas', { matches }); append(`confirm_sas(${matches}) ok`); }
    catch (e) { append(`confirm_sas error: ${e}`); }
  }
  let textToSend = '';
  async function sendText() {
    if (!textToSend) return;
    try { await invoke('send_text', { content: textToSend }); textToSend = ''; }
    catch (e) { append(`send_text error: ${e}`); }
  }
  async function closeSession() {
    try { await invoke('close_session'); }
    catch (e) { append(`close error: ${e}`); }
  }
</script>

<main>
  <h1>wormhole-gui · Phase 0 dev console</h1>
  <p style="color: #999">真正的 UI 在 Phase 2 接入。本页只用来验证 IPC 通路。</p>

  <section>
    <button on:click={startSend}>发送（生成短码）</button>
    <input placeholder="或输入对方短码 e.g. 12-foo-bar" bind:value={codeInput} />
    <button on:click={startRecv}>接收</button>
  </section>

  {#if mySas}
    <section>
      <h3>SAS：<code>{mySas}</code></h3>
      <button on:click={() => confirmSas(true)}>一致</button>
      <button on:click={() => confirmSas(false)}>不一致</button>
    </section>
  {/if}

  <section>
    <input placeholder="文字" bind:value={textToSend} on:keydown={(e) => e.key === 'Enter' && sendText()} />
    <button on:click={sendText}>发送文字</button>
    <button on:click={closeSession}>关闭会话</button>
  </section>

  <pre>
{#each log as line}
{line}
{/each}
  </pre>
</main>

<style>
  :global(body) { font-family: system-ui, sans-serif; padding: 16px; max-width: 720px; }
  section { margin: 12px 0; display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  pre { background: #f5f5f5; padding: 12px; height: 360px; overflow: auto; }
  code { background: #eef; padding: 2px 6px; border-radius: 4px; }
  button { padding: 6px 12px; cursor: pointer; }
  input { padding: 6px; flex: 1; min-width: 160px; }
</style>
