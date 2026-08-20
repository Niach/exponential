import { z } from "zod"
import { sql, type SQL } from "drizzle-orm"
import { router, adminProcedure, type Context } from "@/lib/trpc"
import { emailBounces, emailDeliveries, notifications } from "@/db/schema"
import { pool } from "@/db/connection"
import {
  getSteerRelayConfig,
  steerServerHttpBase,
  type RelayFetch,
  type SteerRelayConfig,
} from "@/lib/steer"
import { cpuPercentSince, metricsSnapshot } from "@/lib/metrics/registry"
import {
  getWidgetConfigIpLimiter,
  getWidgetRateLimiters,
} from "@/lib/widget/rate-limit"

// Admin console → Performance (EXP-553). Four procedures with distinct poll
// cadences: `runtime` (in-memory metrics + process stats, 5s), `relays`
// (server-side steer/push probes, 15s), `database` (pool + pg_stat, 30s) and
// `notificationsEmail` (windowed DB aggregates, 60s). Works self-hosted —
// nothing here is cloud-gated; unconfigured relays report `configured: false`.

// ── Relay probes ──────────────────────────────────────────────────────────────

const RELAY_PROBE_TIMEOUT_MS = 3_000

export interface SteerRelayCounters {
  connectionsAccepted: number
  activityFramesFanned: number
  startsRouted: number
  slowConsumerEvictions: number
  rateLimitedRejections: number
}

export interface SteerRelayProbe {
  configured: boolean
  ok?: boolean
  latencyMs?: number
  connections?: number
  devices?: number
  rooms?: number
  startedAt?: number | null
  /** null = the deployed relay predates /stats (healthz fallback). */
  counters?: SteerRelayCounters | null
}

export interface PushRelayStats {
  sendRequests: number
  sendOk: number
  sendFailed: number
  deadlineTimeouts: number
  tokensRequested: number
  tokensOk: number
  tokensFailed: number
  invalidTokens: number
  lastErrorAt: number | null
  lastError: string | null
}

export interface PushRelayProbe {
  configured: boolean
  ok?: boolean
  latencyMs?: number
  firebaseConfigured?: boolean | null
  startedAt?: number | null
  /** null = the deployed relay predates /stats (healthz fallback). */
  stats?: PushRelayStats | null
}

function asCount(value: unknown): number {
  return typeof value === `number` && Number.isFinite(value) ? value : 0
}

/** GET {steer relay}/stats (secret-gated), degrading to the public /healthz
 * gauges against an already-deployed older relay (404 there). Exported for
 * mock-fetch tests. */
export async function probeSteerRelay(
  config: SteerRelayConfig | null,
  fetchImpl: RelayFetch = globalThis.fetch
): Promise<SteerRelayProbe> {
  if (!config) return { configured: false }
  const base = steerServerHttpBase(config)
  const start = performance.now()
  try {
    const res = await fetchImpl(`${base}/stats`, {
      headers: { "x-relay-secret": config.secret },
      signal: AbortSignal.timeout(RELAY_PROBE_TIMEOUT_MS),
    })
    const latencyMs = Math.round(performance.now() - start)
    if (res.ok) {
      const json = (await res.json()) as {
        startedAt?: number
        connections?: number
        devices?: number
        rooms?: number
        counters?: Partial<SteerRelayCounters>
      }
      return {
        configured: true,
        ok: true,
        latencyMs,
        connections: asCount(json.connections),
        devices: asCount(json.devices),
        rooms: asCount(json.rooms),
        startedAt: typeof json.startedAt === `number` ? json.startedAt : null,
        counters: {
          connectionsAccepted: asCount(json.counters?.connectionsAccepted),
          activityFramesFanned: asCount(json.counters?.activityFramesFanned),
          startsRouted: asCount(json.counters?.startsRouted),
          slowConsumerEvictions: asCount(json.counters?.slowConsumerEvictions),
          rateLimitedRejections: asCount(json.counters?.rateLimitedRejections),
        },
      }
    }
    if (res.status === 404) {
      const health = await fetchImpl(`${base}/healthz`, {
        signal: AbortSignal.timeout(RELAY_PROBE_TIMEOUT_MS),
      })
      if (health.ok) {
        const json = (await health.json()) as {
          connections?: number
          devices?: number
          rooms?: number
        }
        return {
          configured: true,
          ok: true,
          latencyMs: Math.round(performance.now() - start),
          connections: asCount(json.connections),
          devices: asCount(json.devices),
          rooms: asCount(json.rooms),
          startedAt: null,
          counters: null,
        }
      }
    }
    return { configured: true, ok: false, latencyMs }
  } catch {
    return { configured: true, ok: false }
  }
}

