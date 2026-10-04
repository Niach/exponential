import { beforeEach, describe, expect, it, vi } from "vitest"

// resolveRepoInstallationToken + resolveRepoDefaultBranch are stubbed —
// fetchBranchDiff / peekBranchDiff stay real so the cache tests exercise the true
// module-level state.
vi.mock(`@/lib/integrations/github-app`, async (importOriginal) => {
  const actual =
    // eslint-disable-next-line quotes -- `typeof import()` requires a string literal; the backtick autofix breaks it
    await importOriginal<typeof import("@/lib/integrations/github-app")>()
  return {
    ...actual,
    resolveRepoInstallationToken: vi.fn(),
    resolveRepoInstallationTokenInfo: vi.fn(),
    resolveRepoDefaultBranch: vi.fn(),
    githubAppConfigured: vi.fn(() => true),
  }
})

// resolveGatedRepoToken flags a drifted row through the module-level db.
const dbWrites = vi.hoisted(() => [] as Array<Record<string, unknown>>)
vi.mock(`@/db/connection`, () => ({
  db: {
    update: () => ({
      set: (values: Record<string, unknown>) => ({
        where: async () => {
          dbWrites.push(values)
        },
      }),
    }),
  },
}))

// The connect gate (SLOP-7: the caller's own GitHub token must see the repo
// with push access, and the App must be installed on it) lives in the
// integrations router module; stub it so connect tests drive it directly.
vi.mock(`@/lib/trpc/integrations`, () => ({
  resolveRepoForConnect: vi.fn(),
}))

// assertRepoManager resolves the actor's membership — mocked so the
// sharer-or-owner rules are testable without a db.
const assertTeamMember = vi.fn(
  async (..._args: unknown[]): Promise<{ role: string }> => ({
    role: `member`,
  })
)
vi.mock(`@/lib/team-membership`, () => ({
  assertTeamMember: (...args: unknown[]) => assertTeamMember(...args),
  getIssueTeamContext: vi.fn(),
}))

import {
  assertRepoManager,
  BRANCH_PREFIX_DEFAULT,
  connectRepositoryInTx,
  effectiveBoardBranch,
  effectiveDefaultBranch,
  healRepoDefaultBranches,
  installationDrifted,
  isForeignKeyViolation,
  issueBranchName,
  repoInUseMessage,
  resolveGatedRepoToken,
} from "@/lib/trpc/repositories"
import {
  fetchBranchDiff,
  peekBranchDiff,
  resolveRepoInstallationTokenInfo,
  type CompareFetch,
} from "@/lib/integrations/github-app"
import { resolveRepoForConnect } from "@/lib/trpc/integrations"

const mockResolveRepo = vi.mocked(resolveRepoForConnect)

function jsonResponse(status: number, body: unknown) {
  return {
    ok: status >= 200 && status < 300,
    status,
    json: async () => body,
  }
}

describe(`issueBranchName`, () => {
  it(`prefixes the identifier with the default exp/ prefix`, () => {
    expect(BRANCH_PREFIX_DEFAULT).toBe(`exp/`)
    expect(issueBranchName(`EXP-42`)).toBe(`exp/EXP-42`)
    expect(issueBranchName(`MET-1`)).toBe(`exp/MET-1`)
  })
})

describe(`isForeignKeyViolation`, () => {
  it(`detects Postgres 23503 on the error or its cause`, () => {
    expect(isForeignKeyViolation({ code: `23503` })).toBe(true)
    expect(isForeignKeyViolation({ cause: { code: `23503` } })).toBe(true)
  })

  it(`ignores unrelated errors`, () => {
    expect(isForeignKeyViolation({ code: `23505` })).toBe(false)
    expect(isForeignKeyViolation(new Error(`boom`))).toBe(false)
    expect(isForeignKeyViolation(null)).toBe(false)
    expect(isForeignKeyViolation(undefined)).toBe(false)
  })
})

