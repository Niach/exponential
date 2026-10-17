import { acquireSteerSession } from "@/lib/steer-session-store"
import { trpc } from "@/lib/trpc-client"

/** EXP-1342: Stop = `sessions.stop`; the device drops every unread message
 *  with the turn, so their text goes back to the composer (×4) and a dropped
 *  message never renders as a row. The text comes back at once (the
 *  STEER-WIRE rule), the call failing leaves it in the draft. */
export async function stopSessionTurn(
  sessionId: string,
  opts?: Parameters<typeof trpc.sessions.stop.mutate>[1]
): Promise<void> {
  acquireSteerSession(sessionId).noteStopped()
  await trpc.sessions.stop.mutate({ sessionId }, opts)
}