/** GET {push relay}/stats (secret-gated), degrading to /healthz (which
 * carries nothing) against an older relay. Exported for mock-fetch tests. */
export async function probePushRelay(
  env: Record<string, string | undefined> = process.env,
  fetchImpl: RelayFetch = globalThis.fetch
): Promise<PushRelayProbe> {
  const url = env.PUSH_RELAY_URL?.trim().replace(/\/+$/, ``)
  if (!url) return { configured: false }
  const secret = env.PUSH_RELAY_SECRET ?? ``
  const start = performance.now()
  try {
    const res = await fetchImpl(`${url}/stats`, {
      headers: { "x-relay-secret": secret },
      signal: AbortSignal.timeout(RELAY_PROBE_TIMEOUT_MS),
    })
    const latencyMs = Math.round(performance.now() - start)
    if (res.ok) {
      const json = (await res.json()) as {
        startedAt?: number
        firebaseConfigured?: boolean
        lastErrorAt?: number | null
        lastError?: string | null
      } & Partial<Record<keyof PushRelayStats, unknown>>
      return {
        configured: true,
        ok: true,
        latencyMs,
        firebaseConfigured:
          typeof json.firebaseConfigured === `boolean`
            ? json.firebaseConfigured
            : null,
        startedAt: typeof json.startedAt === `number` ? json.startedAt : null,
        stats: {
          sendRequests: asCount(json.sendRequests),
          sendOk: asCount(json.sendOk),
          sendFailed: asCount(json.sendFailed),
          deadlineTimeouts: asCount(json.deadlineTimeouts),
          tokensRequested: asCount(json.tokensRequested),
          tokensOk: asCount(json.tokensOk),
          tokensFailed: asCount(json.tokensFailed),
          invalidTokens: asCount(json.invalidTokens),
          lastErrorAt:
            typeof json.lastErrorAt === `number` ? json.lastErrorAt : null,
          lastError:
            typeof json.lastError === `string` ? json.lastError : null,
        },
      }
    }
    if (res.status === 404) {
      const health = await fetchImpl(`${url}/healthz`, {
        signal: AbortSignal.timeout(RELAY_PROBE_TIMEOUT_MS),
      })
      if (health.ok) {
        return {
          configured: true,
          ok: true,
          latencyMs: Math.round(performance.now() - start),
          firebaseConfigured: null,
          startedAt: null,
          stats: null,
        }
      }
    }
    return { configured: true, ok: false, latencyMs }
  } catch {
    return { configured: true, ok: false }
  }
}

// ── Database stat SQL ─────────────────────────────────────────────────────────

// Plain read-only pg_stat queries, vanilla PG17 — no pg_stat_statements or
// other extensions. Exported as constants so admin-performance.test.ts can
// pin the load-bearing clauses.

export const SQL_CONNECTIONS_BY_STATE = `select coalesce(state, 'unknown') as state, count(*)::int as count from pg_stat_activity where datname = current_database() group by 1 order by 2 desc`

export const SQL_DB_STATS = `select pg_database_size(current_database())::bigint as size_bytes, blks_hit::bigint as blks_hit, blks_read::bigint as blks_read, xact_commit::bigint as xact_commit, xact_rollback::bigint as xact_rollback, deadlocks::bigint as deadlocks, temp_files::bigint as temp_files, numbackends as backends from pg_stat_database where datname = current_database()`

export const SQL_LONG_RUNNING = `select count(*)::int as count from pg_stat_activity where datname = current_database() and state = 'active' and pid <> pg_backend_pid() and now() - query_start > interval '5 seconds'`

export const SQL_TOP_TABLES = `select c.relname as table_name, pg_total_relation_size(c.oid)::bigint as total_bytes, coalesce(s.n_live_tup, 0)::bigint as live_rows, coalesce(s.n_dead_tup, 0)::bigint as dead_rows, coalesce(s.seq_scan, 0)::bigint as seq_scans, coalesce(s.idx_scan, 0)::bigint as idx_scans from pg_class c join pg_namespace n on n.oid = c.relnamespace left join pg_stat_user_tables s on s.relid = c.oid where n.nspname = 'public' and c.relkind = 'r' order by pg_total_relation_size(c.oid) desc limit 12`

