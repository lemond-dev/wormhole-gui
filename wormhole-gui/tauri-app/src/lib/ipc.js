// Thin wrapper around Tauri's invoke + event APIs.
// Wires backend Evt enum (core/session.rs) into our Svelte stores.

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { get } from 'svelte/store';
import {
  appState,
  code,
  lastError,
  closeReason,
  pushMessage,
  updateMessage,
  reset,
} from './store.js';

/**
 * Decide which screen to switch to when the session closes. The reason string
 * is whatever the backend's CoreError::Display produced (or "ok").
 */
function pickClosedScreen(reason) {
  const r = (reason || '').toLowerCase();
  if (r === 'ok' || r === '') return 'closed';
  if (r.includes('pake')) return 'error';        // wrong code
  if (r.includes('rendezvous') || r.includes('io error') || r.includes('connection')) return 'error';
  return 'error';
}

function raw_error_get() {
  return get(lastError);
}

let unlistenFns = [];

export async function setupListeners() {
  // Tear down old listeners if we re-init.
  await teardownListeners();

  unlistenFns.push(
    await listen('session:code', (e) => {
      code.set(e.payload.code);
      // First code event lands while we're in 'allocator-wait' (started send mode).
      appState.set('allocator-wait');
    })
  );

  unlistenFns.push(
    await listen('session:connected', () => {
      appState.set('connected');
      pushMessage({
        kind: 'system',
        id: 'sys-connected',
        content: '已建立加密通道',
        ts: Date.now(),
      });
    })
  );

  unlistenFns.push(
    await listen('session:closed', (e) => {
      const reason = e.payload.reason || '';
      closeReason.set(reason);
      const next = pickClosedScreen(reason);
      if (next === 'error' && !raw_error_get()) {
        // Surface the close reason as an error too so the Error screen
        // can show the actual message under the friendly title.
        lastError.set({ code: 'closed', message: reason });
      }
      appState.set(next);
    })
  );

  unlistenFns.push(
    await listen('msg:text', (e) => {
      pushMessage({
        kind: 'text',
        side: 'peer',
        id: e.payload.id,
        content: e.payload.content,
        ts: e.payload.ts,
      });
    })
  );

  unlistenFns.push(
    await listen('msg:text_sent', (e) => {
      pushMessage({
        kind: 'text',
        side: 'self',
        id: e.payload.id,
        content: e.payload.content,
        ts: e.payload.ts,
        status: '✓',
      });
    })
  );

  unlistenFns.push(
    await listen('error', (e) => {
      lastError.set({ code: e.payload.code, message: e.payload.message });
    })
  );

  // ── File transfer events ──
  unlistenFns.push(
    await listen('msg:file_offer', (e) => {
      pushMessage({
        kind: 'file',
        side: 'peer',
        id: e.payload.id,
        name: e.payload.name,
        size: e.payload.size,
        mime: e.payload.mime,
        state: 'offer',
        bytes: 0,
        ts: Date.now(),
      });
    })
  );

  unlistenFns.push(
    await listen('msg:file_offer_sent', (e) => {
      pushMessage({
        kind: 'file',
        side: 'self',
        id: e.payload.id,
        name: e.payload.name,
        size: e.payload.size,
        state: 'awaiting',
        bytes: 0,
        ts: Date.now(),
      });
    })
  );

  unlistenFns.push(
    await listen('file:accepted', (e) => {
      updateMessage(e.payload.id, { state: 'sending', bytes: 0 });
    })
  );

  unlistenFns.push(
    await listen('file:progress', (e) => {
      const { id, bytes, total, dir } = e.payload;
      updateMessage(id, {
        state: dir === 'in' ? 'receiving' : 'sending',
        bytes,
        size: total,
      });
    })
  );

  unlistenFns.push(
    await listen('file:done', (e) => {
      const { id, ok, dir, save_path } = e.payload;
      const next = ok
        ? { state: dir === 'in' ? 'received' : 'sent', save_path: save_path || null }
        : { state: 'failed', error: '传输失败' };
      updateMessage(id, next);
    })
  );

  unlistenFns.push(
    await listen('file:cancelled', (e) => {
      updateMessage(e.payload.id, { state: 'cancelled' });
    })
  );

  unlistenFns.push(
    await listen('file:error', (e) => {
      updateMessage(e.payload.id, { state: 'failed', error: e.payload.message });
    })
  );
}

export async function teardownListeners() {
  for (const fn of unlistenFns) {
    try { fn(); } catch {}
  }
  unlistenFns = [];
}

// ───── Commands ─────
export async function startSend() {
  reset();
  appState.set('allocator-wait'); // optimistic; backend will confirm via session:code
  await invoke('start_session', { mode: 'send', code: null });
}

export async function startRecv(codeStr) {
  reset();
  // Display the code in the session TopBar on the joiner side too — the
  // backend's `session:code` event only fires for the allocator.
  code.set(codeStr);
  appState.set('connecting');
  await invoke('start_session', { mode: 'recv', code: codeStr });
}

export async function sendText(content) {
  await invoke('send_text', { content });
}

export async function sendFile(path) {
  await invoke('send_file', { path });
}

export async function acceptFile(id) {
  await invoke('accept_file', { id });
}

export async function rejectFile(id, reason) {
  await invoke('reject_file', { id, reason });
}

export async function cancelFile(id) {
  await invoke('cancel_file', { id });
}

export async function closeSession() {
  await invoke('close_session');
}
