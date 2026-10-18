import { acquireSteerSession } from "@/lib/steer-session-store"
import { trpc } from "@/lib/trpc-client"

/** EXP-1342: Stop = `sessions.stop`; the device drops every unread message
 *  with the turn, so their text goes back to the composer (×4) and a dropped
 *  message never renders as a row. The text comes back at once (the
 *  STEER-WIRE rule); a REFUSED call re-takes it (the device still holds the
 *  queue, its next `queue` frame repaints the strip) and rethrows. */
export async function stopSessionTurn(
  sessionId: string,
  opts?: Parameters<typeof trpc.sessions.stop.mutate>[1]
): Promise<void> {
  const store = acquireSteerSession(sessionId)
  const draftBefore = store.getDraftSnapshot().text
  const tookQueue = store.noteStopped()
  try {
    await trpc.sessions.stop.mutate({ sessionId }, opts)
  } catch (error) {
    if (tookQueue) store.setDraftText(draftBefore)
    throw error
  }
}