describe(`repoInUseMessage`, () => {
  it(`pluralizes the board count`, () => {
    expect(repoInUseMessage(1)).toContain(`1 board.`)
    expect(repoInUseMessage(3)).toContain(`3 boards.`)
  })
})

describe(`fetchBranchDiff`, () => {
  const base = { repo: `acme/app`, base: `main`, token: `t0k3n` }

  it(`maps the compare response into the shared prFiles shape`, async () => {
    const fetchImpl = vi.fn<CompareFetch>().mockResolvedValue(
      jsonResponse(200, {
        files: [
          {
            filename: `src/app.rs`,
            status: `modified`,
            additions: 10,
            deletions: 2,
            patch: `@@ -1 +1 @@`,
          },
          {
            filename: `src/new.rs`,
            status: `added`,
            additions: 5,
            deletions: 0,
          },
        ],
      })
    )

    const result = await fetchBranchDiff({
      ...base,
      branch: `exp/UNIQ-1`,
      now: 1_000,
      fetchImpl,
    })

    expect(result).toEqual({
      repo: `acme/app`,
      prNumber: null,
      files: [
        {
          filename: `src/app.rs`,
          status: `modified`,
          additions: 10,
          deletions: 2,
          patch: `@@ -1 +1 @@`,
        },
        {
          filename: `src/new.rs`,
          status: `added`,
          additions: 5,
          deletions: 0,
          patch: undefined,
        },
      ],
    })
    // Compare API hit with the base...branch range and the auth token.
    const [url, init] = fetchImpl.mock.calls[0]
    expect(url).toContain(`/repos/acme/app/compare/main...exp/UNIQ-1`)
    expect(init.headers.authorization).toBe(`Bearer t0k3n`)
  })

  it(`returns null when the branch was never pushed (404)`, async () => {
    const fetchImpl = vi
      .fn<CompareFetch>()
      .mockResolvedValue(jsonResponse(404, { message: `Not Found` }))

    const result = await fetchBranchDiff({
      ...base,
      branch: `exp/UNIQ-404`,
      now: 2_000,
      fetchImpl,
    })
    expect(result).toBeNull()
  })

  it(`serves a cache hit within the ~60s window without re-fetching`, async () => {
    const fetchImpl = vi
      .fn<CompareFetch>()
      .mockResolvedValue(jsonResponse(200, { files: [] }))

    const first = await fetchBranchDiff({
      ...base,
      branch: `exp/UNIQ-CACHE`,
      now: 10_000,
      fetchImpl,
    })
    const second = await fetchBranchDiff({
      ...base,
      branch: `exp/UNIQ-CACHE`,
      now: 10_000 + 59_000,
      fetchImpl,
    })

    expect(first).toEqual({ repo: `acme/app`, prNumber: null, files: [] })
    expect(second).toEqual(first)
    expect(fetchImpl).toHaveBeenCalledTimes(1)
  })

  it(`re-fetches once the cache entry has expired`, async () => {
    const fetchImpl = vi
      .fn<CompareFetch>()
      .mockResolvedValue(jsonResponse(200, { files: [] }))

    await fetchBranchDiff({
      ...base,
      branch: `exp/UNIQ-EXPIRE`,
      now: 100_000,
      fetchImpl,
    })
    await fetchBranchDiff({
      ...base,
      branch: `exp/UNIQ-EXPIRE`,
      now: 100_000 + 61_000,
      fetchImpl,
    })
    expect(fetchImpl).toHaveBeenCalledTimes(2)
  })

  it(`throws on a non-404 GitHub error`, async () => {
    const fetchImpl = vi
      .fn<CompareFetch>()
      .mockResolvedValue(jsonResponse(500, { message: `boom` }))

    await expect(
      fetchBranchDiff({
        ...base,
        branch: `exp/UNIQ-500`,
        now: 3_000,
        fetchImpl,
      })
    ).rejects.toThrow(/GitHub compare failed \(500\)/)
  })

  it(`does NOT cache a 404 miss — a later call re-checks (manual Refresh)`, async () => {
    // Branch not pushed yet (404), then pushed (200) within the same TTL window.
    const fetchImpl = vi
      .fn<CompareFetch>()
      .mockResolvedValueOnce(jsonResponse(404, { message: `Not Found` }))
      .mockResolvedValueOnce(
        jsonResponse(200, {
          files: [
            { filename: `f.rs`, status: `added`, additions: 1, deletions: 0 },
          ],
        })
      )

    const miss = await fetchBranchDiff({
      ...base,
      branch: `exp/UNIQ-MISS`,
      now: 500_000,
      fetchImpl,
    })
    const hit = await fetchBranchDiff({
      ...base,
      branch: `exp/UNIQ-MISS`,
      now: 500_000 + 1_000, // still inside the 60s TTL
      fetchImpl,
    })

    expect(miss).toBeNull()
    // The null miss was not cached, so the second call re-fetched and saw 200.
    expect(fetchImpl).toHaveBeenCalledTimes(2)
    expect(hit).toEqual({
      repo: `acme/app`,
      prNumber: null,
      files: [
        { filename: `f.rs`, status: `added`, additions: 1, deletions: 0, patch: undefined },
      ],
    })
  })

  it(`keys the cache by base ref — a different base is a cache miss`, async () => {
    const fetchImpl = vi
      .fn<CompareFetch>()
      .mockResolvedValue(jsonResponse(200, { files: [] }))

    await fetchBranchDiff({
      repo: `acme/app`,
      base: `main`,
      branch: `exp/UNIQ-BASE`,
      token: `t0k3n`,
      now: 600_000,
      fetchImpl,
    })

    // Same repo + branch, cached under base=main.
    expect(peekBranchDiff(`acme/app`, `main`, `exp/UNIQ-BASE`, 600_000)).toEqual({
      repo: `acme/app`,
      prNumber: null,
      files: [],
    })
    // A different base ref must not collide onto that entry.
    expect(
      peekBranchDiff(`acme/app`, `develop`, `exp/UNIQ-BASE`, 600_000)
    ).toBeNull()
  })
})