// ── Notifications & email query builders ──────────────────────────────────────

/** Rows still awaiting the email digest. The predicate must textually match
 * the partial index idx_notifications_digest_pending
 * (`read_at is null and emailed_at is null`) so the count stays an index
 * scan. Extracted for the compiled-SQL test. */
export function buildDigestBacklogQuery(db: Context[`db`]) {
  return db
    .select({
      count: sql<number>`count(*)::int`,
      // Raw sql`` selections bypass drizzle's Date mapping, so the driver
      // hands back pg's text form (`2026-07-31 18:41:54+00`) — not
      // Date-parseable in every browser. Emit strict ISO UTC instead.
      oldestCreatedAt: sql<
        string | null
      >`to_char(min(${notifications.createdAt}) at time zone 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')`,
    })
    .from(notifications)
    .where(
      sql`${notifications.readAt} is null and ${notifications.emailedAt} is null`
    )
}

/** Window totals with pushed/emailed/read breakdowns in one round trip.
 * Extracted for the compiled-SQL test. */
export function buildNotificationTotalsQuery(
  db: Context[`db`],
  windowStart: SQL
) {
  return db
    .select({
      total: sql<number>`count(*)::int`,
      pushed: sql<number>`count(*) filter (where ${notifications.pushedAt} is not null)::int`,
      emailed: sql<number>`count(*) filter (where ${notifications.emailedAt} is not null)::int`,
      read: sql<number>`count(*) filter (where ${notifications.readAt} is not null)::int`,
    })
    .from(notifications)
    .where(sql`${notifications.createdAt} >= ${windowStart}`)
}

// pg returns bigint as string; every count crossing the wire goes through
// this.
function num(value: unknown): number {
  const parsed = Number(value)
  return Number.isFinite(parsed) ? parsed : 0
}

// ── Router ────────────────────────────────────────────────────────────────────

