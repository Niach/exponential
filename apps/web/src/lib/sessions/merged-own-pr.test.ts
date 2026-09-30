import { describe, expect, it } from "vitest"
import { PgDialect } from "drizzle-orm/pg-core"

// EXP-637/639: the durable own-PR spare, shared by the MCP pr_merge closure
// and the EXP-1146 yolo tree merge. A structural fake db records set values
// and the where clause; the clause is asserted by its SQL text.

import {
  priorOf,
  revertMergedOwnPr,
  stampMergedOwnPr,
} from "@/lib/sessions/merged-own-pr"

const SESSION = `66666666-6666-4666-8666-666666666666`

function fakeDb() {
  const updates: Array<{ set: Record<string, unknown>; where: unknown }> = []
  const db = {
    update: () => ({
      set: (set: Record<string, unknown>) => ({
        where: async (where: unknown) => {
          updates.push({ set, where })
        },
      }),
    }),
  }
  return { db: db as never, updates }
}

function sqlOf(where: unknown) {
  return new PgDialect().sqlToQuery(where as never)
}

describe(`stampMergedOwnPr`, () => {
  it(`stamps the spare, restores running and clears the picker, on live rows only`, async () => {
    const { db, updates } = fakeDb()
    await stampMergedOwnPr(db, SESSION)
    expect(updates).toHaveLength(1)
    expect(updates[0]!.set).toMatchObject({
      mergedOwnPr: true,
      status: `running`,
      needsInput: false,
    })
    const { sql, params } = sqlOf(updates[0]!.where)
    expect(params).toContain(SESSION)
    expect(sql).toContain(`"status" in (`)
    expect(params).toContain(`running`)
    expect(params).toContain(`in_review`)
    expect(params).not.toContain(`ended`)
  })
})

describe(`revertMergedOwnPr`, () => {
  it(`puts the row back exactly, guarded on the stamp and the status the stamp wrote`, async () => {
    const { db, updates } = fakeDb()
    await revertMergedOwnPr(
      db,
      SESSION,
      priorOf({ status: `in_review`, needsInput: true })
    )
    expect(updates[0]!.set).toMatchObject({
      mergedOwnPr: false,
      status: `in_review`,
      needsInput: true,
    })
    const { sql, params } = sqlOf(updates[0]!.where)
    expect(sql).toContain(`"merged_own_pr" =`)
    expect(params).toContain(true)
    expect(params).toContain(`running`)
  })

  it(`priorOf maps every status but in_review to running`, () => {
    expect(priorOf({ status: `running`, needsInput: false })).toEqual({
      status: `running`,
      needsInput: false,
    })
    expect(priorOf({ status: `in_review`, needsInput: false }).status).toBe(`in_review`)
  })
})
