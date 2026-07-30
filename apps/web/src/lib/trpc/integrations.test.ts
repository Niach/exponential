import { beforeEach, describe, expect, it, vi } from "vitest"

// One linked installation resolves for every team; the repos query then
// aggregates + caches per team. A queue lets individual tests override
// specific SELECT results (e.g. unlink's in-use check); everything else gets
// the default link row, which keeps the cache tests isolated to cache
// behavior (fresh serve, refresh bypass, invalidation, per-team keys).
const DEFAULT_ROWS = [
  { installationId: 1, accountLogin: `acme`, accountType: `User` },
]
const selectQueue: unknown[][] = []
function nextRows(): unknown[] {
  return selectQueue.length > 0 ? selectQueue.shift()! : DEFAULT_ROWS
}

const inserted: Record<string, unknown>[] = []
const deletes: number[] = []
// Every `db.update(...).set(...)` payload — the suspension probe's self-heal
// write is the only updater in this router.
const updates: Record<string, unknown>[] = []

vi.mock(`@/db/connection`, () => ({
  db: {
    select: () => {
      const rows = nextRows()
      const chain: {
        from: () => typeof chain
        innerJoin: () => typeof chain
        where: () => typeof chain
        limit: () => Promise<unknown[]>
        then: (
          onFulfilled: (rows: unknown[]) => unknown,
          onRejected?: (err: unknown) => unknown
        ) => Promise<unknown>
      } = {
        from: () => chain,
        innerJoin: () => chain,
        where: () => chain,
        limit: () => Promise.resolve(rows),
        then: (onFulfilled, onRejected) =>
          Promise.resolve(rows).then(onFulfilled, onRejected),
      }
      return chain
    },
    insert: () => {
      const chain = {
        values: (v: Record<string, unknown> | Record<string, unknown>[]) => {
          inserted.push(...(Array.isArray(v) ? v : [v]))
          return chain
        },
        onConflictDoNothing: () => Promise.resolve(),
      }
      return chain
    },
    update: () => ({
      set: (v: Record<string, unknown>) => ({
        where: () => {
          updates.push(v)
          return Promise.resolve()
        },
      }),
    }),
    delete: () => ({
      where: () => {
        deletes.push(1)
        return Promise.resolve()
      },
    }),
  },
}))

vi.mock(`@/lib/admin`, () => ({
  isUserAdmin: vi.fn(async () => false),
}))

const assertTeamMember = vi.fn(
  async (..._args: unknown[]) => ({ role: `owner` })
)
vi.mock(`@/lib/team-membership`, () => ({
  assertTeamMember: (...args: unknown[]) => assertTeamMember(...args),
  getUserTeamIds: vi.fn(async () => [`ws-union`]),
}))

const listAllInstallationRepos = vi.fn(async (_installationId: number) => ({
  repos: [
    {
      fullName: `acme/repo`,
      private: false,
      defaultBranch: `main`,
      installationId: 1,
    },
  ],
  hasMore: false,
}))

// OAuth-configured toggles the grant gate: false (the default here) keeps the
// legacy installation-wide behavior every pre-existing test pins; the grant
// tests flip it to true. `installationIdForRepo` is hoisted the same way so
// the fallback-scan test can make the live lookup 404 (null).
const githubOAuthConfigured = vi.fn(() => false)
const installationIdForRepo = vi.fn(async (_repo: string): Promise<number | null> => 1)

// Captures the signed state tokens passed into each minted URL so the
// platform/purpose tests can decode their markers (the setup-state module
// stays REAL — minting and consuming exercise the true HMAC path).
const installUrlStates: (string | undefined)[] = []
const connectUrlStates: (string | undefined)[] = []

vi.mock(`@/lib/integrations/github-app`, () => ({
  githubAppConfigured: () => true,
  githubOAuthConfigured: () => githubOAuthConfigured(),
  githubAppInstallUrl: (state?: string) => {
    installUrlStates.push(state)
    return `https://install.example`
  },
  githubOAuthAuthorizeUrl: (state?: string) => {
    connectUrlStates.push(state)
    return state ? `https://oauth.example` : null
  },
  installationIdForRepo: (...args: unknown[]) =>
    installationIdForRepo(...(args as [string])),
  installationManageUrl: (inst: { installationId: number }) =>
    `https://manage.example/${inst.installationId}`,
  listAllInstallationRepos: (...args: unknown[]) =>
    listAllInstallationRepos(...(args as [number])),
}))

