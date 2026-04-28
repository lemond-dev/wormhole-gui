// Thin wrapper around Tauri's invoke + event APIs.
// Wires backend Evt enum (core/session.rs) into our Svelte stores.

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  appState,
  code,
  sas,
  sasLocalConfirmed,
  lastError,
  closeReason,
  pushMessage,
  reset,
} from './store.js';

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
    await listen('session:sas_ready', (e) => {
      sas.set(e.payload.sas);
      appState.set('sas');
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
      closeReason.set(e.payload.reason);
      // Decide which screen: on graceful close go back to idle; on error stay
      // on error screen. The reason string distinguishes them.
      const reason = e.payload.reason || '';
      if (reason === 'ok' || reason === 'PAKE failed (code wrong, or attacker)') {
        if (reason.includes('PAKE')) appState.set('error');
        else appState.set('closed');
      } else {
        // Any other reason is treated as an error.
        appState.set('error');
      }
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
  appState.set('connecting');
  await invoke('start_session', { mode: 'recv', code: codeStr });
}

export async function confirmSas(matches) {
  if (matches) sasLocalConfirmed.set(true);
  await invoke('confirm_sas', { matches });
}

export async function sendText(content) {
  await invoke('send_text', { content });
}

export async function closeSession() {
  await invoke('close_session');
}
