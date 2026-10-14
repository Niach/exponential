// EXP-894: what a WORK TAB remembers across a switch. Switching tabs on md+
// is a route change, so the issue route (and its comment composer) unmounts
// and a half-typed comment used to be gone on the way back. Keeping hidden
// route trees mounted would keep every tab's live queries running, so the
// tabs instead stash the little view state that matters here — composer
// drafts, which reply is open, the scroll offset — and the remounted view
// reads it back.
//
// In memory only (a reload starts clean, like the composer always did),
// grouped per OWNER = the work tab's identity (`issue:<id>` / `session:<id>`,
// `tabKey`), forgotten when that tab closes (`use-work-tabs.ts`) and bounded
// by a least-recently-used cap for views that never had a tab. Pure: no
// React, no DOM — the hooks wrap it.

import { tabSessionId, type WorkTab } from "@/lib/work-tabs"

/** Owners kept at most — far past any real strip, small enough to never
 *  matter. The least recently touched owner goes first. */
export const WORK_TAB_MEMORY_CAP = 64

type Slots = Map<string, unknown>

const owners = new Map<string, Slots>()

/** The memory owner of an issue view. */
export function issueMemoryOwner(issueId: string): string {
  return `issue:${issueId}`
}

/** The memory owner of a session view (a thread, an agent's channel, one
 *  raw session: each is a `sessions` row). */
export function sessionMemoryOwner(sessionId: string): string {
  return `session:${sessionId}`
}

function touch(owner: string, create: boolean): Slots | undefined {
  let slots = owners.get(owner)
  if (slots) {
    // Re-insert: Map iteration order is the LRU order.
    owners.delete(owner)
    owners.set(owner, slots)
    return slots
  }
  if (!create) return undefined
  slots = new Map()
  owners.set(owner, slots)
  while (owners.size > WORK_TAB_MEMORY_CAP) {
    const oldest = owners.keys().next().value
    if (oldest === undefined) break
    owners.delete(oldest)
  }
  return slots
}

export function readTabMemory<T>(owner: string, slot: string): T | undefined {
  return touch(owner, false)?.get(slot) as T | undefined
}

/** Store a slot. `undefined`, `null` and the empty string CLEAR it — an empty
 *  draft is no draft, and an owner with no slots left is dropped. */
export function writeTabMemory(
  owner: string,
  slot: string,
  value: unknown
): void {
  if (value === undefined || value === null || value === ``) {
    const slots = owners.get(owner)
    if (!slots) return
    slots.delete(slot)
    if (slots.size === 0) owners.delete(owner)
    return
  }
  touch(owner, true)!.set(slot, value)
}

/** Forget everything an owner remembered. */
export function forgetTabMemory(owner: string): void {
  owners.delete(owner)
}

/** The owners a tab's memory lives under: its issue or its session row. */
export function tabMemoryOwners(tab: WorkTab): string[] {
  const sessionId = tabSessionId(tab)
  if (sessionId) return [sessionMemoryOwner(sessionId)]
  if (tab.kind === `issue`) return [issueMemoryOwner(tab.issueId)]
  // EXP-1255: the New session tab owns no work item.
  return []
}

/** Forget the memory of every tab in `before` that is gone from `after` —
 *  called on each tab-state transition, so closing a tab (or pruning one)
 *  drops its drafts. An owner still named by a surviving tab is kept. */
export function forgetClosedTabs(
  before: readonly WorkTab[],
  after: readonly WorkTab[]
): void {
  const surviving = new Set(after.flatMap(tabMemoryOwners))
  for (const tab of before) {
    for (const owner of tabMemoryOwners(tab)) {
      if (!surviving.has(owner)) forgetTabMemory(owner)
    }
  }
}

/** Test seam. */
export function resetTabMemory(): void {
  owners.clear()
}

/** Test seam: the owners, oldest first. */
export function tabMemoryOwnersForTest(): string[] {
  return [...owners.keys()]
}