import {
  assertRepoInstallationAccess,
  integrationsRouter,
  invalidateRepoCache,
} from "@/lib/trpc/integrations"
import {
  consumeGithubSetupState,
  githubSetupStateWantsDialog,
  githubSetupStateWantsMobile,
  mintGithubClaimTicket,
} from "@/lib/integrations/github-setup-state"

function callerFor(userId: string) {
  return integrationsRouter.createCaller({
    session: { user: { id: userId } },
  } as never)
}

let wsCounter = 0
function freshTeamId(): string {
  wsCounter += 1
  return `00000000-0000-4000-8000-${String(wsCounter).padStart(12, `0`)}`
}

beforeEach(() => {
  process.env.BETTER_AUTH_SECRET = `test-secret-test-secret-test-secret!`
  listAllInstallationRepos.mockClear()
  assertTeamMember.mockClear()
  githubOAuthConfigured.mockReturnValue(false)
  installationIdForRepo.mockClear()
  selectQueue.length = 0
  inserted.length = 0
  deletes.length = 0
  updates.length = 0
  installUrlStates.length = 0
  connectUrlStates.length = 0
})

describe(`integrations.github.repos scoping`, () => {
  it(`member-gates the team-scoped path`, async () => {
    const teamId = freshTeamId()
    await callerFor(`user-a`).github.repos({ teamId })
    expect(assertTeamMember).toHaveBeenCalledWith(`user-a`, teamId)
  })

  it(`rejects a call without a teamId (the shim is gone)`, async () => {
    await expect(
      // @ts-expect-error — the input now requires teamId; simulate an
      // outdated client sending none.
      callerFor(`user-legacy`).github.repos()
    ).rejects.toThrow()
    expect(assertTeamMember).not.toHaveBeenCalled()
  })

  it(`mints a team-bound connect URL`, async () => {
    const teamId = freshTeamId()
    const result = await callerFor(`user-a`).github.repos({ teamId })
    expect(result.connectUrl).toBe(`https://oauth.example`)
    // The state inside carries the OAuth purpose + the target team.
    const state = connectUrlStates.at(-1) ?? null
    expect(
      consumeGithubSetupState(state, `user-a`, { expectOauth: true })
    ).toEqual({ userId: `user-a`, teamId })
  })
})

describe(`integrations.github.repos cache`, () => {
  it(`serves a fresh cache entry without re-hitting GitHub (keyed per team)`, async () => {
    const caller = callerFor(`user-fresh`)
    const wsA = freshTeamId()
    const wsB = freshTeamId()

    await caller.github.repos({ teamId: wsA })
    expect(listAllInstallationRepos).toHaveBeenCalledTimes(1)

    // Second call within the TTL is served from cache — GitHub not re-hit.
    await caller.github.repos({ teamId: wsA })
    expect(listAllInstallationRepos).toHaveBeenCalledTimes(1)

    // A different team never shares the entry.
    await caller.github.repos({ teamId: wsB })
    expect(listAllInstallationRepos).toHaveBeenCalledTimes(2)
  })

  it(`bypasses the cache when refresh is set`, async () => {
    const caller = callerFor(`user-refresh`)
    const teamId = freshTeamId()
    await caller.github.repos({ teamId })
    expect(listAllInstallationRepos).toHaveBeenCalledTimes(1)

    // refresh: true drops the cached entry before fetching → re-hits GitHub.
    await caller.github.repos({ teamId, refresh: true })
    expect(listAllInstallationRepos).toHaveBeenCalledTimes(2)
  })

  it(`re-hits GitHub after invalidateRepoCache (the claim/setup path)`, async () => {
    const caller = callerFor(`user-setup`)
    const teamId = freshTeamId()
    await caller.github.repos({ teamId })
    expect(listAllInstallationRepos).toHaveBeenCalledTimes(1)

    // The claim callback / setup route invalidates the team's entry when
    // a link lands.
    invalidateRepoCache(teamId)

    await caller.github.repos({ teamId })
    expect(listAllInstallationRepos).toHaveBeenCalledTimes(2)
  })
})

