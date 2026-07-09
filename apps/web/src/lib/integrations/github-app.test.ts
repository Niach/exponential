import { afterEach, describe, expect, it, vi } from "vitest"

import {
  listUserInstallationRepos,
  resolveInstallationTokenWith,
} from "@/lib/integrations/github-app"

// resolveInstallationTokenWith is the pure resolution policy behind
// resolveRepoInstallationToken — GitHub's round-trips (per-repo installation
// lookup + token mint + repo-access probe) are injected, so it can be exercised
// without a real App JWT. These cases pin the fallback behavior that fixes the
// spurious 412 AND the verification that stops the fallback from minting
// tokens that can't reach a repo removed from the installation's selection.
describe(`resolveInstallationTokenWith`, () => {
  const repo = `Niach/exponential`

  it(`mints against the live installation id and skips the verify probe on a hit`, async () => {
    const resolveId = vi.fn(async () => 42)
    const mintToken = vi.fn(async () => `tok-live`)
    const verifyRepo = vi.fn(async () => true)

    const resolved = await resolveInstallationTokenWith(repo, 7, {
      resolveId,
      mintToken,
      verifyRepo,
    })

    expect(resolved).toEqual({ token: `tok-live`, installationId: 42 })
    expect(mintToken).toHaveBeenCalledTimes(1)
    expect(mintToken).toHaveBeenCalledWith(42)
    expect(verifyRepo).not.toHaveBeenCalled()
  })

  it(`falls back to the stored installation id when the live lookup 404s and the probe passes`, async () => {
    // GitHub's per-repo endpoint reports "not installed" (null) even though the
    // App still covers the repo through the installation persisted at connect
    // time — mint against that instead of 412ing, verified by the repo probe.
    const resolveId = vi.fn(async () => null)
    const mintToken = vi.fn(async () => `tok-fallback`)
    const verifyRepo = vi.fn(async () => true)

    const resolved = await resolveInstallationTokenWith(repo, 7, {
      resolveId,
      mintToken,
      verifyRepo,
    })

    expect(resolved).toEqual({ token: `tok-fallback`, installationId: 7 })
    expect(mintToken).toHaveBeenCalledTimes(1)
    expect(mintToken).toHaveBeenCalledWith(7)
    expect(verifyRepo).toHaveBeenCalledWith(repo, `tok-fallback`)
  })

  it(`returns null when the fallback token can't reach the repo (really removed)`, async () => {
    // The repo was dropped from the installation's "Only select repositories"
    // set: the mint succeeds (the installation exists) but the token can't
    // access the repo — the old blind fallback handed that token out anyway.
    const resolveId = vi.fn(async () => null)
    const mintToken = vi.fn(async () => `tok-dead`)
    const verifyRepo = vi.fn(async () => false)

    const resolved = await resolveInstallationTokenWith(repo, 7, {
      resolveId,
      mintToken,
      verifyRepo,
    })

    expect(resolved).toBeNull()
  })

  it(`returns the fallback token when the verify probe THROWS (transient error ≠ no access)`, async () => {
    const resolveId = vi.fn(async () => null)
    const mintToken = vi.fn(async () => `tok-fallback`)
    const verifyRepo = vi.fn(async () => {
      throw new Error(`GitHub repo probe failed (500)`)
    })

    const resolved = await resolveInstallationTokenWith(repo, 7, {
      resolveId,
      mintToken,
      verifyRepo,
    })

    expect(resolved).toEqual({ token: `tok-fallback`, installationId: 7 })
  })

  it(`returns null when the live lookup misses and there is no fallback`, async () => {
    const resolveId = vi.fn(async () => null)
    const mintToken = vi.fn(async () => `never`)
    const verifyRepo = vi.fn(async () => true)

    const resolved = await resolveInstallationTokenWith(repo, null, {
      resolveId,
      mintToken,
      verifyRepo,
    })

    expect(resolved).toBeNull()
    expect(mintToken).not.toHaveBeenCalled()
  })

  it(`returns null (not a throw) when the fallback mint fails — App genuinely lost access`, async () => {
    const resolveId = vi.fn(async () => null)
    const mintToken = vi.fn(async () => {
      throw new Error(`GitHub installation token failed (404)`)
    })
    const verifyRepo = vi.fn(async () => true)

    const resolved = await resolveInstallationTokenWith(repo, 7, {
      resolveId,
      mintToken,
      verifyRepo,
    })

    expect(resolved).toBeNull()
    expect(verifyRepo).not.toHaveBeenCalled()
  })

  it(`propagates a throw on the LIVE mint (a transient error must not read as "not installed")`, async () => {
    const resolveId = vi.fn(async () => 42)
    const mintToken = vi.fn(async () => {
      throw new Error(`GitHub installation token failed (500)`)
    })
    const verifyRepo = vi.fn(async () => true)

    await expect(
      resolveInstallationTokenWith(repo, 7, { resolveId, mintToken, verifyRepo })
    ).rejects.toThrow(/500/)
  })
})

