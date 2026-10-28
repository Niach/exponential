// @vitest-environment node
import { afterAll, describe, expect, it } from "vitest"
import { eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { pullRequests, repositories, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

// FEED-160/FEED-165: `applyPullRequestDraft`'s `notBefore` guard is SQL
// (`github_updated_at IS NULL OR <= notBefore`); the unit suite mocks the
// db, so this runs it against the REAL migrated Postgres. A late generic
// delivery older than the last applied PR state never flips the draft flag.
// `applyPullRequestDraft` writes through the shared pool (no transaction to
// roll back), so each case builds its own team and the suite deletes them.

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.columns
    WHERE table_name = 'pull_requests' AND column_name = 'github_updated_at') AS ok
`)
if (!ready.ok) console.info(`[pr-sync.real-db] skipped: ${ready.reason}`)

const { applyPullRequestDraft, applyPullRequestReopened } = await import(`@/lib/integrations/pr-sync`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made: string[] = []

afterAll(async () => {
  if (!ready.ok || made.length === 0) return
  await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.join(`','`)}')`)}`)
})

const at = (iso: string) => new Date(iso)

/** A PR row in `state`, its last applied GitHub state at `githubUpdatedAt`. */
async function pr(state: `open` | `draft` | `closed`, githubUpdatedAt: Date | null) {
  const [team] = await db.insert(teams).values({ name: `Sync`, slug: `sync-${id8()}` }).returning({ id: teams.id })
  made.push(team!.id)
  const fullName = `o/sync-${id8()}`
  const [repo] = await db
    .insert(repositories)
    .values({ teamId: team!.id, fullName, defaultBranch: `main` })
    .returning({ id: repositories.id })
  const [row] = await db
    .insert(pullRequests)
    .values({
      teamId: team!.id,
      repositoryId: repo!.id,
      number: 1,
      url: `https://github.com/${fullName}/pull/1`,
      head: `feature`,
      base: `main`,
      state,
      githubUpdatedAt,
    })
    .returning({ id: pullRequests.id })
  return row!.id
}

const read = async (id: string) =>
  (
    await db
      .select({ state: pullRequests.state, githubUpdatedAt: pullRequests.githubUpdatedAt })
      .from(pullRequests)
      .where(eq(pullRequests.id, id))
  )[0]!

describe.skipIf(!ready.ok)(`FEED-160: the github_updated_at guard`, () => {
  it(`a generic delivery OLDER than the last applied state never flips the draft flag`, async () => {
    const id = await pr(`open`, at(`2026-10-01T12:00:00Z`))
    expect(await applyPullRequestDraft({ pullRequestId: id, draft: true, notBefore: at(`2026-10-01T11:59:59Z`) })).toBeNull()
    expect(await read(id)).toEqual({ state: `open`, githubUpdatedAt: at(`2026-10-01T12:00:00Z`) })
  })

  it(`a delivery as new or newer flips it and stores its time`, async () => {
    const same = await pr(`open`, at(`2026-10-01T12:00:00Z`))
    expect(await applyPullRequestDraft({ pullRequestId: same, draft: true, notBefore: at(`2026-10-01T12:00:00Z`) })).toEqual({
      rootSessionId: null,
    })
    expect(await read(same)).toEqual({ state: `draft`, githubUpdatedAt: at(`2026-10-01T12:00:00Z`) })
    const newer = await pr(`draft`, at(`2026-10-01T12:00:00Z`))
    expect(await applyPullRequestDraft({ pullRequestId: newer, draft: false, notBefore: at(`2026-10-02T08:00:00Z`) })).not.toBeNull()
    expect(await read(newer)).toEqual({ state: `open`, githubUpdatedAt: at(`2026-10-02T08:00:00Z`) })
  })

  it(`a row with no applied state yet (NULL) takes any delivery`, async () => {
    const id = await pr(`draft`, null)
    expect(await applyPullRequestDraft({ pullRequestId: id, draft: false, notBefore: at(`2020-01-01T00:00:00Z`) })).not.toBeNull()
    expect(await read(id)).toEqual({ state: `open`, githubUpdatedAt: at(`2020-01-01T00:00:00Z`) })
  })

  it(`an explicit state (no notBefore) applies whatever the stored time, and moves it`, async () => {
    const id = await pr(`open`, at(`2026-10-05T00:00:00Z`))
    expect(await applyPullRequestDraft({ pullRequestId: id, draft: true, githubUpdatedAt: at(`2026-10-04T00:00:00Z`) })).not.toBeNull()
    expect(await read(id)).toEqual({ state: `draft`, githubUpdatedAt: at(`2026-10-04T00:00:00Z`) })
  })

  it(`a reopen stores its delivery time, so an older generic delivery after it is ignored`, async () => {
    const id = await pr(`closed`, at(`2026-10-01T00:00:00Z`))
    await applyPullRequestReopened({ pullRequestId: id, draft: false, githubUpdatedAt: at(`2026-10-03T00:00:00Z`) })
    expect(await read(id)).toEqual({ state: `open`, githubUpdatedAt: at(`2026-10-03T00:00:00Z`) })
    expect(await applyPullRequestDraft({ pullRequestId: id, draft: true, notBefore: at(`2026-10-02T00:00:00Z`) })).toBeNull()
    expect((await read(id)).state).toBe(`open`)
  })
})