describe(`integrations.github.repos install URL platform marker`, () => {
  it(`marks the minted state mobile only when platform is "mobile"`, async () => {
    const caller = callerFor(`user-platform`)
    const teamId = freshTeamId()

    await caller.github.repos({ teamId, platform: `mobile` })
    const mobileState = installUrlStates.at(-1) ?? null
    expect(githubSetupStateWantsMobile(mobileState)).toBe(true)
    // The mobile marker rides alongside the dialog flag, never instead of it.
    expect(githubSetupStateWantsDialog(mobileState)).toBe(true)

    // Cached serve (same team, within TTL) still mints per-request, so a
    // web call right after a mobile one must NOT inherit the mobile marker.
    await caller.github.repos({ teamId })
    const webState = installUrlStates.at(-1) ?? null
    expect(githubSetupStateWantsMobile(webState)).toBe(false)
    expect(githubSetupStateWantsDialog(webState)).toBe(true)

    await caller.github.repos({ teamId, platform: `web` })
    expect(githubSetupStateWantsMobile(installUrlStates.at(-1) ?? null)).toBe(
      false
    )
  })
})

describe(`integrations.github.status install URL platform marker (EXP-368)`, () => {
  it(`marks the minted state mobile only when platform is "mobile"`, async () => {
    const caller = callerFor(`user-status-platform`)
    const teamId = freshTeamId()

    await caller.github.status({ teamId, platform: `mobile` })
    const mobileInstall = installUrlStates.at(-1) ?? null
    const mobileConnect = connectUrlStates.at(-1) ?? null
    expect(githubSetupStateWantsMobile(mobileInstall)).toBe(true)
    expect(githubSetupStateWantsMobile(mobileConnect)).toBe(true)
    expect(githubSetupStateWantsDialog(mobileInstall)).toBe(true)

    await caller.github.status({ teamId })
    expect(githubSetupStateWantsMobile(installUrlStates.at(-1) ?? null)).toBe(
      false
    )

    await caller.github.status({ teamId, platform: `web` })
    expect(githubSetupStateWantsMobile(installUrlStates.at(-1) ?? null)).toBe(
      false
    )
  })
})

describe(`assertRepoInstallationAccess grant gate`, () => {
  // Select order on the live-resolution path: #1 the team's linked
  // installations, #2 the grant lookup for (team, installation, repo).
  it(`denies a linked-installation repo with NO grant when OAuth is configured`, async () => {
    githubOAuthConfigured.mockReturnValue(true)
    selectQueue.push(DEFAULT_ROWS) // linked installations
    selectQueue.push([]) // grant lookup → none
    await expect(
      assertRepoInstallationAccess(freshTeamId(), `acme/other-private`)
    ).rejects.toThrow(/reconnect GitHub in team settings/)
  })

  it(`allows a granted repo and returns the authoritative installation id`, async () => {
    githubOAuthConfigured.mockReturnValue(true)
    selectQueue.push(DEFAULT_ROWS)
    selectQueue.push([{ id: `grant-1` }])
    await expect(
      assertRepoInstallationAccess(freshTeamId(), `acme/repo`)
    ).resolves.toBe(1)
  })

  it(`gates the fallback scan too (live per-repo lookup 404s)`, async () => {
    githubOAuthConfigured.mockReturnValue(true)
    installationIdForRepo.mockResolvedValueOnce(null)
    selectQueue.push(DEFAULT_ROWS) // linked installations
    selectQueue.push([]) // grant lookup after the scan hit → none
    await expect(
      assertRepoInstallationAccess(freshTeamId(), `acme/repo`)
    ).rejects.toThrow(/reconnect GitHub in team settings/)
    // The scan itself ran (installation-wide listing) — the DENY came from the
    // missing grant, not from the repo being absent.
    expect(listAllInstallationRepos).toHaveBeenCalledTimes(1)
  })

  it(`bypasses the grant gate when OAuth is NOT configured (trusted self-hosted fallback)`, async () => {
    githubOAuthConfigured.mockReturnValue(false)
    selectQueue.push(DEFAULT_ROWS)
    // Poison the next select: if the gate wrongly ran, it would see no grant
    // row and throw.
    selectQueue.push([])
    await expect(
      assertRepoInstallationAccess(freshTeamId(), `acme/repo`)
    ).resolves.toBe(1)
  })
})

