import { acquireSteerSession } from "@/lib/steer-session-store"
import { trpc } from "@/lib/trpc-client"


/** EXP-1342: a composer that drains the store's draft into a field of its
 *  own (`takeDraftText`, the web `SessionComposer`) says how to give a
 *  Stop's text back when the Stop was refused. `held` = the exact text the
 *  store handed out; true = the field still showed it untouched (nothing
 *  typed since) and took it out. */
export type StopRetraction = (held: string) => boolean

const retractions = new Map<string, Set<StopRetraction>>()

/** Registers a retraction for `sessionId`; returns the unsubscribe. */
export function onStopRetraction(
  sessionId: string,
  retract: StopRetraction
): () => void {
  let set = retractions.get(sessionId)
  if (!set) {
    set = new Set()
    retractions.set(sessionId, set)
  }
  set.add(retract)
  return () => {
    set.delete(retract)
    if (set.size === 0) retractions.delete(sessionId)
  }
}
/** EXP-1342: Stop = `sessions.stop`; the device drops every unread message
 *  with the turn, so their text goes back to the composer (×4) and a dropped
 *  message never renders as a row. The text comes back at once (the
 *  STEER-WIRE rule); a REFUSED call re-takes it (the device still holds the
 *  queue, its next `queue` frame repaints the strip — the text must not sit
 *  in a field too, or it is sent twice) and rethrows. */
export async function stopSessionTurn(
  sessionId: string,
  opts?: Parameters<typeof trpc.sessions.stop.mutate>[1]
): Promise<void> {
  const store = acquireSteerSession(sessionId)
  const draftBefore = store.getDraftSnapshot().text
  const tookQueue = store.noteStopped()
  // What `noteStopped` put in the draft: the draft in progress with the
  // queue's text after it (`mergeHeldIntoDraft`).
  const held = tookQueue ? store.getDraftSnapshot().text : ``
  try {
    await trpc.sessions.stop.mutate({ sessionId }, opts)
  } catch (error) {
    if (tookQueue) retractHeld(sessionId, store, held, draftBefore)
    throw error
  }
}

/** The refused Stop's text goes back where it came from: out of the store's
 *  draft when no composer drained it yet, else out of the composer that
 *  did — unless something was typed over it meanwhile (then it stays: a
 *  person's edit is never clobbered). The draft in progress before the Stop
 *  returns through the store (the composer drains it again). */
function retractHeld(
  sessionId: string,
  store: ReturnType<typeof acquireSteerSession>,
  held: string,
  draftBefore: string
): void {
  if (store.getDraftSnapshot().text === held) {
    store.setDraftText(draftBefore)
    return
  }
  let retracted = false
  for (const retract of retractions.get(sessionId) ?? []) {
    if (retract(held)) retracted = true
  }
  if (retracted && draftBefore) store.setDraftText(draftBefore)
}
