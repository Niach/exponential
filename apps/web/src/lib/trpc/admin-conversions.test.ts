import { describe, expect, it } from "vitest"
import { sql } from "drizzle-orm"
import { drizzle } from "drizzle-orm/node-postgres"
import { Pool } from "pg"
import { buildSignupSourcesQuery } from "./admin-conversions"
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
