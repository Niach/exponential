import crypto from "node:crypto"

import { afterEach, describe, expect, it, vi } from "vitest"

import {
  BRANCH_DIFF_CACHE_MAX,
  evictStaleEntries,
  fetchBranchDiff,
  getAuthenticatedGithubLogin,
  getUserOrgMembershipState,
  listUserInstallationRepos,
  partitionControlledInstallations,
  peekBranchDiff,
  resolveInstallationTokenWith,
  type AppInstallation,
  type CompareFetch,
  type OrgMembershipState,
} from "@/lib/integrations/github-app"

// resolveInstallationTokenWith is the pure resolution policy behind
// resolveRepoInstallationToken — GitHub's round-trips (per-repo installation
// lookup + token mint + repo-access probe) are injected, so it can be exercised
// without a real App JWT. These cases pin the fallback behavior that fixes the
// spurious 412 AND the verification that stops the fallback from minting
// tokens that can't reach a repo removed from the installation's selection.
describe(`resolveInstallationTokenWith`, () => {
  const repo = `Niach/exponential`
  // The REAL GitHub expiry the mint reports — must reach the resolved object
  // untouched (EXP-73: a synthetic expiry here poisoned every desktop
  // freshness check).
  const expiresAt = Date.parse(`2099-01-01T00:00:00.000Z`)
  const minted = (token: string) => ({ token, expiresAt })

  it(`mints against the live installation id and skips the verify probe on a hit`, async () => {
    const resolveId = vi.fn(async () => 42)
    const mintToken = vi.fn(async () => minted(`tok-live`))
    const verifyRepo = vi.fn(async () => true)

    const resolved = await resolveInstallationTokenWith(repo, 7, {
      resolveId,
      mintToken,
      verifyRepo,
    })

    expect(resolved).toEqual({ token: `tok-live`, installationId: 42, expiresAt })
    expect(mintToken).toHaveBeenCalledTimes(1)
    expect(mintToken).toHaveBeenCalledWith(42)
    expect(verifyRepo).not.toHaveBeenCalled()
  })

  it(`falls back to the stored installation id when the live lookup 404s and the probe passes`, async () => {
    // GitHub's per-repo endpoint reports "not installed" (null) even though the
    // App still covers the repo through the installation persisted at connect
    // time — mint against that instead of 412ing, verified by the repo probe.
    const resolveId = vi.fn(async () => null)
    const mintToken = vi.fn(async () => minted(`tok-fallback`))
    const verifyRepo = vi.fn(async () => true)

    const resolved = await resolveInstallationTokenWith(repo, 7, {
      resolveId,
      mintToken,
      verifyRepo,
    })

    expect(resolved).toEqual({
      token: `tok-fallback`,
      installationId: 7,
      expiresAt,
    })
    expect(mintToken).toHaveBeenCalledTimes(1)
    expect(mintToken).toHaveBeenCalledWith(7)
    expect(verifyRepo).toHaveBeenCalledWith(repo, `tok-fallback`)
  })

  it(`returns null when the fallback token can't reach the repo (really removed)`, async () => {
    // The repo was dropped from the installation's "Only select repositories"
    // set: the mint succeeds (the installation exists) but the token can't
    // access the repo — the old blind fallback handed that token out anyway.
    const resolveId = vi.fn(async () => null)
    const mintToken = vi.fn(async () => minted(`tok-dead`))
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
    const mintToken = vi.fn(async () => minted(`tok-fallback`))
    const verifyRepo = vi.fn(async () => {
      throw new Error(`GitHub repo probe failed (500)`)
    })

    const resolved = await resolveInstallationTokenWith(repo, 7, {
      resolveId,
      mintToken,
      verifyRepo,
    })

    expect(resolved).toEqual({
      token: `tok-fallback`,
      installationId: 7,
      expiresAt,
    })
  })

  it(`returns null when the live lookup misses and there is no fallback`, async () => {
    const resolveId = vi.fn(async () => null)
    const mintToken = vi.fn(async () => minted(`never`))
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

// EXP-363: the claim flow's control verification. GitHub's /user/installations
// lists any installation the user can reach ONE repo of, so a collaborator on
// a stranger's repo enumerated the stranger's installation and could brand it
// as their team's GitHub connection. These pin the helpers' fail-closed value
// semantics (they must NEVER throw — the callback's catch-all renders a fake
// mobile "connected" page) and the ownership/membership policy itself.
describe(`getAuthenticatedGithubLogin`, () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it(`returns the login from GET /user with the user token`, async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ login: `octocat` }),
    })
    vi.stubGlobal(`fetch`, fetchMock)

    await expect(getAuthenticatedGithubLogin(`user-tok`)).resolves.toBe(
      `octocat`
    )
    const [url, init] = fetchMock.mock.calls[0] as [
      string,
      { headers: Record<string, string> },
    ]
    expect(url).toBe(`https://api.github.com/user`)
    expect(init.headers.authorization).toBe(`Bearer user-tok`)
  })

  it(`returns null (not a throw) on an error status`, async () => {
    vi.stubGlobal(
      `fetch`,
      vi.fn().mockResolvedValue({ ok: false, status: 401, json: async () => ({}) })
    )
    await expect(getAuthenticatedGithubLogin(`bad-tok`)).resolves.toBeNull()
  })

  it(`returns null (not a throw) when fetch itself rejects`, async () => {
    vi.stubGlobal(
      `fetch`,
      vi.fn().mockRejectedValue(new Error(`network down`))
    )
    await expect(getAuthenticatedGithubLogin(`user-tok`)).resolves.toBeNull()
  })
})

