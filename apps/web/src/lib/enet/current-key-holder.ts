// EXP-1459: this tab's KeyHolder (use-key-holder.ts sets it once the install
// is known). Kept apart from the hook so pure adapters (lib/session-tree.ts)
// read it without pulling the tRPC client in.
//
// READERS take `currentKeySource()`: the holder, else the keys cached on the
// last run (loaded before the install is resolved, so a cold start opens
// sealed content at once, `coldStart` in fixtures/sealed-content.json).
// WRITERS take `currentKeyHolder()` (sealing needs the install).

import type { CachedKeys, KeyHolder } from "./key-holder"

let holder: KeyHolder | undefined
let cached: CachedKeys | undefined
let version = 0
const listeners = new Set<() => void>()

export function currentKeyHolder(): KeyHolder | undefined {
  return holder
}

/** Where readers open sealed content with: the holder, else the cache. */
export function currentKeySource(): Pick<KeyHolder, `keysFor` | `request`> | undefined {
  return holder ?? cached
}

export function setCurrentKeyHolder(next: KeyHolder | undefined) {
  holder = next
  emitKeyChange()
}

/** The cached keys readers use until a holder runs (`undefined` clears). */
export function setCachedKeys(next: CachedKeys | undefined) {
  if (cached === next) return
  cached = next
  emitKeyChange()
}

/** Moves on every key change (a holder, a cache, a key). */
export function keyVersion(): number {
  return version
}

/** Something about this tab's keys changed (a holder, a key). */
export function emitKeyChange() {
  version++
  for (const listener of listeners) listener()
}

export function subscribeKeyChange(listener: () => void): () => void {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

let kick: (() => void) | undefined

/** use-key-holder.ts: looks for this browser's install again at once. */
export function setHolderKick(next: (() => void) | undefined) {
  kick = next
}

/** #EDE-97 (sealed-content.json `clearTree`, `holder: late`): a writer
 *  without a holder waits for one (asking for the install at once) up to
 *  `ms`; undefined = none came, the send fails rather than go in clear. */
export function awaitKeyHolder(ms: number): Promise<KeyHolder | undefined> {
  if (holder) return Promise.resolve(holder)
  kick?.()
  return new Promise((resolve) => {
    const done = (value: KeyHolder | undefined) => {
      clearTimeout(timer)
      unsubscribe()
      resolve(value)
    }
    const timer = setTimeout(() => done(holder), ms)
    const unsubscribe = subscribeKeyChange(() => {
      if (holder) done(holder)
    })
  })
}
