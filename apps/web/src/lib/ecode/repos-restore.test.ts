import { beforeEach, describe, expect, it, vi } from "vitest"

// #EXP-2201: a restore never silently rolls branches back: it names the refs
// a backup would change and refuses without `force`, and it takes (and waits
// for) a fresh backup of the current state before it queues the restore.

const h = vi.hoisted(() => ({
  refs: {} as Record<string, string>,
  bundles: [] as Array<{ id: string; key: string; bytes: number; refs: Record<string, string>; createdAt: Date }>,
  queued: [] as Array<{ op: string }>,
  freshStatus: `done` as string | null,
}))
vi.mock(`@/db/connection`, () => {
  const row = { id: `repo-1`, teamId: `team-1`, provider: `ecode`, fullName: `acme/dog`, remoteUrl: `exp://acme/dog`, defaultBranch: `main`, defaultBranchOverride: null, providerConfig: {}, archivedAt: null }
  const chain = { from: () => chain, where: () => chain, limit: async () => [row] }
  return { db: { select: () => chain } }
})
vi.mock(`@/lib/auth/access`, () => ({ resolveTeamAccess: vi.fn(async () => ({ role: `owner` })) }))
vi.mock(`@/lib/ecode/git-store`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/ecode/git-store")>()),
  listRefs: async () => h.refs,
  listBundles: async () => h.bundles,
}))
vi.mock(`@/lib/ecode/jobs`, () => ({
  queueMaintenance: vi.fn(async (_repo: unknown, job: { op: string }) => {
    h.queued.push(job)
    return `cmd-${job.op}`
  }),
}))
vi.mock(`@/lib/trpc/devices`, () => ({
  awaitDeviceCommand: vi.fn(async () => (h.freshStatus ? { status: h.freshStatus, result: `{"ok":true}` } : null)),
}))

const { restoreEcodeBackup, restoreRefChanges } = await import(`@/lib/ecode/repos`)

const A = `a`.repeat(40)
const B = `b`.repeat(40)
const C = `c`.repeat(40)
const bundle = (refs: Record<string, string>) => ({ id: `bk-1`, key: `k/bundle`, bytes: 1, refs, createdAt: new Date() })

beforeEach(() => {
  h.queued.length = 0
  h.freshStatus = `done`
  h.refs = { "refs/heads/main": B, "refs/heads/exp/DOG-2": C }
})

describe(`restoreRefChanges`, () => {
  it(`names every ref that rolls back, goes or comes back`, () => {
    expect(restoreRefChanges({ "refs/heads/main": B, "refs/heads/x": C }, { "refs/heads/main": A, "refs/heads/old": A })).toEqual([
      { ref: `refs/heads/main`, from: B, to: A },
      { ref: `refs/heads/old`, from: null, to: A },
      { ref: `refs/heads/x`, from: C, to: null },
    ])
    expect(restoreRefChanges({ "refs/heads/main": B }, { "refs/heads/main": B })).toEqual([])
  })
})

describe(`restoreEcodeBackup`, () => {
  it(`refuses an older backup without force, naming the branches, and queues nothing`, async () => {
    h.bundles = [bundle({ "refs/heads/main": A })]
    await expect(restoreEcodeBackup(`u-1`, { repositoryId: `repo-1`, backupId: `bk-1` })).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: expect.stringMatching(/exp\/DOG-2 \(removed\), main \(rolled back\)\. Pass force/),
    })
    expect(h.queued).toEqual([])
  })

  it(`with force (or an identical backup) takes a fresh backup first, then restores`, async () => {
    h.bundles = [bundle({ "refs/heads/main": A })]
    const forced = await restoreEcodeBackup(`u-1`, { repositoryId: `repo-1`, backupId: `bk-1`, force: true })
    expect(h.queued.map((j) => j.op)).toEqual([`backup`, `restore`])
    expect(forced.changes).toHaveLength(2)
    h.queued.length = 0
    h.bundles = [bundle({ ...h.refs })]
    expect((await restoreEcodeBackup(`u-1`, { repositoryId: `repo-1`, backupId: `bk-1` })).changes).toEqual([])
    expect(h.queued.map((j) => j.op)).toEqual([`backup`, `restore`])
  })

  it(`never restores when the fresh backup did not finish`, async () => {
    h.bundles = [bundle({ ...h.refs })]
    h.freshStatus = null
    await expect(restoreEcodeBackup(`u-1`, { repositoryId: `repo-1`, backupId: `bk-1` })).rejects.toMatchObject({ code: `PRECONDITION_FAILED` })
    expect(h.queued.map((j) => j.op)).toEqual([`backup`])
  })
})