describe(`integrations.github.repos grant scoping (OAuth configured)`, () => {
  it(`lists only granted repos — never GitHub's installation-wide listing`, async () => {
    githubOAuthConfigured.mockReturnValue(true)
    const teamId = freshTeamId()
    selectQueue.push(DEFAULT_ROWS) // linked installations
    selectQueue.push([
      // Two grant rows for the same repo (two members proved access) — the
      // picker output dedups by fullName.
      {
        installationId: 1,
        fullName: `acme/granted`,
        private: true,
        defaultBranch: `dev`,
      },
      {
        installationId: 1,
        fullName: `acme/granted`,
        private: true,
        defaultBranch: `dev`,
      },
    ])
    const result = await callerFor(`user-grant`).github.repos({ teamId })
    expect(result.repos).toEqual([
      {
        fullName: `acme/granted`,
        private: true,
        defaultBranch: `dev`,
        installationId: 1,
      },
    ])
    expect(result.hasMore).toBe(false)
    expect(result.installations[0]).toMatchObject({
      installationId: 1,
      needsReauth: false,
    })
    // The whole point: the installation-token WHOLE-selection listing (which
    // leaks every repo of the account) is never consulted on this path.
    expect(listAllInstallationRepos).not.toHaveBeenCalled()
  })

  it(`returns an empty list + needsReauth for a linked installation with zero grants`, async () => {
    githubOAuthConfigured.mockReturnValue(true)
    const teamId = freshTeamId()
    selectQueue.push(DEFAULT_ROWS) // linked installations
    selectQueue.push([]) // no grants (e.g. a pre-grant legacy link)
    const result = await callerFor(`user-grant`).github.repos({ teamId })
    expect(result.repos).toEqual([])
    expect(result.installations[0]).toMatchObject({ needsReauth: true })
    expect(listAllInstallationRepos).not.toHaveBeenCalled()

    // Cached serve preserves the grant-derived list and the needsReauth flag.
    const cached = await callerFor(`user-grant`).github.repos({ teamId })
    expect(cached.repos).toEqual([])
    expect(cached.installations[0]).toMatchObject({ needsReauth: true })
  })
})

