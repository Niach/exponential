// @vitest-environment node
import { afterAll, describe, expect, it } from "vitest"
import { and, eq, inArray } from "drizzle-orm"
import { db } from "@/db/connection"
import { skills, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

// #VAPP-213: the built-in skills' seed + boot reconcile against the REAL
// migrated Postgres (fixtures/skills.json `builtin`), narrowed to this
// file's own teams so parallel suites never see the rows move.

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'skills' AND column_name = 'builtin') AS ok
`)
if (!ready.ok) console.info(`[builtin.real-db] skipped: ${ready.reason}`)

const { BUILTINS, reconcileBuiltinSkills, seedBuiltinSkills } = await import(`./builtin`)

const made: string[] = []
async function team(): Promise<string> {
  const [row] = await db
    .insert(teams)
    .values({ name: `Built-ins`, slug: `builtins-${crypto.randomUUID().slice(0, 8)}` })
    .returning({ id: teams.id })
  made.push(row!.id)
  return row!.id
}
const rowsOf = (teamId: string) =>
  db
    .select({ name: skills.name, builtin: skills.builtin, body: skills.body, description: skills.description })
    .from(skills)
    .where(eq(skills.teamId, teamId))
    .orderBy(skills.name)

afterAll(async () => {
  if (made.length) await db.delete(teams).where(inArray(teams.id, made))
})

describe.skipIf(!ready.ok)(`built-in skills (real db)`, () => {
  it(`seeds every built-in once; a second seed is a no-op`, async () => {
    const teamId = await team()
    await seedBuiltinSkills(db, [teamId])
    await seedBuiltinSkills(db, [teamId])
    const rows = await rowsOf(teamId)
    expect(rows.map((r) => [r.name, r.builtin])).toEqual(BUILTINS.map((b) => [b.name, true]).sort())
  })

  it(`reconcile renames exponential-ui in place, and retires it where the team already holds the new name`, async () => {
    const teamId = await team()
    const [old] = await db
      .insert(skills)
      .values({ teamId, name: `exponential-ui`, description: `old`, body: `old`, builtin: true })
      .returning({ id: skills.id })
    await reconcileBuiltinSkills([teamId])
    const [renamed] = await db.select({ id: skills.id }).from(skills).where(and(eq(skills.teamId, teamId), eq(skills.name, `exponential-genui`)))
    expect(renamed!.id).toBe(old!.id)
    expect((await rowsOf(teamId)).map((r) => r.name)).toEqual(BUILTINS.map((b) => b.name).sort())

    const taken = await team()
    await db.insert(skills).values([
      { teamId: taken, name: `exponential-ui`, description: `old`, body: `old`, builtin: true },
      { teamId: taken, name: `exponential-genui`, description: `ours`, body: `# ours`, builtin: false },
    ])
    await reconcileBuiltinSkills([taken])
    expect((await rowsOf(taken)).map((r) => [r.name, r.builtin])).toEqual([
      [`exponential-apps`, true],
      [`exponential-genui`, false],
      [`exponential-motion`, true],
    ])
  })

  it(`reconcile seeds, heals a stale row, drops a retired built-in and leaves a team's own skill alone`, async () => {
    const teamId = await team()
    await db.insert(skills).values([
      { teamId, name: `exponential-genui`, description: `ours`, body: `# ours`, builtin: false },
      { teamId, name: `exponential-legacy`, description: `old`, body: `# old`, builtin: true },
    ])
    await reconcileBuiltinSkills([teamId])
    await db
      .update(skills)
      .set({ body: `stale` })
      .where(and(eq(skills.teamId, teamId), eq(skills.name, `exponential-apps`)))
    await reconcileBuiltinSkills([teamId])
    const rows = await rowsOf(teamId)
    expect(rows.map((r) => [r.name, r.builtin])).toEqual([
      [`exponential-apps`, true],
      [`exponential-genui`, false],
      [`exponential-motion`, true],
    ])
    expect(rows[0]!.body).toBe(BUILTINS.find((b) => b.name === `exponential-apps`)!.body)
    expect(rows[1]!.body).toBe(`# ours`)
  })
})
