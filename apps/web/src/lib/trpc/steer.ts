import { z } from "zod"
import { router, authedProcedure } from "@/lib/trpc"
import { eq } from "drizzle-orm"
import { sessions } from "@/db/schema"
import { getSteerRelayConfig, mintSteerTicket } from "@/lib/steer"
import { loadSessionForUser, sessionDeviceOwner } from "@/lib/sessions/access"

// The steer relay (masterplan §3.5), VAPP-43: device presence, control and
// eTalk calls, and NO session content. A device holds one CONTROL socket
// (presence; `check_in`, `kill`, `interrupt` and `compact_request` routed
// down it from the server's admin calls); every viewer reads a session's
// activity and steers it over eNet from the device that runs it
// (fixtures/steer-enet.json). EXP-1291: starts, messages, Stop and End are
// `sessions.*` writes on the DB (the bus). The web app is the only holder of
// STEER_RELAY_SECRET: it mints the short-lived HS256 relay tickets (control
// here, the call pair in `voice.*`). STEER_RELAY_URL unset ⇒ the subsystem
// reports disabled and every proc degrades gracefully.
//
// #EXP-1525: the relay is back as eNet's FALLBACK carrier, ciphertext only
// (fixtures/steer-enet.json `fallback`): `tunnelTicket` mints a viewer's
// `tunnel` ticket to the host device of one session. eNet stays primary; a
// viewer dials this only when eNet stalls, has no feed or drops.

const mintTicketInput = z.discriminatedUnion(`kind`, [
  // Device presence + control socket (account-scoped, no session).
  z.object({ kind: z.literal(`control`) }),
])

export const steerRouter = router({
  // Whether the relay is available on this instance — enabled iff BOTH
  // STEER_RELAY_URL and STEER_RELAY_SECRET are set. No relay round-trip.
  config: authedProcedure.query(() => {
    const config = getSteerRelayConfig()
    return { enabled: config !== null, relayUrl: config?.url ?? null }
  }),

  // Mint a short-lived relay ticket (60s connect window) + the full ws(s)
  // dial URL. Relay disabled is a result, not an error.
  mintTicket: authedProcedure.input(mintTicketInput).mutation(({ ctx }) => {
    const config = getSteerRelayConfig()
    if (!config) return { disabled: true as const }
    return mintSteerTicket(config, { kind: `control`, userId: ctx.session.user.id })
  }),

  // #EXP-1525: a sealed tunnel ticket for one session. The visibility rule
  // (loadSessionForUser: a member, and shared or the starter) is the gate;
  // the tunnel is routed to the session's host device (its steer deviceId
  // under the host account). A keyless ROOT has no tunnel: the relay may
  // only ever carry envelopes sealed to the tree key. `unavailable` is a
  // result, never an error: the viewer stays on eNet.
  tunnelTicket: authedProcedure
    .input(z.object({ sessionId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const config = getSteerRelayConfig()
      if (!config) return { disabled: true as const }
      const userId = ctx.session.user.id
      const session = await loadSessionForUser(userId, input.sessionId, ctx.db)
      if (!session.deviceId) return { unavailable: true as const }
      const [root] = await ctx.db
        .select({ sealPublicKey: sessions.sealPublicKey })
        .from(sessions)
        .where(eq(sessions.id, session.rootId ?? session.id))
        .limit(1)
      if (!root?.sealPublicKey) return { unavailable: true as const }
      const deviceOwnerId = await sessionDeviceOwner(session, ctx.db)
      return mintSteerTicket(config, {
        kind: `tunnel`,
        userId,
        teamId: session.teamId,
        sessionId: session.id,
        deviceId: session.deviceId,
        deviceOwnerId,
      })
    }),
})