export const adminPerformanceRouter = router({
  // In-memory metrics + process stats. Poll fast (5s) — everything here is a
  // memory read.
  runtime: adminProcedure.query(() => {
    const mem = process.memoryUsage()
    const { perKeyLimiter, perIpLimiter, perSupportRecipientLimiter } =
      getWidgetRateLimiters()
    return {
      now: new Date().toISOString(),
      process: {
        uptimeSeconds: Math.floor(process.uptime()),
        rssBytes: mem.rss,
        heapUsedBytes: mem.heapUsed,
        heapTotalBytes: mem.heapTotal,
        cpuPercent: cpuPercentSince(),
        bunVersion:
          (globalThis as { Bun?: { version: string } }).Bun?.version ?? null,
      },
      metrics: metricsSnapshot(),
      widgetRateLimiters: [
        { name: `widget per-key`, trackedKeys: perKeyLimiter.trackedKeys },
        { name: `widget per-IP`, trackedKeys: perIpLimiter.trackedKeys },
        {
          name: `support recipient`,
          trackedKeys: perSupportRecipientLimiter.trackedKeys,
        },
        {
          name: `widget config per-IP`,
          trackedKeys: getWidgetConfigIpLimiter().trackedKeys,
        },
      ],
    }
  }),

  // Server-side relay probes — secrets never reach the browser. Poll ~15s.
  relays: adminProcedure.query(async () => {
    const [steer, push] = await Promise.all([
      probeSteerRelay(getSteerRelayConfig()),
      probePushRelay(),
    ])
    return { steer, push }
  }),

  // Pool gauges + pg_stat. Poll ~30s.
  database: adminProcedure.query(async ({ ctx }) => {
    const [connectionsByState, dbStats, longRunning, topTables] =
      await Promise.all([
        ctx.db.execute(sql.raw(SQL_CONNECTIONS_BY_STATE)),
        ctx.db.execute(sql.raw(SQL_DB_STATS)),
        ctx.db.execute(sql.raw(SQL_LONG_RUNNING)),
        ctx.db.execute(sql.raw(SQL_TOP_TABLES)),
      ])
    const statsRow = dbStats.rows[0] ?? {}
    const blksHit = num(statsRow.blks_hit)
    const blksRead = num(statsRow.blks_read)
    return {
      // This web process's pool only — other replicas/tools have their own.
      pool: {
        total: pool.totalCount,
        idle: pool.idleCount,
        waiting: pool.waitingCount,
      },
      connectionsByState: connectionsByState.rows.map((row) => ({
        state: String(row.state),
        count: num(row.count),
      })),
      dbSizeBytes: num(statsRow.size_bytes),
      cacheHitPct: (blksHit / Math.max(1, blksHit + blksRead)) * 100,
      xactCommit: num(statsRow.xact_commit),
      xactRollback: num(statsRow.xact_rollback),
      deadlocks: num(statsRow.deadlocks),
      tempFiles: num(statsRow.temp_files),
      backends: num(statsRow.backends),
      longRunningQueries: num(longRunning.rows[0]?.count),
      topTables: topTables.rows.map((row) => ({
        table: String(row.table_name),
        totalBytes: num(row.total_bytes),
        liveRows: num(row.live_rows),
        deadRows: num(row.dead_rows),
        seqScans: num(row.seq_scans),
        idxScans: num(row.idx_scans),
      })),
    }
  }),

  // Windowed notification + email aggregates (grouped together on purpose —
  // push fires on create, email is the digest of still-unread). Poll ~60s.
  notificationsEmail: adminProcedure
    .input(
      z
        .object({
          days: z
            .union([z.literal(7), z.literal(30), z.literal(90)])
            .default(30),
        })
        .default({ days: 30 })
    )
    .query(async ({ ctx, input }) => {
      const windowStart = sql`now() - make_interval(days => ${input.days})`
      const notificationDay = sql<string>`to_char(date_trunc('day', ${notifications.createdAt}), 'YYYY-MM-DD')`
      const deliveryDay = sql<string>`to_char(date_trunc('day', ${emailDeliveries.createdAt}), 'YYYY-MM-DD')`

      const [
        [totals],
        byDayType,
        [digestBacklog],
        emailByStatus,
        emailByKind,
        emailByDay,
        [bounces],
      ] = await Promise.all([
        buildNotificationTotalsQuery(ctx.db, windowStart),
        ctx.db
          .select({
            day: notificationDay,
            type: notifications.type,
            count: sql<number>`count(*)::int`,
          })
          .from(notifications)
          .where(sql`${notifications.createdAt} >= ${windowStart}`)
          .groupBy(notificationDay, notifications.type)
          .orderBy(notificationDay),
        buildDigestBacklogQuery(ctx.db),
        ctx.db
          .select({
            status: emailDeliveries.status,
            count: sql<number>`count(*)::int`,
          })
          .from(emailDeliveries)
          .where(sql`${emailDeliveries.createdAt} >= ${windowStart}`)
          .groupBy(emailDeliveries.status),
        ctx.db
          .select({
            kind: emailDeliveries.kind,
            count: sql<number>`count(*)::int`,
          })
          .from(emailDeliveries)
          .where(sql`${emailDeliveries.createdAt} >= ${windowStart}`)
          .groupBy(emailDeliveries.kind)
          .orderBy(sql`count(*) desc`),
        ctx.db
          .select({
            day: deliveryDay,
            count: sql<number>`count(*)::int`,
          })
          .from(emailDeliveries)
          .where(sql`${emailDeliveries.createdAt} >= ${windowStart}`)
          .groupBy(deliveryDay)
          .orderBy(deliveryDay),
        ctx.db
          .select({
            total: sql<number>`count(*)::int`,
            suppressed: sql<number>`count(*) filter (where ${emailBounces.suppressedAt} is not null)::int`,
            complaints: sql<number>`count(*) filter (where ${emailBounces.kind} = 'complaint')::int`,
          })
          .from(emailBounces)
          .where(sql`${emailBounces.lastEventAt} >= ${windowStart}`),
      ])

      return {
        days: input.days,
        totals: {
          total: totals?.total ?? 0,
          pushed: totals?.pushed ?? 0,
          emailed: totals?.emailed ?? 0,
          read: totals?.read ?? 0,
        },
        byDayType,
        digestBacklog: {
          count: digestBacklog?.count ?? 0,
          oldestCreatedAt: digestBacklog?.oldestCreatedAt ?? null,
        },
        emailByStatus,
        emailByKind,
        emailByDay,
        bounces: {
          total: bounces?.total ?? 0,
          suppressed: bounces?.suppressed ?? 0,
          complaints: bounces?.complaints ?? 0,
        },
      }
    }),
})
