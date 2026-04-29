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
  messages.set([]);
}
