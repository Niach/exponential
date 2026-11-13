import { beforeEach, describe, expect, it, vi } from "vitest"

// #EXP-1616: the wire of eCode's own git (`/api/ecode/git/<op>`): the repo
// token is the bearer, membership is re-checked per call, job-only ops stay
// job-only, uploads are scoped to the repo's prefix and sized by the bucket,
// and a push is ONE CAS (a moved ref = 409 + the ref).

const TEAM = `11111111-1111-4111-8111-111111111111`
const REPO = `22222222-2222-4222-8222-222222222222`
const SHA_A = `a`.repeat(40)
const SHA_B = `b`.repeat(40)

const h = vi.hoisted(() => ({
  repo: null as Record<string, unknown> | null,
  member: true,
  pushes: [] as unknown[],
  pushResult: { ok: true } as { ok: true } | { ok: false; rejected: string },
  bundles: [] as unknown[],
  restores: [] as unknown[],
  storageChecks: [] as number[],
}))

vi.mock(`@/db/connection`, () => ({
  db: {
    select: () => ({ from: () => ({ where: () => ({ limit: async () => (h.repo ? [h.repo] : []) }) }) }),
  },
}))
vi.mock(`@/lib/auth/access`, () => ({
  resolveTeamAccess: async () => {
    if (!h.member) {
      const { TRPCError } = await import(`@trpc/server`)
      throw new TRPCError({ code: `FORBIDDEN`, message: `Not a member` })
    }
    return { kind: `member` }
  },
}))
vi.mock(`@/lib/ecode/git-store`, async (original) => ({
  ...(await original<typeof import("@/lib/ecode/git-store")>()),
  listRefs: async () => ({ "refs/heads/main": SHA_A }),
  livePacks: async () => [{ id: `p1`, key: `k1`, bytes: 10, seq: 1 }],
  recordPush: async (args: unknown) => {
    h.pushes.push(args)
    return h.pushResult
  },
  recordBundle: async (args: unknown) => {
    h.bundles.push(args)
  },
  replaceRepository: async (args: unknown) => {
    h.restores.push(args)
  },
}))
vi.mock(`@/lib/storage`, () => ({
  presignGet: async (key: string) => `https://bucket.test/${key}?sig=get`,
  presignPut: async (key: string) => `https://bucket.test/${key}?sig=put`,
  headObject: async () => ({ sizeBytes: 1234, contentType: null }),
}))
vi.mock(`@/lib/billing`, () => ({
  assertWithinStorageLimit: async (_team: string, bytes: number) => {
    h.storageChecks.push(bytes)
  },
}))
vi.mock(`@/lib/ecode/ecode-host`, () => ({ pgEcodeHost: { publish: async () => {} } }))

process.env.BETTER_AUTH_SECRET = `test-secret`
const { handleGitRequest } = await import(`@/lib/ecode/git-routes`)
const { mintRepoToken, verifyRepoToken } = await import(`@/lib/ecode/repo-token`)

const member = () => mintRepoToken({ repositoryId: REPO, userId: `u1` }).token
const job = () => mintRepoToken({ repositoryId: REPO, userId: `u1`, scope: `job` }).token

function call(op: string, token: string | null, init: { method?: string; body?: unknown; query?: string } = {}) {
  return handleGitRequest(
    new Request(`https://app.test/api/ecode/git/${op}${init.query ?? ``}`, {
      method: init.method ?? (init.body ? `POST` : `GET`),
      headers: token ? { authorization: `Bearer ${token}`, "content-type": `application/json` } : {},
      ...(init.body ? { body: JSON.stringify(init.body) } : {}),
    }),
    op
  )
}

beforeEach(() => {
  h.repo = { id: REPO, teamId: TEAM, fullName: `team/app`, defaultBranch: `main`, override: null, provider: `ecode`, archivedAt: null }
  h.member = true
  h.pushes = []
  h.pushResult = { ok: true }
  h.bundles = []
  h.restores = []
  h.storageChecks = []
})

describe(`the repo token`, () => {
  it(`round-trips, expires and refuses a tampered or foreign token`, () => {
    const { token, expiresAt } = mintRepoToken({ repositoryId: REPO, userId: `u1`, now: 1_000 })
    expect(verifyRepoToken(token, 2_000)).toMatchObject({ r: REPO, u: `u1`, s: `member` })
    expect(verifyRepoToken(token, expiresAt.getTime())).toBeNull()
    expect(verifyRepoToken(token.slice(0, -2) + `xx`, 2_000)).toBeNull()
    expect(verifyRepoToken(`ghs_abc`, 2_000)).toBeNull()
    process.env.BETTER_AUTH_SECRET = `another`
    expect(verifyRepoToken(token, 2_000)).toBeNull()
    process.env.BETTER_AUTH_SECRET = `test-secret`
  })
})