describe(`integrations.github.claimLinks guards`, () => {
  it(`refuses installation ids outside the ticket's verified set`, async () => {
    const teamId = freshTeamId()
    const ticket = mintGithubClaimTicket({
      u: `user-claim`,
      w: teamId,
      ids: [1, 2],
    })!
    await expect(
      callerFor(`user-claim`).github.claimLinks({
        ticket,
        installationIds: [999],
      })
    ).rejects.toThrow(/didn't verify/)
  })

  it(`refuses a ticket minted for another user`, async () => {
    const ticket = mintGithubClaimTicket({
      u: `victim`,
      w: freshTeamId(),
      ids: [1],
    })!
    await expect(
      callerFor(`attacker`).github.claimLinks({ ticket, installationIds: [1] })
    ).rejects.toThrow(/expired or belongs to another session/)
  })
})

describe(`integrations.github.unlink`, () => {
  it(`CONFLICTs while connected repos still use the installation`, async () => {
    // First SELECT in unlink = the in-use repos check.
    selectQueue.push([{ id: `repo-1` }, { id: `repo-2` }])
    await expect(
      callerFor(`user-unlink`).github.unlink({
        teamId: freshTeamId(),
        installationId: 1,
      })
    ).rejects.toThrow(/2 connected repositories use this GitHub account/)
    expect(deletes).toHaveLength(0)
  })

  it(`deletes the link once no repos use the installation`, async () => {
    selectQueue.push([]) // in-use check → none
    selectQueue.push([{ id: `gi-row-uuid` }]) // installation row lookup
    const result = await callerFor(`user-unlink`).github.unlink({
      teamId: freshTeamId(),
      installationId: 1,
    })
    expect(result).toEqual({ ok: true })
    expect(deletes).toHaveLength(1)
  })

})

// REV2-29: a GitHub suspension no longer destroys the team's claim link — the
// `suspend` webhook marks `github_installations.suspended_at` instead. The
// router turns that mark into an INERT-but-recoverable installation: it lists
// no connectable repos, refuses connects with an unsuspend-shaped message, and
// stays visible so the settings UI can explain itself. A missed `unsuspend`
// delivery self-heals — any read probes GitHub (minting an installation token
// is the definitive suspension test) and clears the mark on success.
describe(`suspended installations (REV2-29)`, () => {
  function suspendedRow(installationId: number) {
    return {
      installationId,
      accountLogin: `acme`,
      accountType: `User`,
      suspendedAt: new Date(`2026-07-20T00:00:00Z`),
    }
  }

  it(`lists no repos and reports suspended when the probe still fails`, async () => {
    const teamId = freshTeamId()
    selectQueue.push([suspendedRow(900)])
    listAllInstallationRepos.mockRejectedValueOnce(
      new Error(`This installation has been suspended`)
    )

    const result = await callerFor(`user-susp`).github.repos({ teamId })

    expect(result.installed).toBe(true)
    expect(result.repos).toEqual([])
    expect(result.installations[0]).toMatchObject({
      installationId: 900,
      suspended: true,
      // An unsuspend is the fix — never nudge the user into a re-auth.
      needsReauth: false,
    })
    // Only the probe hit GitHub; the guaranteed-failing repo listing is skipped.
    expect(listAllInstallationRepos).toHaveBeenCalledTimes(1)
    expect(updates).toHaveLength(0)
  })

  it(`self-heals the mark when the probe succeeds (missed unsuspend webhook)`, async () => {
    const teamId = freshTeamId()
    selectQueue.push([suspendedRow(901)])

    // The default listAllInstallationRepos mock resolves → GitHub says the
    // installation is fine → the mark is cleared and the repos list normally.
    const result = await callerFor(`user-heal`).github.repos({ teamId })

    expect(updates).toEqual([{ suspendedAt: null }])
    expect(result.installations[0]).toMatchObject({
      installationId: 901,
      suspended: false,
    })
    expect(result.repos.map((r) => r.fullName)).toEqual([`acme/repo`])
  })

  it(`surfaces the suspension on github.status (the settings section's source)`, async () => {
    const teamId = freshTeamId()
    selectQueue.push([suspendedRow(902)])
    listAllInstallationRepos.mockRejectedValueOnce(new Error(`suspended`))

    const result = await callerFor(`user-status`).github.status({ teamId })

    // Still "installed" — the claim survives a suspension — but flagged, so the
    // UI can't render repos as healthy while every token mint fails.
    expect(result.installed).toBe(true)
    expect(result.installations[0]).toMatchObject({
      installationId: 902,
      suspended: true,
    })
  })

  it(`refuses a connect through a suspended installation with an unsuspend message`, async () => {
    selectQueue.push([suspendedRow(903)])
    listAllInstallationRepos.mockRejectedValueOnce(new Error(`suspended`))

    await expect(
      assertRepoInstallationAccess(freshTeamId(), `acme/repo`)
    ).rejects.toThrow(/GitHub suspended the Exponential app for acme/)
    // Refused before any per-repo resolution — nothing to resolve through.
    expect(installationIdForRepo).not.toHaveBeenCalled()
  })

  it(`names the suspended owner when another installation is still healthy`, async () => {
    selectQueue.push([
      { installationId: 1, accountLogin: `other`, accountType: `User`, suspendedAt: null },
      suspendedRow(904),
    ])
    listAllInstallationRepos.mockRejectedValueOnce(new Error(`suspended`))
    installationIdForRepo.mockResolvedValueOnce(904)

    await expect(
      assertRepoInstallationAccess(freshTeamId(), `acme/repo`)
    ).rejects.toThrow(/suspended the Exponential app for acme, which owns acme\/repo/)
  })
})
