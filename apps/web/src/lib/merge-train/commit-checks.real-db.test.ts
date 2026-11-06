// @vitest-environment node
import { afterAll, describe, expect, it } from "vitest"
import { eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { commitChecks, repositories, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import type { Forge } from "@/lib/ecode/forge"

// EXP-1709: commit_checks against the real migrated Postgres: one row per
// repository row of the full name, never downgraded, and the forge read-back.

const ready = await probeRealDb(`SELECT to_regclass('public.commit_checks') IS NOT NULL AS ok`)
if (!ready.ok) console.info(`[commit-checks.real-db] skipped: ${ready.reason}`)

const { writeCommitCheck, readCommitChecks } = await import(`@/lib/merge-train/commit-checks`)

const made: string[] = []
afterAll(async () => {
  if (!ready.ok || made.length === 0) return
  await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.join(`','`)}')`)}`)
})

async function repo(fullName: string): Promise<string> {
  const slug = `cc-${crypto.randomUUID().slice(0, 8)}`
  const [team] = await db.insert(teams).values({ name: slug, slug }).returning({ id: teams.id })
  made.push(team!.id)
  const [row] = await db.insert(repositories).values({ teamId: team!.id, fullName }).returning({ id: repositories.id })
  return row!.id
}

describe.skipIf(!ready.ok)(`commit_checks`, () => {
  it(`writes every repository row of the name and never downgrades a completed check`, async () => {
    const fullName = `org/cc-${crypto.randomUUID().slice(0, 6)}`
    const a = await repo(fullName)
    const b = await repo(fullName)
    expect(await writeCommitCheck({ repoFullName: fullName, sha: `s1`, check: { name: `gate`, status: `in_progress`, conclusion: null } })).toBe(2)
    await writeCommitCheck({ repoFullName: fullName, sha: `s1`, check: { name: `gate`, status: `completed`, conclusion: `success` } })
    await writeCommitCheck({ repoFullName: fullName, sha: `s1`, check: { name: `gate`, status: `queued`, conclusion: null } })
    for (const id of [a, b]) {
      const rows = await db.select().from(commitChecks).where(eq(commitChecks.repositoryId, id))
      expect(rows.map((r) => [r.name, r.status, r.conclusion])).toEqual([[`gate`, `completed`, `success`]])
    }
  })

  it(`reads locally once settled, else from the forge and writes it back`, async () => {
    const fullName = `org/cc-${crypto.randomUUID().slice(0, 6)}`
    const id = await repo(fullName)
    let reads = 0
    const forge = {
      checksAt: async () => {
        reads++
        return [{ name: `gate`, status: `completed` as const, conclusion: `failure` }]
      },
    } as unknown as Forge
    expect(await readCommitChecks(forge, { id, fullName }, `s2`)).toEqual([{ name: `gate`, status: `completed`, conclusion: `failure` }])
    expect(await readCommitChecks(forge, { id, fullName }, `s2`)).toEqual([{ name: `gate`, status: `completed`, conclusion: `failure` }])
    expect(reads).toBe(1)
  })
})
