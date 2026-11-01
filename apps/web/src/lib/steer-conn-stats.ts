// EXP-1530: P2P connection analytics, the server half of
// fixtures/steer-enet.json `connStats`. Viewers and serving devices post eCore
// envelopes to tRPC `connStats.record`; each is validated with the contract's
// `steerConnEventValid` (invalid = dropped silently), stored in
// `steer_connection_events` (no user/team/session/device column) and deleted
// after `retentionDays` by the device-command sweep's hourly pass. The admin
// dashboard buckets rows per platform + version and runs the contract's own
// `steerConnAggregate`, so its numbers ARE the fixture's `aggregateCases`.

import { and, eq, gte, lte } from "drizzle-orm"
import {
  STEER_CONN_STATS,
  steerConnAggregate,
  steerConnEventValid,
  type SteerConnAggregate,
  type SteerConnEvent,
  type SteerConnRow,
} from "@exp/domain-contract/steer-enet"
import { db } from "@/db/connection"
import { steerConnectionEvents } from "@/db/schema"
import { TokenBucketLimiter } from "@/lib/widget/rate-limit"

type Executor = Pick<typeof db, `select` | `insert` | `delete`>

export const STEER_CONN_RETENTION_MS =
  STEER_CONN_STATS.retentionDays * 24 * 60 * 60 * 1000

// A client flushes <= batchMax events every flushMs (60 s) plus on background
// / page hide: 30 batches of burst, 240 an hour sustained per user is far
// above any honest client and caps a hostile one. Keyed by user id in memory
// only; the user is never stored.
let recordLimiter: TokenBucketLimiter | null = null
export function getConnStatsRecordLimiter(): TokenBucketLimiter {
  recordLimiter ??= new TokenBucketLimiter({ capacity: 30, refillPerHour: 240 })
  return recordLimiter
}
export function resetConnStatsRecordLimiterForTest(): void {
  recordLimiter = null
}

/** Only the events the contract accepts, deduped by envelope id. */
export function acceptedConnEvents(events: unknown[]): SteerConnEvent[] {
  const seen = new Set<string>()
  const out: SteerConnEvent[] = []
  for (const event of events.slice(0, STEER_CONN_STATS.batchMax)) {
    if (!steerConnEventValid(event) || seen.has(event.id)) continue
    seen.add(event.id)
    out.push(event)
  }
  return out
}

export function connEventRow(
  event: SteerConnEvent,
  receivedAt: Date
): typeof steerConnectionEvents.$inferInsert {
  const d = event.data
  return {
    id: event.id,
    at: new Date(event.at),
    receivedAt,
    kind: event.kind,
    phase: d.phase,
    platform: d.platform,
    version: d.version,
    outcome: d.outcome,
    path: d.path ?? null,
    firstFrameMs: d.firstFrameMs ?? null,
    errorClass: d.errorClass ?? null,
    fallbacks: d.fallbacks ?? null,
    reconnects: d.reconnects ?? null,
    durationMs: d.durationMs ?? null,
    failures: d.failures ?? null,
  }
}

/** Store a batch; a retried flush (same envelope ids) inserts nothing new. */
export async function recordConnEvents(
  events: unknown[],
  now: Date = new Date(),
  executor: Executor = db
): Promise<{ accepted: number }> {
  const accepted = acceptedConnEvents(events)
  if (accepted.length === 0) return { accepted: 0 }
  await executor
    .insert(steerConnectionEvents)
    .values(accepted.map((e) => connEventRow(e, now)))
    .onConflictDoNothing()
  return { accepted: accepted.length }
}

// ── Retention ────────────────────────────────────────────────────────────────

/** Pure: a row is due once it was received retentionDays ago (the delete
 *  below applies the same cutoff server-side). */
export function isConnEventExpired(receivedAt: Date, now: Date = new Date()): boolean {
  return receivedAt.getTime() + STEER_CONN_RETENTION_MS <= now.getTime()
}

/** One retention pass: every row received before now - retentionDays. */
export async function runConnStatsRetention(
  now: Date = new Date(),
  executor: Executor = db
): Promise<{ connEventsDeleted: number }> {
  const cutoff = new Date(now.getTime() - STEER_CONN_RETENTION_MS)
  const deleted = await executor
    .delete(steerConnectionEvents)
    .where(lte(steerConnectionEvents.receivedAt, cutoff))
    .returning({ id: steerConnectionEvents.id })
  return { connEventsDeleted: deleted.length }
}

// ── Dashboard ────────────────────────────────────────────────────────────────