// `add` and `boards.create`'s inline connect both route through this single
// helper (install-check → upsert → un-archive), so its semantics ARE the shared
// connect semantics. A minimal fake tx exercises each branch.
describe(`connectRepositoryInTx`, () => {
  beforeEach(() => {
    mockResolveRepo.mockReset()
    // Default: the caller's token sees the repo with push access on
    // installation 7 and GitHub reports `main` as its default branch.
    mockResolveRepo.mockResolvedValue({
      installationId: 7,
      private: true,
      defaultBranch: `main`,
    })
  })

  function makeTx(opts: {
    insert?: Array<{ id: string }>
    update?: Array<{ id: string }>
    captured?: { values?: Record<string, unknown> }
  }) {
    const insertChain = {
      values: (v: Record<string, unknown>) => {
        if (opts.captured) opts.captured.values = v
        return insertChain
      },
      onConflictDoNothing: () => insertChain,
      returning: async () => opts.insert ?? [],
    }
    const updateChain = {
      set: () => updateChain,
      where: () => updateChain,
      returning: async () => opts.update ?? [],
    }
    return {
      insert: () => insertChain,
      update: () => updateChain,
    }
  }

  const input = { userId: `u1`, teamId: `ws1`, fullName: `acme/app` }

  it(`returns the freshly inserted id and persists the resolved installation`, async () => {
    const captured: { values?: Record<string, unknown> } = {}
    const tx = makeTx({ insert: [{ id: `r1` }], captured })
    await expect(
      connectRepositoryInTx(tx as never, input)
    ).resolves.toBe(`r1`)
    // The gate runs on the CALLER (their token proves push access).
    expect(mockResolveRepo).toHaveBeenCalledWith(`u1`, `acme/app`)
    // The persisted id is GitHub's authoritative one, never a client claim.
    expect(captured.values?.installationId).toBe(7)
    expect(captured.values?.private).toBe(true)
    // EXP-557: the connector becomes the sharer.
    expect(captured.values?.sharedByUserId).toBe(`u1`)
  })

  it(`persists GitHub's default branch, even over a client-supplied one`, async () => {
    mockResolveRepo.mockResolvedValue({
      installationId: 7,
      private: false,
      defaultBranch: `master`,
    })
    const captured: { values?: Record<string, unknown> } = {}
    const tx = makeTx({ insert: [{ id: `r1` }], captured })
    await connectRepositoryInTx(tx as never, { ...input, defaultBranch: `develop` })
    expect(captured.values?.defaultBranch).toBe(`master`)
  })

  it(`falls back to the client's branch, then main, when GitHub names none`, async () => {
    mockResolveRepo.mockResolvedValue({
      installationId: 7,
      private: false,
      defaultBranch: ``,
    })
    const captured: { values?: Record<string, unknown> } = {}
    let tx = makeTx({ insert: [{ id: `r1` }], captured })
    await connectRepositoryInTx(tx as never, { ...input, defaultBranch: `develop` })
    expect(captured.values?.defaultBranch).toBe(`develop`)
    tx = makeTx({ insert: [{ id: `r1` }], captured })
    await connectRepositoryInTx(tx as never, input)
    expect(captured.values?.defaultBranch).toBe(`main`)
  })

  it(`un-archives and returns the existing id on conflict`, async () => {
    const tx = makeTx({ insert: [], update: [{ id: `r2` }] })
    await expect(
      connectRepositoryInTx(tx as never, input)
    ).resolves.toBe(`r2`)
  })

  it(`throws CONFLICT when the row was removed concurrently`, async () => {
    const tx = makeTx({ insert: [], update: [] })
    await expect(
      connectRepositoryInTx(tx as never, input)
    ).rejects.toThrow(/removed concurrently/)
  })

  it(`propagates the gate's rejection (not installed / no push access)`, async () => {
    mockResolveRepo.mockRejectedValue(
      new Error(`The Exponential GitHub App isn't installed on acme/app. Install it for acme on GitHub (and grant the repository), then try again.`)
    )
    const tx = makeTx({ insert: [{ id: `r1` }] })
    await expect(
      connectRepositoryInTx(tx as never, input)
    ).rejects.toThrow(/isn't installed/)
  })
})

// The `list` procedure heals stale default branches through this helper. A
// custom `resolve` + `persist` exercise each branch without a live GitHub call.
describe(`healRepoDefaultBranches`, () => {
  type HealPatch = { defaultBranch?: string; clearInaccessible?: boolean }
  const rows = [
    { id: `r1`, fullName: `acme/one`, defaultBranch: `main` },
    { id: `r2`, fullName: `acme/two`, defaultBranch: `main` },
  ]

  it(`returns the live value and persists the fix when the stored branch disagrees`, async () => {
    const persist = vi.fn<(id: string, patch: HealPatch) => Promise<void>>(
      async () => {}
    )
    const resolve = vi.fn(async (fullName: string) =>
      fullName === `acme/two` ? `master` : `main`
    )

    const healed = await healRepoDefaultBranches(rows, persist, resolve)

    expect(healed[0].defaultBranch).toBe(`main`)
    expect(healed[1].defaultBranch).toBe(`master`)
    // Only the disagreeing row is written.
    expect(persist).toHaveBeenCalledTimes(1)
    expect(persist).toHaveBeenCalledWith(`r2`, { defaultBranch: `master` })
  })

  it(`leaves the stored value when the live lookup yields nothing`, async () => {
    const persist = vi.fn<(id: string, patch: HealPatch) => Promise<void>>(
      async () => {}
    )
    const resolve = vi.fn(async () => null)

    const healed = await healRepoDefaultBranches(rows, persist, resolve)

    expect(healed.map((r) => r.defaultBranch)).toEqual([`main`, `main`])
    expect(persist).not.toHaveBeenCalled()
  })

  it(`clears a stale no-access flag when the live lookup succeeds`, async () => {
    // A resolved branch proves the App can reach the repo again — the heal
    // clears `inaccessibleAt` even when the branch itself didn't change.
    const flagged = {
      id: `r1`,
      fullName: `acme/one`,
      defaultBranch: `main`,
      inaccessibleAt: new Date(`2026-01-01T00:00:00Z`),
    }
    const persist = vi.fn<(id: string, patch: HealPatch) => Promise<void>>(
      async () => {}
    )
    const resolve = vi.fn(async () => `main`)

    const healed = await healRepoDefaultBranches([flagged], persist, resolve)

    expect(healed[0].inaccessibleAt).toBeNull()
    expect(persist).toHaveBeenCalledWith(`r1`, { clearInaccessible: true })
  })

  it(`never clears the no-access flag on a failed lookup`, async () => {
    const flagged = {
      id: `r1`,
      fullName: `acme/one`,
      defaultBranch: `main`,
      inaccessibleAt: new Date(`2026-01-01T00:00:00Z`),
    }
    const persist = vi.fn<(id: string, patch: HealPatch) => Promise<void>>(
      async () => {}
    )
    const resolve = vi.fn(async () => null)

    const healed = await healRepoDefaultBranches([flagged], persist, resolve)

    expect(healed[0].inaccessibleAt).toEqual(flagged.inaccessibleAt)
    expect(persist).not.toHaveBeenCalled()
  })

  it(`still returns the healed value when the persist write fails`, async () => {
    const persist = vi
      .fn<(id: string, patch: HealPatch) => Promise<void>>()
      .mockRejectedValue(new Error(`db down`))
    const resolve = vi.fn(async () => `master`)

    const healed = await healRepoDefaultBranches(
      [rows[0]],
      persist,
      resolve
    )

    expect(healed[0].defaultBranch).toBe(`master`)
  })

  it(`keeps the stored value when the resolve lookup throws`, async () => {
    const persist = vi.fn<(id: string, patch: HealPatch) => Promise<void>>(
      async () => {}
    )
    const resolve = vi.fn(async () => {
      throw new Error(`network`)
    })

    const healed = await healRepoDefaultBranches([rows[0]], persist, resolve)

    expect(healed[0].defaultBranch).toBe(`main`)
    expect(persist).not.toHaveBeenCalled()
  })

  it(`a team override rides through untouched — the heal only maintains the GitHub column (EXP-462)`, async () => {
    // The pinned branch must survive every heal pass: GitHub renaming its
    // default updates `defaultBranch` underneath, never the override.
    const pinned = {
      id: `r1`,
      fullName: `acme/one`,
      defaultBranch: `main`,
      defaultBranchOverride: `develop`,
    }
    const persist = vi.fn<(id: string, patch: HealPatch) => Promise<void>>(
      async () => {}
    )
    const resolve = vi.fn(async () => `master`)

    const healed = await healRepoDefaultBranches([pinned], persist, resolve)

    expect(healed[0].defaultBranch).toBe(`master`)
    expect(healed[0].defaultBranchOverride).toBe(`develop`)
    expect(persist).toHaveBeenCalledWith(`r1`, { defaultBranch: `master` })
    expect(effectiveDefaultBranch(healed[0])).toBe(`develop`)
  })
})

describe(`effectiveBoardBranch (EXP-712)`, () => {
  const repo = { defaultBranch: `main`, defaultBranchOverride: `develop` }

  it(`prefers the board's own branch over the team pin`, () => {
    expect(
      effectiveBoardBranch({ defaultBranch: `release/1.x` }, repo)
    ).toBe(`release/1.x`)
  })

  it(`falls back to the repo's effective default when the board pins nothing`, () => {
    expect(effectiveBoardBranch({ defaultBranch: null }, repo)).toBe(`develop`)
    expect(
      effectiveBoardBranch(
        { defaultBranch: null },
        { defaultBranch: `main`, defaultBranchOverride: null }
      )
    ).toBe(`main`)
  })
})

describe(`effectiveDefaultBranch (EXP-462)`, () => {
  it(`prefers the team override and falls back to the GitHub column`, () => {
    expect(
      effectiveDefaultBranch({
        defaultBranch: `master`,
        defaultBranchOverride: `develop`,
      })
    ).toBe(`develop`)
    expect(
      effectiveDefaultBranch({
        defaultBranch: `master`,
        defaultBranchOverride: null,
      })
    ).toBe(`master`)
  })
})

// EXP-557 per-user sharing: `remove` and `setDefaultBranch` route through this
// single gate — the member who shared the repo or a team owner.
describe(`assertRepoManager (EXP-557)`, () => {
  beforeEach(() => {
    assertTeamMember.mockClear()
    assertTeamMember.mockResolvedValue({ role: `member` })
  })

  it(`allows the sharer (a plain member)`, async () => {
    await expect(
      assertRepoManager(`u1`, { teamId: `ws1`, sharedByUserId: `u1` })
    ).resolves.toBeUndefined()
    expect(assertTeamMember).toHaveBeenCalledWith(`u1`, `ws1`)
  })

  it(`allows an owner who is not the sharer`, async () => {
    assertTeamMember.mockResolvedValue({ role: `owner` })
    await expect(
      assertRepoManager(`owner-1`, { teamId: `ws1`, sharedByUserId: `u1` })
    ).resolves.toBeUndefined()
  })

  it(`refuses a plain member who is not the sharer`, async () => {
    await expect(
      assertRepoManager(`u2`, { teamId: `ws1`, sharedByUserId: `u1` })
    ).rejects.toMatchObject({ code: `FORBIDDEN` })
  })

  it(`a NULL sharer (pre-EXP-557 row) is owner-only`, async () => {
    await expect(
      assertRepoManager(`u1`, { teamId: `ws1`, sharedByUserId: null })
    ).rejects.toMatchObject({ code: `FORBIDDEN` })
    assertTeamMember.mockResolvedValue({ role: `owner` })
    await expect(
      assertRepoManager(`owner-1`, { teamId: `ws1`, sharedByUserId: null })
    ).resolves.toBeUndefined()
  })

  it(`membership itself is still required (non-member throws through)`, async () => {
    assertTeamMember.mockRejectedValue(new Error(`FORBIDDEN`))
    await expect(
      assertRepoManager(`stranger`, { teamId: `ws1`, sharedByUserId: `stranger` })
    ).rejects.toThrow(/FORBIDDEN/)
  })
})

describe(`installation drift (SLOP-7 review)`, () => {
  const repo = { id: `r1`, teamId: `t1`, fullName: `acme/app`, installationId: 11 }
  const mockResolve = vi.mocked(resolveRepoInstallationTokenInfo)

  beforeEach(() => {
    dbWrites.length = 0
    mockResolve.mockReset()
  })

  it(`a different stored installation is drift; a NULL one heals`, () => {
    expect(installationDrifted(11, 11)).toBe(false)
    expect(installationDrifted(11, 22)).toBe(true)
    expect(installationDrifted(null, 22)).toBe(false)
  })

  it(`resolveGatedRepoToken hands out the token of the stored installation`, async () => {
    mockResolve.mockResolvedValue({ token: `tok`, expiresAt: 0, installationId: 11 })
    expect(await resolveGatedRepoToken(repo)).toBe(`tok`)
    expect(dbWrites).toEqual([])
  })

  it(`resolveGatedRepoToken refuses a drifted installation and flags the row`, async () => {
    mockResolve.mockResolvedValue({ token: `foreign`, expiresAt: 0, installationId: 22 })
    expect(await resolveGatedRepoToken(repo)).toBeNull()
    expect(dbWrites).toHaveLength(1)
    expect(dbWrites[0]!.inaccessibleAt).toBeInstanceOf(Date)
  })

  it(`resolveGatedRepoToken still serves a legacy NULL row`, async () => {
    mockResolve.mockResolvedValue({ token: `tok`, expiresAt: 0, installationId: 22 })
    expect(await resolveGatedRepoToken({ ...repo, installationId: null })).toBe(`tok`)
    expect(dbWrites).toEqual([])
  })
})
