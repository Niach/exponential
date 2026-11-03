// @vitest-environment node
import { describe, expect, it } from "vitest"
import { sql } from "drizzle-orm"
import { appRecords, apps, teams } from "@/db/schema"
import { db } from "@/db/connection"
import { inRollback, probeRealDb, type RealDbTx } from "@/test/real-db"

// #VAPP-193: the RLS backstop behind the records service, against the REAL
// migrated Postgres with the custom SQL applied. Under role `eapps` a
// deliberately wrong query (no app filter at all) sees only the scope's
// rows, can read no other table, cannot write outside its scope (WITH
// CHECK), cannot hard-delete, and a slow statement dies at 2 s.

const ready = await probeRealDb(`
  SELECT (EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'eapps')
    AND EXISTS (SELECT 1 FROM pg_policies WHERE tablename = 'app_records' AND policyname = 'eapps_scope')) AS ok
`)
if (!ready.ok) console.info(`[records-service.real-db] skipped: ${ready.reason}`)

const { asEapps } = await import(`@/lib/apps/records-service`)

const id8 = () => crypto.randomUUID().slice(0, 8)

async function seed(tx: RealDbTx) {
  const [a, b] = await tx
    .insert(teams)
    .values([
      { name: `A`, slug: `rls-a-${id8()}` },
      { name: `B`, slug: `rls-b-${id8()}` },
    ])
    .returning({ id: teams.id })
  const [a1, a2, b1] = await tx
    .insert(apps)
    .values([
      { teamId: a!.id, slug: `one`, name: `One` },
      { teamId: a!.id, slug: `two`, name: `Two` },
      { teamId: b!.id, slug: `one`, name: `One` },
    ])
    .returning({ id: apps.id })
  await tx.insert(appRecords).values([
    { appId: a1!.id, teamId: a!.id, kind: `entry`, data: { n: 1 } },
    { appId: a1!.id, teamId: a!.id, kind: `entry`, data: { n: 2 } },
    { appId: a2!.id, teamId: a!.id, kind: `entry`, data: { n: 3 } },
    { appId: b1!.id, teamId: b!.id, kind: `entry`, data: { n: 4 } },
  ])
  return { teamA: a!.id, teamB: b!.id, a1: a1!.id, a2: a2!.id, b1: b1!.id }
}

class Undo extends Error {}

/** `fn` under the role inside a SAVEPOINT that is always rolled back (the
 *  role and scope revert with it): its value, or the Postgres error. */
async function underRole<T>(tx: RealDbTx, scope: { teamId: string; appIds: string[]; write?: boolean } | null, fn: (tx: RealDbTx) => Promise<T>): Promise<{ value?: T; error?: { code?: string; message: string } }> {
  let out: { value?: T; error?: { code?: string; message: string } } = {}
  try {
    await tx.transaction(async (sp) => {
      try {
        if (scope) out = { value: await asEapps(sp, scope, fn) }
        else {
          await sp.execute(sql`SET LOCAL ROLE eapps`)
          out = { value: await fn(sp) }
        }
      } catch (e) {
        const cause = (e as { cause?: { code?: string; message?: string } }).cause ?? (e as { code?: string; message?: string })
        out = { error: { code: cause.code, message: cause.message ?? String(e) } }
      }
      throw new Undo()
    })
  } catch (e) {
    if (!(e instanceof Undo)) throw e
  }
  return out
}

const allRows = (tx: RealDbTx) => tx.execute<{ n: number; app_id: string }>(sql`SELECT (data->>'n')::int AS n, app_id FROM app_records ORDER BY 1`).then((r) => r.rows.map((row) => row.n))

