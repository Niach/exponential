import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { STEER_CONN_STATS } from "@exp/domain-contract/steer-enet"
import { router, procedure, adminProcedure } from "@/lib/trpc"
import { SCOPED_API_KEY_MESSAGE } from "@/lib/auth/api-key-kind"
import {
  CONN_STATS_WINDOWS,
  connStatsBuckets,
  countConnStatsCall,
  getConnStatsRecordLimiter,
  loadConnStatsRows,
  recordConnEvents,
  type ConnStatsWindow,
} from "@/lib/steer-conn-stats"

// EXP-1530: P2P connection analytics (fixtures/steer-enet.json `connStats`).
// `record` is what every viewer and serving device flushes its buffered eCore
// envelopes to; each event is validated against the contract and an invalid
// one is dropped silently (clients never block or error on this). Over the
// per-user rate limit the batch is dropped too ({accepted: 0}), never an
// error a client could retry into a storm. No user id is stored.
// EXP-1593: every call is counted (lib/steer-conn-stats.ts tally, one log
// line a minute), a signed-out or off-shape one included: the auth and the
// input check run in here, not in a middleware or the input parser that
// would throw before the count.
// `dashboard` = the admin console's Connections page.

const recordInput = z.object({ events: z.array(z.unknown()).max(STEER_CONN_STATS.batchMax) }).passthrough()

const windowKeys = Object.keys(CONN_STATS_WINDOWS) as [ConnStatsWindow, ...ConnStatsWindow[]]

export const connStatsRouter = router({
  record: procedure.input(z.unknown()).mutation(async ({ ctx, input }) => {
    const userId = ctx.session?.user?.id
    if (!userId) {
      countConnStatsCall({ outcome: `rejected`, reason: `unauthorized` })
      throw new TRPCError({
        code: `UNAUTHORIZED`,
        ...(ctx.scopedKey ? { message: SCOPED_API_KEY_MESSAGE } : {}),
      })
    }
    const parsed = recordInput.safeParse(input)
    if (!parsed.success) {
      countConnStatsCall({ outcome: `invalid`, dropped: 0 })
      throw new TRPCError({ code: `BAD_REQUEST`, message: `Invalid connStats batch` })
    }
    const { events } = parsed.data
    if (events.length === 0) {
      countConnStatsCall({ outcome: `invalid`, dropped: 0 })
      return { accepted: 0 }
    }
    if (!getConnStatsRecordLimiter().tryTake(userId).ok) {
      countConnStatsCall({ outcome: `rejected`, reason: `rate_limited` })
      return { accepted: 0 }
    }
    return recordConnEvents(events)
  }),

  dashboard: adminProcedure
    .input(z.object({ window: z.enum(windowKeys) }))
    .query(async ({ input }) => {
      const { rows, capped } = await loadConnStatsRows(input.window)
      return { window: input.window, capped, ...connStatsBuckets(rows) }
    }),
})
