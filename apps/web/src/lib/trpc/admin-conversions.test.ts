import { describe, expect, it } from "vitest"
import { sql } from "drizzle-orm"
import { drizzle } from "drizzle-orm/node-postgres"
import { Pool } from "pg"
import {
  buildSignupCohortQuery,
  buildSignupJourneyQuery,
  buildSignupSourcesQuery,
} from "./admin-conversions"
import type { Context } from "@/lib/trpc"

// EXP-373: /admin/conversions blackscreened because the signup-sources query
// compiled to `ce.user_id = "id"` — drizzle's buildSelection drops the table
// qualifier from column refs inside select-list sql`` templates when the
// query has a single FROM table, so the outer `${users.id}` rebound to
// conversion_events.id inside the correlated subquery (`operator does not
// exist: text = uuid` → 500 → the router's global error fallback replaced the
// whole app). The bug is invisible in the source and only shows in the
// COMPILED sql, so that is what this pins. No connection is opened —
// .toSQL() never touches the pool.

const db = drizzle({
  client: new Pool({ connectionString: `postgresql://localhost:1/none` }),
  casing: `snake_case`,
}) as unknown as Context[`db`]

describe(`buildSignupSourcesQuery`, () => {
  const { sql: text } = buildSignupSourcesQuery(
    db,
    sql`now() - make_interval(days => 30)`
  ).toSQL()

  it(`correlates paid users on a fully qualified users.id`, () => {
    // Both sides carry their table: an unqualified side is free to rebind to
    // whatever the nearest FROM happens to expose.
    expect(text).toContain(`"paid_users"."user_id" = "users"."id"`)
  })

  it(`keeps the paid count bound to the joined subquery`, () => {
    expect(text).toContain(`count("paid_users"."user_id")`)
  })

  it(`counts paid users through a deduped join, not a correlated exists`, () => {
    // Grouping the subquery is what keeps the join 1:0..1 so the sibling
    // `count(*)` signup total is not multiplied by paid events.
    expect(text).toContain(`left join`)
    expect(text).toContain(`"paid_users"`)
    expect(text).toMatch(/group by "conversion_events"\."user_id"/)
    expect(text).not.toContain(`exists`)
  })
})

// EXP-759: the signup cohort follows the same grouped-join rule. Every stage
// is a 1:0..1 subquery LEFT JOINed on a fully qualified users.id — never a
// correlated subquery in the select list. Drizzle emits a subquery's aliased
// columns UNQUALIFIED in select-list sql`` templates ("last_board_at", not
// "b"."last_board_at"), so every alias must be unique across the whole join
// (users has no column by those names) — pinned here too.
const STAGE_JOINS = [
  `"m"."user_id" = "users"."id"`,
  `"b"."user_id" = "users"."id"`,
  `"i"."creator_id" = "users"."id"`,
  `"inv"."invited_by_id" = "users"."id"`,
  `"d"."user_id" = "users"."id"`,
  `"rv"."user_id" = "users"."id"`,
  `"ucp"."user_id" = "users"."id"`,
  `"paid"."user_id" = "users"."id"`,
]

describe(`buildSignupCohortQuery`, () => {
  const { sql: text } = buildSignupCohortQuery(
    db,
    sql`now() - make_interval(days => 30)`
  ).toSQL()

  it(`joins every stage on qualified user ids and uses no exists`, () => {
    for (const join of STAGE_JOINS) expect(text).toContain(join)
    expect(text).not.toContain(`exists`)
  })

  it(`groups each stage subquery so the outer count stays exact`, () => {
    expect(text).toMatch(/group by "team_members"\."user_id"/)
    expect(text).toMatch(/group by "issues"\."creator_id"/)
    expect(text).toMatch(/group by "team_invites"\."invited_by_id"/)
    expect(text).toMatch(/group by "devices"\."user_id"/)
    expect(text).toMatch(/group by "conversion_events"\."user_id"/)
    expect(text).not.toMatch(/group by "users"/)
  })

  it(`keeps the users side of every select-list comparison qualified`, () => {
    expect(text).toContain(`to_char("users"."created_at"`)
    expect(text).toContain(`"last_board_at" > "users"."created_at"::timestamptz`)
    expect(text).toContain(`"platform_last_seen_at"::date > "users"."created_at"::date`)
    expect(text).toContain(`= 'return_visit'`)
  })

  it(`uses subquery aliases that cannot collide with a users column`, () => {
    // Unqualified in the select list — a users column of the same name would
    // make Postgres refuse the query as ambiguous.
    for (const alias of [`last_board_at`, `last_day`, `platform_last_seen_at`]) {
      expect(text).toContain(`as "${alias}"`)
    }
  })
})

describe(`buildSignupJourneyQuery`, () => {
  const { sql: text } = buildSignupJourneyQuery(
    db,
    sql`now() - make_interval(days => 30)`
  ).toSQL()

  it(`is the newest-first per-user view of the same joins`, () => {
    for (const join of [...STAGE_JOINS, `"s"."user_id" = "users"."id"`]) {
      expect(text).toContain(join)
    }
    expect(text).not.toContain(`exists`)
    expect(text).toMatch(/order by "users"\."created_at" desc limit/)
  })
})
