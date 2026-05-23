// Central state store: app state machine + chat timeline.
// State transitions are driven by Tauri events from the smol session thread.

import { writable, derived } from 'svelte/store';

/**
 * UI state machine. Mirrors core/session.rs but adds a few UI-only states
 * (idle, joiner-input).
 *
 *   idle              → no session running
 *   allocator-wait    → code emitted, awaiting peer (PAKE pending)
 *   joiner-input      → user typing the peer's code
 *   connecting        → PAKE in progress
 *   connected         → full session
 *   error             → terminal error; show ErrorScreen
 *   closed            → graceful close; show return-to-idle prompt
 */
export const appState = writable('idle');

// Allocator code (full string, e.g. "26-dinosaur-spaniel")
export const code = writable(null);

// Last error from backend ({ code, message })
export const lastError = writable(null);

// Reason of close (string)
export const closeReason = writable(null);

// Why the close-confirm modal is open: 'session' (log-out icon, keep window)
// or 'window' (X button, also destroy window after end). null = modal closed.
export const closeIntent = writable(null);

// Timeline messages: array of
//   { kind: 'system' | 'text' | 'file', side: 'self'|'peer', id, ... }
//
// File entries carry a `state` field:
//   'offer'      — peer sent us an offer; awaiting accept/reject
//   'sending'    — outgoing transit in progress
//   'receiving'  — incoming transit in progress
//   'sent'       — outgoing complete (peer confirmed)
//   'received'   — incoming complete (saved to disk)
//   'cancelled'  — cancelled by self/peer
//   'failed'     — error during transfer
//   'awaiting'   — outgoing offer sent, peer hasn't accepted yet
export const messages = writable([]);

// Convenience: append a message
export function pushMessage(m) {
  messages.update((list) => [...list, m]);
}

// In-place update message by id (used heavily for file cards).
export function updateMessage(id, patch) {
  messages.update((list) =>
    list.map((m) => (m.id === id ? { ...m, ...patch } : m))
  );
}

export function reset() {
  appState.set('idle');
  code.set(null);
  lastError.set(null);
  closeReason.set(null);
  closeIntent.set(null);
  messages.set([]);
}

// ───── Auto-update state ─────
//
// Single object store for the banner so the UI can branch on status:
//   null                                                  → banner hidden
//   { status: 'available', version, notes, pubDate, form } → "v0.x.y 可用"
//   { status: 'downloading', downloaded, total, version } → progress bar
//   { status: 'error', message, version? }                → error + retry
//
// `pubDate` is the manifest's pub_date string verbatim; the UI formats it.
export const updateState = writable(null);

// "稍后" was clicked this session — in-memory only, so the next launch
// will prompt again. Acts as a guard against re-triggering checks from
// the silent startup path or other code paths.
export const updateDismissedThisSession = writable(false);