export const CONN_STATS_WINDOWS = { "24h": 1, "7d": 7, "30d": 30 } as const
export type ConnStatsWindow = keyof typeof CONN_STATS_WINDOWS

export type ConnStatsDbRow = SteerConnRow & {
  platform: string
  version: string
  failures?: Record<string, number> | null
}

export type ConnStatsBucket = SteerConnAggregate & {
  platform: string
  version: string
  topErrors: { errorClass: string; count: number }[]
}

const TOP_ERRORS = 3

// Failed settles count their (last) errorClass; ended views add their whole
// failures map (an ended view's errorClass is already inside it).
function topErrors(rows: ConnStatsDbRow[]): ConnStatsBucket[`topErrors`] {
  const counts = new Map<string, number>()
  const add = (k: string, n: number) => counts.set(k, (counts.get(k) ?? 0) + n)
  for (const r of rows) {
    if (r.phase === `settled` && r.outcome === `failed` && r.errorClass) add(r.errorClass, 1)
    if (r.phase === `ended` && r.failures) {
      for (const [k, n] of Object.entries(r.failures)) add(k, n)
    }
  }
  return [...counts.entries()]
    .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .slice(0, TOP_ERRORS)
    .map(([errorClass, count]) => ({ errorClass, count }))
}

function bucket(platform: string, version: string, rows: ConnStatsDbRow[]): ConnStatsBucket {
  return { platform, version, ...steerConnAggregate(rows), topErrors: topErrors(rows) }
}

/** Pure: the overall line + one bucket per platform and per platform+version
 *  (busiest first). `version: ""` = every version of that platform. */
export function connStatsBuckets(rows: ConnStatsDbRow[]): {
  total: ConnStatsBucket
  platforms: ConnStatsBucket[]
  versions: ConnStatsBucket[]
} {
  const byPlatform = new Map<string, ConnStatsDbRow[]>()
  const byVersion = new Map<string, ConnStatsDbRow[]>()
  for (const r of rows) {
    byPlatform.set(r.platform, [...(byPlatform.get(r.platform) ?? []), r])
    const key = `${r.platform}\u0000${r.version}`
    const list = byVersion.get(key)
    if (list) list.push(r)
    else byVersion.set(key, [r])
  }
  const busiest = (a: ConnStatsBucket, b: ConnStatsBucket) =>
    b.views - a.views || a.platform.localeCompare(b.platform) || b.version.localeCompare(a.version)
  return {
    total: bucket(``, ``, rows),
    platforms: [...byPlatform.entries()].map(([p, list]) => bucket(p, ``, list)).sort(busiest),
    versions: [...byVersion.entries()]
      .map(([key, list]) => {
        const [p, v] = key.split(`\u0000`)
        return bucket(p ?? ``, v ?? ``, list)
      })
      .sort(busiest),
  }
}

// Bounds one dashboard read; at the target scale a 30-day window is far below.
const DASHBOARD_ROW_CAP = 500_000

/** The viewer rows (`steer.view`) received inside the window. */
export async function loadConnStatsRows(
  window: ConnStatsWindow,
  now: Date = new Date(),
  executor: Executor = db
): Promise<{ rows: ConnStatsDbRow[]; capped: boolean }> {
  const since = new Date(now.getTime() - CONN_STATS_WINDOWS[window] * 24 * 60 * 60 * 1000)
  const t = steerConnectionEvents
  const raw = await executor
    .select({
      platform: t.platform,
      version: t.version,
      phase: t.phase,
      outcome: t.outcome,
      path: t.path,
      firstFrameMs: t.firstFrameMs,
      fallbacks: t.fallbacks,
      errorClass: t.errorClass,
      durationMs: t.durationMs,
      failures: t.failures,
    })
    .from(t)
    .where(and(eq(t.kind, `steer.view`), gte(t.receivedAt, since)))
    .limit(DASHBOARD_ROW_CAP)
  const rows = raw.map((r) => ({
    platform: r.platform,
    version: r.version,
    phase: r.phase as SteerConnRow[`phase`],
    outcome: r.outcome as SteerConnRow[`outcome`],
    path: (r.path ?? undefined) as SteerConnRow[`path`],
    firstFrameMs: r.firstFrameMs ?? undefined,
    fallbacks: r.fallbacks ?? undefined,
    errorClass: r.errorClass ?? undefined,
    durationMs: r.durationMs ?? undefined,
    failures: r.failures,
  }))
  return { rows, capped: raw.length >= DASHBOARD_ROW_CAP }
}
