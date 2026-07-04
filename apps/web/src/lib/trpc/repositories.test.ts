import { beforeEach, describe, expect, it, vi } from "vitest"

// Only resolveRepoInstallationToken is stubbed — fetchBranchDiff / peekBranchDiff
// stay real so the cache tests exercise the true module-level state.
vi.mock("@/lib/integrations/github-app", async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("@/lib/integrations/github-app")>()
  return { ...actual, resolveRepoInstallationToken: vi.fn() }
})

import {
  BRANCH_PREFIX_DEFAULT,
  connectRepositoryInTx,
  isForeignKeyViolation,
  issueBranchName,
  repoInUseMessage,
} from "@/lib/trpc/repositories"
import {
  fetchBranchDiff,
  peekBranchDiff,
  resolveRepoInstallationToken,
  type CompareFetch,
} from "@/lib/integrations/github-app"

const mockResolveToken = vi.mocked(resolveRepoInstallationToken)

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
  it(`pluralizes the project count`, () => {
    expect(repoInUseMessage(1)).toContain(`1 project.`)
    expect(repoInUseMessage(3)).toContain(`3 projects.`)
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

// `add` and `projects.create`'s inline connect both route through this single
// helper (install-check → upsert → un-archive), so its semantics ARE the shared
// connect semantics. A minimal fake tx exercises each branch.
describe(`connectRepositoryInTx`, () => {
  beforeEach(() => mockResolveToken.mockReset())

  function makeTx(opts: {
    insert?: Array<{ id: string }>
    update?: Array<{ id: string }>
  }) {
    const insertChain = {
      values: () => insertChain,
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

  const input = { workspaceId: `ws1`, fullName: `acme/app` }

  it(`returns the freshly inserted id`, async () => {
    mockResolveToken.mockResolvedValue(`tok`)
    const tx = makeTx({ insert: [{ id: `r1` }] })
    await expect(
      connectRepositoryInTx(tx as never, input)
    ).resolves.toBe(`r1`)
  })

  it(`un-archives and returns the existing id on conflict`, async () => {
    mockResolveToken.mockResolvedValue(`tok`)
    const tx = makeTx({ insert: [], update: [{ id: `r2` }] })
    await expect(
      connectRepositoryInTx(tx as never, input)
    ).resolves.toBe(`r2`)
  })

  it(`throws CONFLICT when the row was removed concurrently`, async () => {
    mockResolveToken.mockResolvedValue(`tok`)
    const tx = makeTx({ insert: [], update: [] })
    await expect(
      connectRepositoryInTx(tx as never, input)
    ).rejects.toThrow(/removed concurrently/)
  })

  it(`throws PRECONDITION_FAILED when the App isn't installed`, async () => {
    mockResolveToken.mockResolvedValue(null)
    const tx = makeTx({ insert: [{ id: `r1` }] })
    await expect(
      connectRepositoryInTx(tx as never, input)
    ).rejects.toThrow(/not installed/)
  })
})