describe(`/api/ecode/git (#EXP-1616)`, () => {
  it(`refuses a request without a valid token, a non-ecode repo and a removed member`, async () => {
    expect((await call(`refs`, null)).status).toBe(401)
    h.repo = { ...h.repo!, provider: `github` }
    expect((await call(`refs`, member())).status).toBe(404)
    h.repo = { ...h.repo!, provider: `ecode` }
    h.member = false
    expect((await call(`refs`, member())).status).toBe(403)
  })

  it(`lists the refs with the default branch as HEAD, and the packs as presigned GETs`, async () => {
    expect(await (await call(`refs`, member())).json()).toEqual({ head: `refs/heads/main`, refs: { "refs/heads/main": SHA_A } })
    expect(await (await call(`packs`, member(), { query: `?after=0` })).json()).toEqual({
      packs: [{ id: `p1`, seq: 1, bytes: 10, url: `https://bucket.test/k1?sig=get` }],
    })
  })

  it(`hands out an upload under the repo's prefix after the storage check; bundles are job-only`, async () => {
    const res = await call(`upload`, member(), { body: { kind: `pack`, bytes: 99 } })
    const out = (await res.json()) as { key: string; url: string }
    expect(out.key).toMatch(new RegExp(`^ecode/${TEAM}/${REPO}/packs/[0-9a-f-]+\\.pack$`))
    expect(out.url).toContain(`sig=put`)
    expect(h.storageChecks).toEqual([99])
    expect((await call(`upload`, member(), { body: { kind: `bundle`, bytes: 5 } })).status).toBe(403)
    expect((await call(`upload`, job(), { body: { kind: `bundle`, bytes: 5 } })).status).toBe(200)
  })

  it(`a push records the pack at the bucket's size and CASes its refs; a moved ref is 409`, async () => {
    const pack = `ecode/${TEAM}/${REPO}/packs/x.pack`
    const ok = await call(`push`, member(), { body: { pack, updates: [{ ref: `refs/heads/topic`, old: null, new: SHA_B }] } })
    expect(ok.status).toBe(200)
    expect(h.pushes[0]).toMatchObject({ repositoryId: REPO, pack: { key: pack, bytes: 1234 }, updates: [{ ref: `refs/heads/topic`, old: null, new: SHA_B }] })
    h.pushResult = { ok: false, rejected: `refs/heads/main` }
    const moved = await call(`push`, member(), { body: { pack: null, updates: [{ ref: `refs/heads/main`, old: SHA_B, new: SHA_A }] } })
    expect(moved.status).toBe(409)
    expect(await moved.json()).toEqual({ ok: false, rejected: `refs/heads/main` })
  })

  it(`refuses a foreign pack key, a bad ref, a non-branch ref and deleting the default branch`, async () => {
    const bad = async (body: unknown) => (await call(`push`, member(), { body })).status
    expect(await bad({ pack: `ecode/other/x/packs/a.pack`, updates: [] })).toBe(400)
    expect(await bad({ updates: [{ ref: `refs/heads/../x`, old: null, new: SHA_A }] })).toBe(400)
    expect(await bad({ updates: [{ ref: `refs/notes/x`, old: null, new: SHA_A }] })).toBe(400)
    expect(await bad({ updates: [{ ref: `refs/heads/main`, old: SHA_A, new: null }] })).toBe(403)
    expect(h.pushes).toEqual([])
  })

  it(`backup, restore and object reads are job-only`, async () => {
    const bundle = `ecode/${TEAM}/${REPO}/bundles/b.bundle`
    expect((await call(`bundle`, member(), { body: { key: bundle, refs: {} } })).status).toBe(403)
    expect((await call(`bundle`, job(), { body: { key: bundle, refs: { "refs/heads/main": SHA_A } } })).status).toBe(200)
    expect(h.bundles[0]).toMatchObject({ key: bundle, bytes: 1234, refs: { "refs/heads/main": SHA_A } })
    expect((await call(`object`, member(), { query: `?key=${bundle}` })).status).toBe(403)
    expect(await (await call(`object`, job(), { query: `?key=${bundle}` })).json()).toEqual({ url: `https://bucket.test/${bundle}?sig=get` })
    const pack = `ecode/${TEAM}/${REPO}/packs/r.pack`
    expect((await call(`restore`, member(), { body: { pack, refs: {} } })).status).toBe(403)
    expect((await call(`restore`, job(), { body: { pack, refs: { "refs/heads/main": SHA_A } } })).status).toBe(200)
    expect(h.restores[0]).toMatchObject({ pack: { key: pack, bytes: 1234 }, refs: { "refs/heads/main": SHA_A } })
  })
})
