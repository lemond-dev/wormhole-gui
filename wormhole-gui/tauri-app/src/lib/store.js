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
 *   connecting        → PAKE in progress (no SAS yet)
 *   sas               → SAS shown, awaiting both confirmations
 *   connected         → full session
 *   error             → terminal error; show ErrorScreen
 *   closed            → graceful close; show return-to-idle prompt
 */
export const appState = writable('idle');

// Allocator code (full string, e.g. "26-dinosaur-spaniel")
export const code = writable(null);

// SAS digits (4-char string e.g. "1234")
export const sas = writable(null);

// Whether local user has confirmed SAS (UI-side); used to show waiting spinner.
export const sasLocalConfirmed = writable(false);

// Last error from backend ({ code, message })
export const lastError = writable(null);

// Reason of close (string)
export const closeReason = writable(null);

// Timeline messages: array of
//   { kind: 'system' | 'text', side: 'self'|'peer', id, content, ts, status? }
export const messages = writable([]);

// Convenience: append a message
export function pushMessage(m) {
  messages.update((list) => [...list, m]);
}

export function reset() {
  appState.set('idle');
  code.set(null);
  sas.set(null);
  sasLocalConfirmed.set(false);
  lastError.set(null);
  closeReason.set(null);
  messages.set([]);
}
