import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { router, authedProcedure } from "@/lib/trpc"
import { getSteerRelayConfig, mintSteerTicket, viewerHistoryClaims } from "@/lib/steer"
import { loadSessionForUser, sessionDeviceOwner } from "@/lib/sessions/access"

// The live layer of a session (masterplan §3.5). EXP-1291: starts, messages,
// Stop and End are `sessions.*` writes on the DB (the bus); the steer relay
// keeps only the LIVE view — a session's activity feed and terminal — and
// device presence. The web app is the only holder of STEER_RELAY_SECRET: it
// mints short-lived HS256 relay tickets after checking authorization against
// `sessions` with the ONE visibility rule (shared → every member may VIEW;
// private → its starter), while only the device's account (the starter or
// the shared box's host) may PUBLISH. STEER_RELAY_URL unset ⇒ the subsystem
// reports disabled and every proc degrades gracefully.

const mintTicketInput = z.discriminatedUnion(`kind`, [
  // Device presence socket (no session yet).
  z.object({ kind: z.literal(`control`) }),
  // The device publishing a session it runs.
  z.object({ kind: z.literal(`publisher`), sessionId: z.string().uuid() }),
  // A live view of a session.
  z.object({ kind: z.literal(`viewer`), sessionId: z.string().uuid() }),
])

export const steerRouter = router({
  // Whether the live layer is available on this instance — enabled iff BOTH
  // STEER_RELAY_URL and STEER_RELAY_SECRET are set. No relay round-trip.
  config: authedProcedure.query(() => {
    const config = getSteerRelayConfig()
    return { enabled: config !== null, relayUrl: config?.url ?? null }
  }),

  // Mint a short-lived relay ticket (60s connect window) + the full ws(s)
  // dial URL. Relay disabled is a result, not an error.
  mintTicket: authedProcedure.input(mintTicketInput).mutation(async ({ ctx, input }) => {
    const config = getSteerRelayConfig()
    if (!config) return { disabled: true as const }
    const userId = ctx.session.user.id

    if (input.kind === `control`) {
      return mintSteerTicket(config, { kind: `control`, userId })
    }

    // Visibility first (member AND shared-or-starter): NOT_FOUND otherwise.
    const session = await loadSessionForUser(userId, input.sessionId)
    const deviceOwner = await sessionDeviceOwner(session)

    if (input.kind === `publisher`) {
      // Only the account the device runs under publishes (an ended session
      // too: the device republishes its stored transcript through it).
      if (deviceOwner !== userId) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `Only the device running this session can publish it`,
        })
      }
      return mintSteerTicket(config, {
        kind: `publisher`,
        userId,
        teamId: session.teamId,
        sessionId: session.id,
      })
    }

    return mintSteerTicket(config, {
      kind: `viewer`,
      userId,
      teamId: session.teamId,
      sessionId: session.id,
      ...viewerHistoryClaims(session, deviceOwner),
    })
  }),
})