describe.skipIf(!ready.ok)(`records RLS scope (real db)`, () => {
  it(`an unfiltered read sees only the scope's team and apps; no scope = nothing`, async () => {
    await inRollback(async (tx) => {
      const w = await seed(tx)
      expect((await underRole(tx, { teamId: w.teamA, appIds: [w.a1] }, allRows)).value).toEqual([1, 2])
      expect((await underRole(tx, { teamId: w.teamA, appIds: [w.a1, w.a2] }, allRows)).value).toEqual([1, 2, 3])
      // Another team's app named under this team's id: nothing.
      expect((await underRole(tx, { teamId: w.teamA, appIds: [w.b1] }, allRows)).value).toEqual([])
      expect((await underRole(tx, null, allRows)).value).toEqual([])
      // The owner role (outside the scope) still sees every row.
      expect((await allRows(tx)).filter((n) => n <= 4).length).toBeGreaterThanOrEqual(4)
    })
  })

  it(`no other table is readable under the role`, async () => {
    await inRollback(async (tx) => {
      const w = await seed(tx)
      for (const table of [`apps`, `teams`, `team_members`, `users`, `issues`, `app_access`]) {
        const read = await underRole(tx, { teamId: w.teamA, appIds: [w.a1] }, (t) => t.execute(sql.raw(`SELECT 1 FROM ${table} LIMIT 1`)))
        expect(read.error?.code, table).toBe(`42501`)
      }
    })
  })

  it(`a write outside the scope fails WITH CHECK; hard deletes are denied`, async () => {
    await inRollback(async (tx) => {
      const w = await seed(tx)
      const scope = { teamId: w.teamA, appIds: [w.a1], write: true }
      const foreignTeam = await underRole(tx, scope, (t) => t.insert(appRecords).values({ appId: w.b1, teamId: w.teamB, kind: `entry`, data: {} }))
      expect(foreignTeam.error?.message).toMatch(/row-level security/)
      const foreignApp = await underRole(tx, scope, (t) => t.insert(appRecords).values({ appId: w.a2, teamId: w.teamA, kind: `entry`, data: {} }))
      expect(foreignApp.error?.message).toMatch(/row-level security/)
      const moved = await underRole(tx, scope, (t) => t.execute(sql`UPDATE app_records SET app_id = ${w.a2}`))
      expect(moved.error?.message).toMatch(/row-level security/)
      const deleted = await underRole(tx, scope, (t) => t.execute(sql`DELETE FROM app_records`))
      expect(deleted.error?.code).toBe(`42501`)
      // An in-scope insert works, and the SECURITY DEFINER trigger still
      // fills the app's trash mirror (it reads `apps`) without an `apps` grant.
      await tx.execute(sql`UPDATE apps SET deleted_at = now() WHERE id = ${w.a1}`)
      const own = await underRole(tx, scope, (t) =>
        t.insert(appRecords).values({ appId: w.a1, teamId: w.teamA, kind: `entry`, data: { n: 9 } }).returning({ id: appRecords.id, appDeletedAt: appRecords.appDeletedAt })
      )
      expect(own.error).toBeUndefined()
      expect(own.value?.[0]?.appDeletedAt).toBeInstanceOf(Date)
    })
  })

  // eApps R3: a write needs `eapps.write = on` (a member with write access);
  // a read-only scope still reads.
  it(`a read-only scope reads but never inserts or updates`, async () => {
    await inRollback(async (tx) => {
      const w = await seed(tx)
      const scope = { teamId: w.teamA, appIds: [w.a1] }
      expect((await underRole(tx, scope, allRows)).value).toEqual([1, 2])
      const insert = await underRole(tx, scope, (t) => t.insert(appRecords).values({ appId: w.a1, teamId: w.teamA, kind: `entry`, data: {} }))
      expect(insert.error?.message).toMatch(/row-level security/)
      const update = await underRole(tx, scope, (t) => t.execute(sql`UPDATE app_records SET data = '{}'::jsonb RETURNING id`))
      expect((update.value as { rows: unknown[] } | undefined)?.rows ?? []).toEqual([])
      const write = await underRole(tx, { ...scope, write: true }, (t) => t.insert(appRecords).values({ appId: w.a1, teamId: w.teamA, kind: `entry`, data: { n: 7 } }))
      expect(write.error).toBeUndefined()
    })
  })

  it(`a slow statement under the role hits the 2 s timeout`, async () => {
    await inRollback(async (tx) => {
      const w = await seed(tx)
      const started = Date.now()
      const slow = await underRole(tx, { teamId: w.teamA, appIds: [w.a1] }, (t) => t.execute(sql`SELECT pg_sleep(5)`))
      expect(slow.error?.code).toBe(`57014`)
      expect(Date.now() - started).toBeLessThan(4500)
    })
  }, 10_000)

  it(`the role and scope end with the transaction (the pool connection is clean)`, async () => {
    await db.transaction(async (tx) => {
      await asEapps(tx, { teamId: crypto.randomUUID(), appIds: [] }, async () => {})
    })
    const [row] = (await db.execute<{ who: string; timeout: string }>(sql`SELECT current_user AS who, current_setting('statement_timeout') AS timeout`)).rows
    expect(row!.who).not.toBe(`eapps`)
    expect(row!.timeout).not.toBe(`2s`)
  })
})