describe(`getUserOrgMembershipState`, () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  function stubStatus(status: number, body: unknown = {}) {
    vi.stubGlobal(
      `fetch`,
      vi.fn().mockResolvedValue({
        ok: status >= 200 && status < 300,
        status,
        json: async () => body,
      })
    )
  }

  it(`active membership → active`, async () => {
    stubStatus(200, { state: `active` })
    await expect(getUserOrgMembershipState(`tok`, `acme`)).resolves.toBe(
      `active`
    )
  })

  it(`pending membership → not-member (an invite is not control)`, async () => {
    stubStatus(200, { state: `pending` })
    await expect(getUserOrgMembershipState(`tok`, `acme`)).resolves.toBe(
      `not-member`
    )
  })

  it(`403 → permission-missing (App lacks members-read or approval pending)`, async () => {
    stubStatus(403)
    await expect(getUserOrgMembershipState(`tok`, `acme`)).resolves.toBe(
      `permission-missing`
    )
  })

  it(`404 → not-member (outside collaborators land here)`, async () => {
    stubStatus(404)
    await expect(getUserOrgMembershipState(`tok`, `acme`)).resolves.toBe(
      `not-member`
    )
  })

  it(`5xx fails CLOSED to not-member, never throws`, async () => {
    stubStatus(500)
    await expect(getUserOrgMembershipState(`tok`, `acme`)).resolves.toBe(
      `not-member`
    )
  })

  it(`encodes the org into the membership URL`, async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ state: `active` }),
    })
    vi.stubGlobal(`fetch`, fetchMock)
    await getUserOrgMembershipState(`tok`, `acme`)
    expect(fetchMock.mock.calls[0][0]).toBe(
      `https://api.github.com/user/memberships/orgs/acme`
    )
  })
})

