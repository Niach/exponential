import { describe, expect, it } from "vitest"
import { sql } from "drizzle-orm"
import { drizzle } from "drizzle-orm/node-postgres"
import { Pool } from "pg"
import {
  buildDigestBacklogQuery,
  buildNotificationTotalsQuery,
  probePushRelay,
  probeSteerRelay,
  SQL_CONNECTIONS_BY_STATE,
  SQL_DB_STATS,
  SQL_LONG_RUNNING,
  SQL_TOP_TABLES,
} from "./admin-performance"
import type { RelayFetch } from "@/lib/steer"
import type { Context } from "@/lib/trpc"

// Compiled-SQL assertions in the admin-conversions.test.ts mold: no
// connection is opened — .toSQL() never touches the pool.

const db = drizzle({
  client: new Pool({ connectionString: `postgresql://localhost:1/none` }),
  casing: `snake_case`,
}) as unknown as Context[`db`]

describe(`buildDigestBacklogQuery`, () => {
  const { sql: text } = buildDigestBacklogQuery(db).toSQL()

  it(`matches the partial index predicate textually`, () => {
    // idx_notifications_digest_pending is `where read_at is null and
    // emailed_at is null` — the backlog gauge must stay an index scan.
    expect(text).toContain(`"notifications"."read_at" is null`)
    expect(text).toContain(`"notifications"."emailed_at" is null`)
  })
})

describe(`buildNotificationTotalsQuery`, () => {
  const { sql: text } = buildNotificationTotalsQuery(
    db,
    sql`now() - make_interval(days => 30)`
  ).toSQL()

  it(`carries the three filtered counts`, () => {
    // drizzle strips the table qualifier inside single-FROM select-list sql``
    // templates (the EXP-373 behavior) — harmless here, notifications is the
    // only table in scope, so the compiled refs are bare column names.
    expect(text).toContain(
      `count(*) filter (where "pushed_at" is not null)::int`
    )
    expect(text).toContain(
      `count(*) filter (where "emailed_at" is not null)::int`
    )
    expect(text).toContain(`count(*) filter (where "read_at" is not null)::int`)
  })
})

describe(`pg_stat SQL constants`, () => {
  it(`scopes activity queries to the current database`, () => {
    expect(SQL_CONNECTIONS_BY_STATE).toContain(`datname = current_database()`)
    expect(SQL_LONG_RUNNING).toContain(`datname = current_database()`)
    expect(SQL_DB_STATS).toContain(`datname = current_database()`)
  })

  it(`excludes the probing backend from the long-running count`, () => {
    expect(SQL_LONG_RUNNING).toContain(`pid <> pg_backend_pid()`)
  })

  it(`limits the table scan to public ordinary tables`, () => {
    expect(SQL_TOP_TABLES).toContain(`n.nspname = 'public'`)
    expect(SQL_TOP_TABLES).toContain(`c.relkind = 'r'`)
    expect(SQL_TOP_TABLES).toContain(`limit 12`)
  })
})

// ── Relay probes ──────────────────────────────────────────────────────────────

function fetchStub(
  routes: Record<string, { status: number; json?: unknown }>
): RelayFetch {
  return (url) => {
    const path = new URL(url).pathname
    const hit = routes[path]
    if (!hit) throw new Error(`unexpected fetch ${url}`)
    return Promise.resolve({
      ok: hit.status >= 200 && hit.status < 300,
      status: hit.status,
      json: () => Promise.resolve(hit.json ?? {}),
    })
  }
}

const steerConfig = { url: `https://steer.example.com`, secret: `s3cret` }

describe(`probeSteerRelay`, () => {
  it(`reports unconfigured without a config`, async () => {
    expect(await probeSteerRelay(null)).toEqual({ configured: false })
  })

  it(`parses gauges and counters from /stats`, async () => {
    const probe = await probeSteerRelay(
      steerConfig,
      fetchStub({
        "/stats": {
          status: 200,
          json: {
            ok: true,
            startedAt: 123,
            connections: 4,
            devices: 2,
            rooms: 1,
            counters: {
              connectionsAccepted: 10,
              activityFramesFanned: 500,
              startsRouted: 3,
              slowConsumerEvictions: 0,
              rateLimitedRejections: 1,
            },
          },
        },
      })
    )
    expect(probe).toMatchObject({
      configured: true,
      ok: true,
      connections: 4,
      devices: 2,
      rooms: 1,
      startedAt: 123,
      counters: { startsRouted: 3, rateLimitedRejections: 1 },
    })
  })

  it(`falls back to /healthz gauges against an old relay`, async () => {
    const probe = await probeSteerRelay(
      steerConfig,
      fetchStub({
        "/stats": { status: 404 },
        "/healthz": {
          status: 200,
          json: { ok: true, connections: 7, devices: 1, rooms: 2 },
        },
      })
    )
    expect(probe).toMatchObject({
      configured: true,
      ok: true,
      connections: 7,
      counters: null,
      startedAt: null,
    })
  })

  it(`reports ok:false on network failure`, async () => {
    const probe = await probeSteerRelay(steerConfig, () =>
      Promise.reject(new Error(`timeout`))
    )
    expect(probe).toEqual({ configured: true, ok: false })
  })
})

describe(`probePushRelay`, () => {
  it(`reports unconfigured without PUSH_RELAY_URL`, async () => {
    expect(await probePushRelay({})).toEqual({ configured: false })
  })

  it(`parses stats from /stats`, async () => {
    const probe = await probePushRelay(
      { PUSH_RELAY_URL: `https://push.example.com`, PUSH_RELAY_SECRET: `x` },
      fetchStub({
        "/stats": {
          status: 200,
          json: {
            ok: true,
            startedAt: 55,
            firebaseConfigured: true,
            sendRequests: 12,
            sendOk: 11,
            sendFailed: 1,
            deadlineTimeouts: 0,
            tokensRequested: 40,
            tokensOk: 38,
            tokensFailed: 2,
            invalidTokens: 2,
            lastErrorAt: null,
            lastError: null,
          },
        },
      })
    )
    expect(probe).toMatchObject({
      configured: true,
      ok: true,
      firebaseConfigured: true,
      stats: { sendRequests: 12, tokensOk: 38, invalidTokens: 2 },
    })
  })

  it(`degrades to a stats-less ok against an old relay`, async () => {
    const probe = await probePushRelay(
      { PUSH_RELAY_URL: `https://push.example.com` },
      fetchStub({
        "/stats": { status: 404 },
        "/healthz": { status: 200, json: { ok: true } },
      })
    )
    expect(probe).toMatchObject({
      configured: true,
      ok: true,
      stats: null,
      firebaseConfigured: null,
    })
  })
})
