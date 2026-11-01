// @vitest-environment node
import { describe, expect, it } from "vitest"
import { inArray } from "drizzle-orm"
import fixture from "@exp/domain-contract/fixtures/steer-enet.json"
import { steerConnectionEvents } from "@/db/schema"
import { inRollback, probeRealDb } from "@/test/real-db"
import {
  connStatsBuckets,
  loadConnStatsRows,
  recordConnEvents,
  runConnStatsRetention,
} from "./steer-conn-stats"

// EXP-1530 against the REAL migrated Postgres, inside a rolled-back
// transaction: the contract's valid events land as rows (a retried flush adds
// nothing), the dashboard reads them back into the same numbers, and the
// retention pass deletes exactly the rows older than `retentionDays`. Every
// row is stamped at a `now` in 2000, so only this case's rows are that old.

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'idx_steer_connection_events_received') AS ok
`)
if (!ready.ok) console.info(`[steer-conn-stats.real-db] skipped: ${ready.reason}`)

const NOW = new Date(`2000-03-01T00:00:00.000Z`)
const DAY = 24 * 60 * 60 * 1000
const stats = fixture.connStats

describe.skipIf(!ready.ok)(`steer connection events (real db)`, () => {
  it(`stores valid events idempotently, reads them back and expires them after retentionDays`, async () => {
    await inRollback(async (tx) => {
      const tag = crypto.randomUUID().slice(0, 8)
      const events = stats.validCases.map((c) => ({ ...c.event, id: `${tag}-${c.event.id}` }))
      const ids = events.map((e) => e.id)
      const valid = stats.validCases.filter((c) => c.valid).length

      expect(await recordConnEvents(events, NOW, tx)).toEqual({ accepted: valid })
      expect(await recordConnEvents(events, NOW, tx)).toEqual({ accepted: valid })
      const stored = await tx.select().from(steerConnectionEvents).where(inArray(steerConnectionEvents.id, ids))
      expect(stored).toHaveLength(valid)

      // The dashboard's window read includes them.
      const { rows } = await loadConnStatsRows(`24h`, new Date(NOW.getTime() + 1000), tx)
      expect(connStatsBuckets(rows).total.views).toBeGreaterThan(0)

      // Retention: one day past the window deletes every row stamped at NOW.
      expect(await runConnStatsRetention(new Date(NOW.getTime() + (stats.retentionDays - 1) * DAY), tx)).toEqual({
        connEventsDeleted: 0,
      })
      const swept = await runConnStatsRetention(new Date(NOW.getTime() + (stats.retentionDays + 1) * DAY), tx)
      expect(swept.connEventsDeleted).toBeGreaterThanOrEqual(valid)
      expect(await tx.select().from(steerConnectionEvents).where(inArray(steerConnectionEvents.id, ids))).toHaveLength(0)
    })
  })
})