describe(`partitionControlledInstallations`, () => {
  const inst = (
    id: number,
    account: string,
    accountType: string
  ): AppInstallation => ({ id, account, accountType })
  const neverMembership = vi.fn(async (): Promise<OrgMembershipState> => {
    throw new Error(`membership must not be consulted for User installations`)
  })

  it(`keeps a User installation whose login matches case-insensitively`, async () => {
    const result = await partitionControlledInstallations(
      [inst(1, `NiAcH`, `User`)],
      { viewerLogin: `niach`, orgMembership: neverMembership }
    )
    expect(result.controlled.map((i) => i.id)).toEqual([1])
    expect(result.orgPermissionBlocked).toBe(false)
  })

  it(`filters a User installation on someone else's account (the EXP-363 leak)`, async () => {
    const result = await partitionControlledInstallations(
      [inst(1, `Niach`, `User`)],
      { viewerLogin: `LukeTechMech`, orgMembership: neverMembership }
    )
    expect(result.controlled).toEqual([])
  })

  it(`keeps an Organization installation only on active membership`, async () => {
    const membership = vi.fn(
      async (org: string): Promise<OrgMembershipState> =>
        org === `goodorg` ? `active` : `not-member`
    )
    const result = await partitionControlledInstallations(
      [inst(1, `goodorg`, `Organization`), inst(2, `otherorg`, `Organization`)],
      { viewerLogin: `octocat`, orgMembership: membership }
    )
    expect(result.controlled.map((i) => i.id)).toEqual([1])
    expect(result.orgPermissionBlocked).toBe(false)
    expect(membership).toHaveBeenCalledWith(`goodorg`)
    expect(membership).toHaveBeenCalledWith(`otherorg`)
  })

  it(`flags orgPermissionBlocked when the membership check lacks the App permission`, async () => {
    const result = await partitionControlledInstallations(
      [inst(1, `acme`, `Organization`)],
      {
        viewerLogin: `octocat`,
        orgMembership: async () => `permission-missing`,
      }
    )
    expect(result.controlled).toEqual([])
    expect(result.orgPermissionBlocked).toBe(true)
    // Reported as undetermined, not silently uncontrolled — callers must not
    // treat the ambiguity as proof of non-control (grant scrub).
    expect([...result.undeterminedIds]).toEqual([1])
  })

  it(`reports no undetermined ids for affirmative verdicts`, async () => {
    const result = await partitionControlledInstallations(
      [
        inst(1, `octocat`, `User`),
        inst(2, `stranger`, `User`),
        inst(3, `acme`, `Organization`),
      ],
      { viewerLogin: `octocat`, orgMembership: async () => `not-member` }
    )
    expect(result.controlled.map((i) => i.id)).toEqual([1])
    expect(result.orgPermissionBlocked).toBe(false)
    expect(result.undeterminedIds.size).toBe(0)
  })

  it(`filters empty logins, empty viewer, and unknown account types (fail closed)`, async () => {
    const membership = vi.fn(async (): Promise<OrgMembershipState> => `active`)
    const noLogin = await partitionControlledInstallations(
      [inst(1, ``, `User`), inst(2, `ent`, `Enterprise`)],
      { viewerLogin: `octocat`, orgMembership: membership }
    )
    expect(noLogin.controlled).toEqual([])
    const noViewer = await partitionControlledInstallations(
      [inst(3, `octocat`, `User`)],
      { viewerLogin: ``, orgMembership: membership }
    )
    expect(noViewer.controlled).toEqual([])
  })

  it(`preserves order across a mixed set and keeps only controlled entries`, async () => {
    const result = await partitionControlledInstallations(
      [
        inst(1, `stranger`, `User`),
        inst(2, `octocat`, `User`),
        inst(3, `acme`, `Organization`),
      ],
      { viewerLogin: `octocat`, orgMembership: async () => `active` }
    )
    expect(result.controlled.map((i) => i.id)).toEqual([2, 3])
  })
})

// The module-level TTL caches only ever grew: their read-side TTL check hides
// dead entries but never frees them, so one-shot keys (a fresh `exp/…` branch
// per coding run, each holding up to 100 files of patch text) leaked for the
// process lifetime. These pin the prune-on-insert bound.
describe(`evictStaleEntries`, () => {
  const opts = { now: 100_000, ttlMs: 60_000, maxEntries: 4 }

  function fill(entries: Array<[string, number]>) {
    return new Map(entries.map(([key, at]) => [key, { at }]))
  }

  it(`does nothing below the cap, even with past-TTL entries`, () => {
    const cache = fill([
      [`a`, 0],
      [`b`, 0],
      [`c`, 99_000],
    ])

    evictStaleEntries(cache, opts)

    expect([...cache.keys()]).toEqual([`a`, `b`, `c`])
  })

  it(`drops every past-TTL entry once the cap is reached`, () => {
    const cache = fill([
      [`stale-1`, 0],
      [`fresh-1`, 50_000],
      [`stale-2`, 39_999],
      [`fresh-2`, 99_000],
    ])

    evictStaleEntries(cache, opts)

    expect([...cache.keys()]).toEqual([`fresh-1`, `fresh-2`])
  })

  it(`drops the oldest writes when a burst of fresh keys is still over the cap`, () => {
    const cache = fill([
      [`f1`, 60_000],
      [`f2`, 70_000],
      [`f3`, 80_000],
      [`f4`, 90_000],
      [`f5`, 95_000],
    ])

    evictStaleEntries(cache, opts)

    // Trimmed to cap - 1 so the caller's own insert lands back at the cap.
    expect([...cache.keys()]).toEqual([`f3`, `f4`, `f5`])
  })
})

