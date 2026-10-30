// EXP-1459: this tab's KeyHolder (use-key-holder.ts sets it once the install
// is known). Kept apart from the hook so pure adapters (lib/session-tree.ts)
// read it without pulling the tRPC client in.

import type { KeyHolder } from "./key-holder"

let holder: KeyHolder | undefined
const listeners = new Set<() => void>()

export function currentKeyHolder(): KeyHolder | undefined {
  return holder
}

export function setCurrentKeyHolder(next: KeyHolder | undefined) {
  holder = next
  emitKeyChange()
}

/** Something about this tab's keys changed (a holder, a key). */
export function emitKeyChange() {
  for (const listener of listeners) listener()
}

export function subscribeKeyChange(listener: () => void): () => void {
  listeners.add(listener)
  return () => listeners.delete(listener)
}