// listUserInstallationRepos is the grant-capture listing: the repos of ONE
// installation as the OAuth'd USER can access them (`GET /user/installations/
// {id}/repositories`, user token) — the user-scoped counterpart of
// listAllInstallationRepos. These cases pin the endpoint/auth, the
// InstallationRepo mapping, pagination-to-completion, the maxPages cap
// (hasMore), full_name dedup, and the error throw.
describe(`listUserInstallationRepos`, () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  function pageResponse(
    totalCount: number,
    repos: Array<{ full_name: string; private: boolean; default_branch: string }>
  ) {
    return {
      ok: true,
      status: 200,
      json: async () => ({ total_count: totalCount, repositories: repos }),
    }
  }

  it(`paginates to completion and maps into the InstallationRepo shape`, async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(
        pageResponse(
          150,
          Array.from({ length: 100 }, (_, i) => ({
            full_name: `acme/repo-${i}`,
            private: i % 2 === 0,
            default_branch: `main`,
          }))
        )
      )
      .mockResolvedValueOnce(
        pageResponse(
          150,
          Array.from({ length: 50 }, (_, i) => ({
            full_name: `acme/repo-${100 + i}`,
            private: false,
            default_branch: `master`,
          }))
        )
      )
    vi.stubGlobal(`fetch`, fetchMock)

    const result = await listUserInstallationRepos(`user-tok`, 42)

    expect(result.hasMore).toBe(false)
    expect(result.repos).toHaveLength(150)
    expect(result.repos[0]).toEqual({
      fullName: `acme/repo-0`,
      private: true,
      defaultBranch: `main`,
      installationId: 42,
    })
    expect(result.repos[149]).toEqual({
      fullName: `acme/repo-149`,
      private: false,
      defaultBranch: `master`,
      installationId: 42,
    })
    // Hits the USER-scoped endpoint with the user token — never the
    // installation-token listing.
    const [url1, init1] = fetchMock.mock.calls[0] as [
      string,
      { headers: Record<string, string> },
    ]
    expect(url1).toBe(
      `https://api.github.com/user/installations/42/repositories?per_page=100&page=1`
    )
    expect(init1.headers.authorization).toBe(`Bearer user-tok`)
    const [url2] = fetchMock.mock.calls[1] as [string]
    expect(url2).toContain(`page=2`)
  })

  it(`stops at a single short page without a second round-trip`, async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      pageResponse(2, [
        { full_name: `acme/a`, private: false, default_branch: `main` },
        { full_name: `acme/b`, private: true, default_branch: `main` },
      ])
    )
    vi.stubGlobal(`fetch`, fetchMock)

    const result = await listUserInstallationRepos(`user-tok`, 7)

    expect(fetchMock).toHaveBeenCalledTimes(1)
    expect(result.hasMore).toBe(false)
    expect(result.repos.map((r) => r.fullName)).toEqual([`acme/a`, `acme/b`])
  })

  it(`caps at maxPages, reports hasMore, and dedups by full_name`, async () => {
    // The same full page every time (as a misbehaving/paging-drifted API
    // would): the cap stops the loop, hasMore flags the truncation, and the
    // dedup keeps each repo once.
    const fetchMock = vi.fn().mockResolvedValue(
      pageResponse(
        500,
        Array.from({ length: 100 }, (_, i) => ({
          full_name: `acme/r${i}`,
          private: false,
          default_branch: `main`,
        }))
      )
    )
    vi.stubGlobal(`fetch`, fetchMock)

    const result = await listUserInstallationRepos(`user-tok`, 7, {
      maxPages: 2,
    })

    expect(fetchMock).toHaveBeenCalledTimes(2)
    expect(result.hasMore).toBe(true)
    expect(result.repos).toHaveLength(100)
  })

  it(`throws on a GitHub error status`, async () => {
    vi.stubGlobal(
      `fetch`,
      vi.fn().mockResolvedValue({ ok: false, status: 401, json: async () => ({}) })
    )
    await expect(listUserInstallationRepos(`bad-tok`, 7)).rejects.toThrow(
      /GitHub user installation repos failed \(401\)/
    )
  })
})
