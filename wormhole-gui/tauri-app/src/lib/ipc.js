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
  closeIntent,
  pushMessage,
  updateMessage,
  reset,
  updateState,
  updateDismissedThisSession,
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

  unlistenFns.push(
    await listen('window:close_requested', () => {
      if (get(appState) === 'connected') {
        closeIntent.set('window');
      } else {
        invoke('end_and_close');
      }
    })
  );

  // ── File transfer events ──
  unlistenFns.push(
    await listen('msg:file_offer', (e) => {
      const { id, name, size, mime } = e.payload;
      pushMessage({
        kind: 'file',
        side: 'peer',
        id,
        name,
        size,
        mime,
        state: 'offer',
        bytes: 0,
        auto_accepted: false,
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

  // ── Updater progress ──
  // The backend emits this roughly every 64 KiB during a download; we
  // store the running totals and let the banner re-render reactively.
  unlistenFns.push(
    await listen('updater:progress', (e) => {
      const { downloaded, total } = e.payload;
      updateState.update((s) => {
        if (!s || s.status !== 'downloading') return s;
        return { ...s, downloaded, total };
      });
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

export async function endAndCloseWindow() {
  await invoke('end_and_close');
}

export async function revealInFolder(path) {
  await invoke('reveal_in_folder', { path });
}

export async function getConfig() {
  return await invoke('get_config');
}

export async function setConfig(newConfig) {
  // Tauri 2 default rename_all is camelCase, so the Rust arg `new_config`
  // is exposed to JS as `newConfig`.
  await invoke('set_config', { newConfig });
}

export async function pickDownloadDir() {
  return await invoke('pick_download_dir');
}

// ───── Auto-update ─────
//
// `checkUpdate()` returns the same shape regardless of deployment form
// (the backend dispatches by form). `null` means up-to-date.
//
// The returned object has: { version, notes, pubDate, form } where `form`
// is "installed" or "portable" — the banner uses it only to phrase the
// confirmation copy correctly.
export async function checkUpdate() {
  // Backend serializes the Rust struct field `pub_date` as snake_case;
  // normalize to camelCase here so the UI doesn't have to know.
  const raw = await invoke('check_update');
  if (!raw) return null;
  return {
    version: raw.version,
    notes: raw.notes,
    pubDate: raw.pub_date,
    form: raw.form,
  };
}

/**
 * Trigger the download + apply. Resolves only on error — on success the
 * current process exits and the new version restarts the app, so the JS
 * never gets to see the resolution.
 */
export async function applyUpdate() {
  await invoke('apply_update');
}

/**
 * Shared startup / manual update check. Updates `updateState` if a newer
 * version is found; no-op otherwise.
 *
 * @param {{ silent?: boolean }} opts
 *   silent=true  → swallow errors and skip if the user dismissed this
 *                  session; used by the 2-second post-startup probe
 *   silent=false → surface errors as an update-banner error and ignore
 *                  the per-session dismissal flag; used by the Settings
 *                  "检查更新" button
 */
export async function triggerUpdateCheck({ silent = false } = {}) {
  if (silent && get(updateDismissedThisSession)) return;
  // Don't start a second check if a banner / progress / error is already
  // visible — that just confuses the user.
  if (get(updateState)) return;
  try {
    const info = await checkUpdate();
    if (info) {
      updateState.set({
        status: 'available',
        version: info.version,
        notes: info.notes,
        pubDate: info.pubDate,
        form: info.form,
      });
    }
  } catch (err) {
    if (!silent) {
      updateState.set({
        status: 'error',
        message: `${err}`,
      });
    }
    // Silent mode: swallow. The user can retry from Settings.
  }
}
