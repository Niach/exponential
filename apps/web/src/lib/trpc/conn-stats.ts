import { z } from "zod"
import { STEER_CONN_STATS } from "@exp/domain-contract/steer-enet"
import { router, authedProcedure, adminProcedure } from "@/lib/trpc"
import {
  CONN_STATS_WINDOWS,
  connStatsBuckets,
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
// `dashboard` = the admin console's Connections page.

const windowKeys = Object.keys(CONN_STATS_WINDOWS) as [ConnStatsWindow, ...ConnStatsWindow[]]

export const connStatsRouter = router({
  record: authedProcedure
    .input(z.object({ events: z.array(z.unknown()).max(STEER_CONN_STATS.batchMax) }).passthrough())
    .mutation(async ({ ctx, input }) => {
      if (input.events.length === 0) return { accepted: 0 }
      if (!getConnStatsRecordLimiter().tryTake(ctx.session.user.id).ok) return { accepted: 0 }
      return recordConnEvents(input.events)
    }),

  dashboard: adminProcedure
    .input(z.object({ window: z.enum(windowKeys) }))
    .query(async ({ input }) => {
      const { rows, capped } = await loadConnStatsRows(input.window)
      return { window: input.window, capped, ...connStatsBuckets(rows) }
    }),
})
