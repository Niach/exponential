import { z } from "zod"
import { router, authedProcedure } from "@/lib/trpc"
import { getSteerRelayConfig, mintSteerTicket } from "@/lib/steer"

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
})
