// @vitest-environment node
import { beforeEach, describe, expect, it, vi } from "vitest"
import { PgDialect } from "drizzle-orm/pg-core"
import fixture from "@exp/domain-contract/fixtures/steer-enet.json"

// EXP-1530: connStats.record validates every posted envelope with the
// contract's steerConnEventValid (fixtures/steer-enet.json `connStats.validCases`),
// drops the invalid ones silently and stores the rest without any user column;
// the dashboard's buckets run steerConnAggregate, so they agree with
// `aggregateCases`; the sweep deletes rows older than `retentionDays`.

const inserted: Record<string, unknown>[][] = []
const deletes: unknown[] = []
vi.mock(`@/db/connection`, () => ({
  db: {
    insert: () => ({
      values: (rows: Record<string, unknown>[]) => ({
        onConflictDoNothing: async () => {
          inserted.push(rows)
        },
      }),
    }),
    delete: () => ({
      where: (cond: unknown) => ({
        returning: async () => {
          deletes.push(cond)
          return [{ id: `x` }, { id: `y` }]
        },
      }),
    }),
  },
}))

import { connStatsRouter } from "@/lib/trpc/conn-stats"
import {
  connStatsBuckets,
  connStatsTallyLine,
  countConnStatsCall,
  isConnEventExpired,
  resetConnStatsRecordLimiterForTest,
  resetConnStatsTallyForTest,
  takeConnStatsTally,
  runConnStatsRetention,
  STEER_CONN_RETENTION_MS,
  type ConnStatsDbRow,
} from "@/lib/steer-conn-stats"

const stats = fixture.connStats

function caller(userId = `u1`) {
  return connStatsRouter.createCaller({
    session: { user: { id: userId, name: `U`, email: `u@example.com` }, session: { id: `s` } },
    db: {},
    request: new Request(`http://localhost/`),
  } as never)
}

beforeEach(() => {
  inserted.length = 0
  deletes.length = 0
  resetConnStatsRecordLimiterForTest()
  resetConnStatsTallyForTest()
})

describe(`connStats.record`, () => {
  for (const c of stats.validCases) {
    it(`${c.valid ? `stores` : `drops`}: ${c.name}`, async () => {
      const res = await caller().record({ events: [c.event] })
      expect(res).toEqual({ accepted: c.valid ? 1 : 0 })
      expect(inserted.flat()).toHaveLength(c.valid ? 1 : 0)
    })
  }

  it(`stores the valid ones of a mixed batch as rows without any user, team, session or device column`, async () => {
    const events = stats.validCases.map((c) => c.event)
    const res = await caller().record({ events })
    const valid = stats.validCases.filter((c) => c.valid).length
    expect(res).toEqual({ accepted: valid })
    const rows = inserted.flat()
    expect(rows).toHaveLength(valid)
    for (const row of rows) {
      expect(Object.keys(row).filter((k) => /user|team|session|device/i.test(k))).toEqual([])
    }
    const first = stats.validCases[0]!.event
    expect(rows[0]).toMatchObject({
      id: first.id,
      at: new Date(first.at),
      kind: first.kind,
      phase: first.data.phase,
      platform: first.data.platform,
      version: first.data.version,
      outcome: first.data.outcome,
      path: first.data.path,
      firstFrameMs: first.data.firstFrameMs,
      errorClass: null,
    })
  })

  it(`dedupes a repeated envelope id within one batch`, async () => {
    const e = stats.validCases.find((c) => c.valid)!.event
    expect(await caller().record({ events: [e, e] })).toEqual({ accepted: 1 })
  })

  it(`refuses more than batchMax events and accepts an empty batch`, async () => {
    const e = stats.validCases[0]!.event
    await expect(caller().record({ events: Array(stats.batchMax + 1).fill(e) })).rejects.toThrow()
    expect(await caller().record({ events: [] })).toEqual({ accepted: 0 })
    expect(inserted).toHaveLength(0)
  })

  it(`drops (never errors) once a user is over the rate limit, per user`, async () => {
    const e = stats.validCases[0]!.event
    let accepted = 0
    for (let i = 0; i < 40; i++) accepted += (await caller(`u1`).record({ events: [{ ...e, id: `r${i}` }] })).accepted
    expect(accepted).toBe(30)
    expect(await caller(`u2`).record({ events: [e] })).toEqual({ accepted: 1 })
  })

  it(`requires a signed-in caller`, async () => {
    const anon = connStatsRouter.createCaller({ session: null, db: {}, request: new Request(`http://localhost/`) } as never)
    await expect(anon.record({ events: [] })).rejects.toThrow()
  })
})