describe(`fetchBranchDiff cache bound`, () => {
  it(`evicts the oldest branch once the cache is full`, async () => {
    const fetchImpl = vi.fn<CompareFetch>().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ files: [] }),
    })
    const now = 1_000
    const repo = `acme/bounded`

    for (let i = 0; i <= BRANCH_DIFF_CACHE_MAX; i++) {
      await fetchBranchDiff({
        repo,
        base: `main`,
        branch: `exp/BOUND-${i}`,
        token: `t0k3n`,
        now,
        fetchImpl,
      })
    }

    // Every entry is inside the TTL, so only the hard cap can free space: the
    // first branch is gone while the newest two are still served.
    expect(peekBranchDiff(repo, `main`, `exp/BOUND-0`, now)).toBeNull()
    expect(peekBranchDiff(repo, `main`, `exp/BOUND-1`, now)).not.toBeNull()
    expect(
      peekBranchDiff(repo, `main`, `exp/BOUND-${BRANCH_DIFF_CACHE_MAX}`, now)
    ).not.toBeNull()
  })
})

// Installation-token minting: the security contract that a token minted for a
// repo is confined to EXACTLY that repo (`repositories: [<bare name>]` in the
// mint body) — repositories.installationToken hands the raw token to any
// team member, so an unscoped mint would reach every repo in the
// installation ("a collaborator on one repo must not discover/connect the rest
// of the installation"). The module reads GITHUB_APP_* at load and signs a real
// RS256 App JWT, so each case stubs the env with a throwaway RSA key and
// re-imports a fresh module instance.
describe(`installation token minting (repo scoping)`, () => {
  afterEach(() => {
    vi.unstubAllEnvs()
    vi.unstubAllGlobals()
    vi.resetModules()
  })

  async function loadWithAppEnv() {
    const { privateKey } = crypto.generateKeyPairSync(`rsa`, {
      modulusLength: 2048,
    })
    const pem = privateKey.export({ type: `pkcs1`, format: `pem` }) as string
    vi.stubEnv(`GITHUB_APP_ID`, `1234`)
    vi.stubEnv(`GITHUB_APP_PRIVATE_KEY`, Buffer.from(pem).toString(`base64`))
    vi.resetModules()
    return import(`@/lib/integrations/github-app`)
  }

  function jsonResponse(body: unknown, status = 200) {
    return { ok: status >= 200 && status < 300, status, json: async () => body }
  }

  // A fixed far-future GitHub expiry so the resolved `expiresAt` (the real
  // `expires_at`, threaded through verbatim) is deterministic to assert.
  const MINT_EXPIRES_AT = `2099-01-01T00:00:00.000Z`

  function tokenResponse(token: string) {
    return jsonResponse({ token, expires_at: MINT_EXPIRES_AT }, 201)
  }

  function mintCalls(fetchMock: ReturnType<typeof vi.fn>) {
    return fetchMock.mock.calls.filter(([url]) =>
      (url as string).includes(`access_tokens`)
    ) as Array<[string, RequestInit]>
  }

  it(`resolveRepoInstallationTokenInfo mints a token scoped to exactly the requested repo`, async () => {
    const mod = await loadWithAppEnv()
    const fetchMock = vi.fn(async (url: string) => {
      if (url.endsWith(`/repos/acme/website/installation`)) {
        return jsonResponse({ id: 42 })
      }
      if (url.endsWith(`/app/installations/42/access_tokens`)) {
        return tokenResponse(`tok-scoped`)
      }
      throw new Error(`unexpected fetch: ${url}`)
    })
    vi.stubGlobal(`fetch`, fetchMock)

    const resolved = await mod.resolveRepoInstallationTokenInfo(`acme/website`)

    expect(resolved).toEqual({
      token: `tok-scoped`,
      installationId: 42,
      expiresAt: Date.parse(MINT_EXPIRES_AT),
    })
    const [[, init]] = mintCalls(fetchMock)
    expect(init.method).toBe(`POST`)
    const headers = init.headers as Record<string, string>
    expect(headers[`content-type`]).toBe(`application/json`)
    // Bare repo name, NOT "acme/website" — GitHub's `repositories` field takes
    // bare names; the owner is implied by the installation.
    expect(JSON.parse(init.body as string)).toEqual({ repositories: [`website`] })
  })

  it(`caches per installation+repo, not per installation`, async () => {
    const mod = await loadWithAppEnv()
    const fetchMock = vi.fn(async (url: string, init?: RequestInit) => {
      if (/\/repos\/acme\/(website|api)\/installation$/.test(url)) {
        return jsonResponse({ id: 42 })
      }
      if (url.endsWith(`/app/installations/42/access_tokens`)) {
        const body = JSON.parse(init?.body as string) as {
          repositories: string[]
        }
        return tokenResponse(`tok-${body.repositories[0]}`)
      }
      throw new Error(`unexpected fetch: ${url}`)
    })
    vi.stubGlobal(`fetch`, fetchMock)

    // Two repos of the SAME installation → two distinct scoped mints; a
    // per-installation cache would hand acme/api the acme/website token.
    const website = await mod.resolveRepoInstallationTokenInfo(`acme/website`)
    const api = await mod.resolveRepoInstallationTokenInfo(`acme/api`)
    expect(website?.token).toBe(`tok-website`)
    expect(api?.token).toBe(`tok-api`)
    expect(
      mintCalls(fetchMock).map(([, init]) => JSON.parse(init.body as string))
    ).toEqual([{ repositories: [`website`] }, { repositories: [`api`] }])

    // Re-resolving a repo serves its own cache slot: the (uncached) per-repo
    // installation lookup still fires, but no third mint happens.
    const again = await mod.resolveRepoInstallationTokenInfo(`acme/website`)
    expect(again?.token).toBe(`tok-website`)
    expect(mintCalls(fetchMock)).toHaveLength(2)
  })

  it(`re-mints instead of serving a cached token inside the 10-min margin`, async () => {
    // EXP-73: the old 60s margin could serve a token with ~2 min of real life
    // to the desktop, which embeds it as ambient git credentials and schedules
    // its refresh 8 min before the reported expiry.
    const mod = await loadWithAppEnv()
    let mintCount = 0
    const fetchMock = vi.fn(async (url: string) => {
      if (url.endsWith(`/repos/acme/website/installation`)) {
        return jsonResponse({ id: 42 })
      }
      if (url.endsWith(`/app/installations/42/access_tokens`)) {
        mintCount += 1
        return jsonResponse(
          {
            token: `tok-${mintCount}`,
            expires_at: new Date(Date.now() + 5 * 60_000).toISOString(),
          },
          201
        )
      }
      throw new Error(`unexpected fetch: ${url}`)
    })
    vi.stubGlobal(`fetch`, fetchMock)

    // Both mints report only ~5 min of remaining life — under the serve
    // margin, so the second resolve re-mints rather than serving the first
    // (nearly-dead) token from cache.
    const first = await mod.resolveRepoInstallationTokenInfo(`acme/website`)
    const second = await mod.resolveRepoInstallationTokenInfo(`acme/website`)
    expect(first?.token).toBe(`tok-1`)
    expect(second?.token).toBe(`tok-2`)
  })

  it(`listInstallationRepos mints an installation-wide token (no repositories body)`, async () => {
    const mod = await loadWithAppEnv()
    const fetchMock = vi.fn(async (url: string) => {
      if (url.endsWith(`/app/installations/42/access_tokens`)) {
        return tokenResponse(`tok-wide`)
      }
      if (url.includes(`/installation/repositories`)) {
        return jsonResponse({
          total_count: 1,
          repositories: [
            { full_name: `acme/website`, private: true, default_branch: `main` },
          ],
        })
      }
      throw new Error(`unexpected fetch: ${url}`)
    })
    vi.stubGlobal(`fetch`, fetchMock)

    const result = await mod.listInstallationRepos(42)

    expect(result).toEqual({
      repos: [
        {
          fullName: `acme/website`,
          private: true,
          defaultBranch: `main`,
          installationId: 42,
        },
      ],
      hasMore: false,
    })
    // The server-internal enumeration path is the ONLY unscoped mint — the
    // token never leaves the process, and its listing call uses it.
    const [[, mintInit]] = mintCalls(fetchMock)
    expect(mintInit.body).toBeUndefined()
    const listCall = fetchMock.mock.calls.find(([url]) =>
      (url as string).includes(`/installation/repositories`)
    ) as unknown as [string, { headers: Record<string, string> }]
    expect(listCall[1].headers.authorization).toBe(`Bearer tok-wide`)
  })
})
