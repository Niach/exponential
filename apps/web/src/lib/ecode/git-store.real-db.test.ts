// @vitest-environment node
import { afterAll, describe, expect, it } from "vitest"
import { sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { repositories, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

// #EXP-1616: eCode's refs and packs against the real migrated Postgres:
// the CAS is all-or-nothing, a push records its pack and moves its refs in
// one transaction, a restore replaces every pack and ref, repo bytes count
// toward the team (superseded ones stop counting), bundles keep the newest.

const ready = await probeRealDb(`SELECT to_regclass('public.repo_refs') IS NOT NULL AS ok`)
if (!ready.ok) console.info(`[git-store.real-db] skipped: ${ready.reason}`)

const store = await import(`@/lib/ecode/git-store`)

const A = `a`.repeat(40)
const B = `b`.repeat(40)
const C = `c`.repeat(40)

const made: string[] = []
afterAll(async () => {
  if (!ready.ok || made.length === 0) return
  await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.join(`','`)}')`)}`)
})

async function repo(): Promise<{ teamId: string; repositoryId: string; slug: string }> {
  const slug = `ec-${crypto.randomUUID().slice(0, 8)}`
  const [team] = await db.insert(teams).values({ name: slug, slug }).returning({ id: teams.id })
  made.push(team!.id)
  const [row] = await db
    .insert(repositories)
    .values({ teamId: team!.id, provider: `ecode`, fullName: `${slug}/app`, remoteUrl: `exp://${slug}/app` })
    .returning({ id: repositories.id })
  return { teamId: team!.id, repositoryId: row!.id, slug }
}

describe.skipIf(!ready.ok)(`eCode git store (#EXP-1616)`, () => {
  it(`moves refs by CAS, all or nothing`, async () => {
    const { repositoryId } = await repo()
    expect(await store.compareAndSwapRefs(repositoryId, [{ ref: `refs/heads/main`, old: null, new: A }])).toBe(true)
    // A second create of the same ref loses.
    expect(await store.compareAndSwapRefs(repositoryId, [{ ref: `refs/heads/main`, old: null, new: B }])).toBe(false)
    // One stale expectation rolls the whole batch back.
    expect(
      await store.compareAndSwapRefs(repositoryId, [
        { ref: `refs/heads/topic`, old: null, new: B },
        { ref: `refs/heads/main`, old: B, new: C },
      ])
    ).toBe(false)
    expect(await store.listRefs(repositoryId)).toEqual({ "refs/heads/main": A })
    expect(await store.compareAndSwapRefs(repositoryId, [{ ref: `refs/heads/main`, old: A, new: B }, { ref: `refs/heads/topic`, old: null, new: C }])).toBe(true)
    expect(await store.compareAndSwapRefs(repositoryId, [{ ref: `refs/heads/topic`, old: C, new: null }])).toBe(true)
    expect(await store.listRefs(repositoryId)).toEqual({ "refs/heads/main": B })
  })

  it(`a push records its pack and refs together; a rejected push records neither`, async () => {
    const { teamId, repositoryId } = await repo()
    const key = (n: string) => `ecode/${teamId}/${repositoryId}/packs/${n}.pack`
    expect(await store.recordPush({ teamId, repositoryId, pack: { key: key(`1`), bytes: 100 }, updates: [{ ref: `refs/heads/main`, old: null, new: A }] })).toEqual({ ok: true })
    expect(await store.recordPush({ teamId, repositoryId, pack: { key: key(`2`), bytes: 50 }, updates: [{ ref: `refs/heads/main`, old: B, new: C }] })).toEqual({
      ok: false,
      rejected: `refs/heads/main`,
    })
    expect((await store.livePacks(repositoryId)).map((p) => p.key)).toEqual([key(`1`)])
    expect(await store.teamRepoBytes(teamId)).toBe(100)
  })

  it(`a restore replaces every pack and ref; superseded bytes stop counting`, async () => {
    const { teamId, repositoryId } = await repo()
    const key = (n: string) => `ecode/${teamId}/${repositoryId}/packs/${n}.pack`
    await store.recordPush({ teamId, repositoryId, pack: { key: key(`1`), bytes: 100 }, updates: [{ ref: `refs/heads/main`, old: null, new: A }] })
    await store.recordPush({ teamId, repositoryId, pack: { key: key(`2`), bytes: 30 }, updates: [{ ref: `refs/heads/x`, old: null, new: B }] })
    await store.replaceRepository({ teamId, repositoryId, pack: { key: key(`r`), bytes: 70 }, refs: { "refs/heads/main": C } })
    expect(await store.listRefs(repositoryId)).toEqual({ "refs/heads/main": C })
    expect((await store.livePacks(repositoryId)).map((p) => p.key)).toEqual([key(`r`)])
    expect(await store.teamRepoBytes(teamId)).toBe(70)
  })

  it(`keeps the newest bundles and finds a repo by its exp:// remote`, async () => {
    const { teamId, repositoryId, slug } = await repo()
    for (let i = 0; i < store.BUNDLES_KEPT + 2; i++) {
      await store.recordBundle({ teamId, repositoryId, key: `ecode/${teamId}/${repositoryId}/bundles/${i}.bundle`, bytes: 10, refs: { "refs/heads/main": A } })
    }
    const kept = await store.listBundles(repositoryId)
    expect(kept).toHaveLength(store.BUNDLES_KEPT)
    expect(kept[0]!.key).toContain(`/${store.BUNDLES_KEPT + 1}.bundle`)
    expect(await store.repositoryByRemote(slug, `app`)).toEqual({ id: repositoryId, teamId })
    expect(await store.repositoryByRemote(slug, `nope`)).toBeNull()
  })
})