// EXP-1593: every client drops a failed post silently, so prod sat at 0
// rows with no way to tell "no calls" from "every call refused". Each call is
// counted (accepted / rejected / invalid / failed) and logged once a minute.
describe(`connStats.record tally`, () => {
  it(`counts every outcome, the signed-out and over-limit calls included`, async () => {
    const e = stats.validCases.find((c) => c.valid)!.event
    const bad = stats.validCases.find((c) => !c.valid)!.event
    await caller().record({ events: [e, bad] })
    await caller().record({ events: [bad] })
    await caller().record({ events: [] })
    await expect(caller().record({ events: Array(stats.batchMax + 1).fill(e) })).rejects.toThrow()
    const anon = connStatsRouter.createCaller({ session: null, db: {}, request: new Request(`http://localhost/`) } as never)
    await expect(anon.record({ events: [e] })).rejects.toMatchObject({ code: `UNAUTHORIZED` })
    for (let i = 0; i < 31; i++) await caller(`u9`).record({ events: [{ ...e, id: `t${i}` }] })
    expect(takeConnStatsTally()).toEqual({
      accepted: 31,
      rejected: 2,
      invalid: 3,
      unauthorized: 1,
      rateLimited: 1,
      failed: 0,
      stored: 31,
      dropped: 2,
    })
    expect(takeConnStatsTally().accepted).toBe(0)
  })

  it(`logs one content-free line a minute, none for a minute without calls`, async () => {
    vi.useFakeTimers()
    try {
      const lines: string[] = []
      countConnStatsCall({ outcome: `accepted`, stored: 2, dropped: 1 }, (l) => lines.push(l))
      countConnStatsCall({ outcome: `rejected`, reason: `unauthorized` })
      vi.advanceTimersByTime(60_000)
      expect(lines).toEqual([
        `[conn-stats] record calls in the last minute: accepted 1, rejected 1 (unauthorized 1, rate-limited 0), invalid 0, failed 0; events stored 2, dropped 1`,
      ])
      vi.advanceTimersByTime(60_000)
      expect(lines).toHaveLength(1)
      expect(connStatsTallyLine(takeConnStatsTally())).toBeNull()
    } finally {
      vi.useRealTimers()
    }
  })
})

describe(`dashboard buckets`, () => {
  for (const c of stats.aggregateCases) {
    it(`agrees with steerConnAggregate: ${c.name}`, () => {
      const rows = c.rows.map((r) => ({ platform: `web`, version: `1.0.0`, ...r }) as ConnStatsDbRow)
      const { total, platforms, versions } = connStatsBuckets(rows)
      const expected = {
        views: c.views,
        successRate: c.successRate,
        p50FirstFrameMs: c.p50FirstFrameMs,
        p95FirstFrameMs: c.p95FirstFrameMs,
        fallbackRate: c.fallbackRate,
      }
      expect(total).toMatchObject(expected)
      if (rows.length > 0) {
        expect(platforms[0]).toMatchObject({ platform: `web`, version: ``, ...expected })
        expect(versions[0]).toMatchObject({ platform: `web`, version: `1.0.0`, ...expected })
      }
    })
  }

  it(`splits per platform and version, busiest first, with the top error classes`, () => {
    const ok = { phase: `settled`, outcome: `ok`, path: `direct`, firstFrameMs: 300 } as const
    const rows: ConnStatsDbRow[] = [
      { platform: `ios`, version: `0.14.80`, ...ok },
      { platform: `ios`, version: `0.14.80`, ...ok },
      { platform: `ios`, version: `0.14.79`, phase: `settled`, outcome: `failed`, errorClass: `join_timeout` },
      { platform: `web`, version: `0.20.8`, ...ok },
      {
        platform: `ios`,
        version: `0.14.80`,
        phase: `ended`,
        outcome: `ok`,
        path: `direct`,
        fallbacks: 1,
        failures: { stale: 2, closed: 1 },
      },
    ]
    const { total, platforms, versions } = connStatsBuckets(rows)
    expect(total.views).toBe(4)
    expect(platforms.map((b) => [b.platform, b.views])).toEqual([[`ios`, 3], [`web`, 1]])
    expect(versions.map((b) => [b.platform, b.version, b.views])).toEqual([
      [`ios`, `0.14.80`, 2],
      [`ios`, `0.14.79`, 1],
      [`web`, `0.20.8`, 1],
    ])
    expect(platforms[0]!.topErrors).toEqual([
      { errorClass: `stale`, count: 2 },
      { errorClass: `closed`, count: 1 },
      { errorClass: `join_timeout`, count: 1 },
    ])
    expect(versions[0]!.fallbackRate).toBe(0.5)
  })
})

describe(`retention`, () => {
  const now = new Date(`2026-11-01T12:00:00Z`)

  it(`is retentionDays from the contract`, () => {
    expect(STEER_CONN_RETENTION_MS).toBe(stats.retentionDays * 24 * 60 * 60 * 1000)
    expect(isConnEventExpired(new Date(now.getTime() - STEER_CONN_RETENTION_MS), now)).toBe(true)
    expect(isConnEventExpired(new Date(now.getTime() - STEER_CONN_RETENTION_MS + 1), now)).toBe(false)
  })

  it(`deletes on received_at <= now - retentionDays`, async () => {
    expect(await runConnStatsRetention(now)).toEqual({ connEventsDeleted: 2 })
    const q = new PgDialect({ casing: `snake_case` }).sqlToQuery(deletes[0] as never)
    expect(q.sql).toContain(`"steer_connection_events"."received_at" <= $1`)
    expect(q.params).toEqual([new Date(now.getTime() - STEER_CONN_RETENTION_MS).toISOString()])
  })
})
