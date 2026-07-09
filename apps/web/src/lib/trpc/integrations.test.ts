import { beforeEach, describe, expect, it, vi } from "vitest"

// One linked installation resolves for every workspace; the repos query then
// aggregates + caches per workspace. A queue lets individual tests override
// specific SELECT results (e.g. unlink's in-use check); everything else gets
// the default link row, which keeps the cache tests isolated to cache
// behavior (fresh serve, refresh bypass, invalidation, per-workspace keys).
const DEFAULT_ROWS = [
  { installationId: 1, accountLogin: `acme`, accountType: `User` },
]
const selectQueue: unknown[][] = []
function nextRows(): unknown[] {
  return selectQueue.length > 0 ? selectQueue.shift()! : DEFAULT_ROWS
}

const inserted: Record<string, unknown>[] = []
const deletes: number[] = []

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

const assertWorkspaceMember = vi.fn(
  async (..._args: unknown[]) => ({ role: `owner` })
)
vi.mock(`@/lib/workspace-membership`, () => ({
  assertWorkspaceMember: (...args: unknown[]) => assertWorkspaceMember(...args),
  getUserWorkspaceIds: vi.fn(async () => [`ws-union`]),
}))

// Stable feedback-workspace id for the unlink protection guard. A fixed value
// (never produced by freshWorkspaceId) keeps the non-feedback unlink tests on
// their normal path and lets one test assert the protected refusal.
const FEEDBACK_WS_ID = `11111111-1111-4111-8111-111111111111`
vi.mock(`@/lib/bootstrap-cloud`, () => ({
  getFeedbackWorkspaceId: vi.fn(async () => FEEDBACK_WS_ID),
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
function freshWorkspaceId(): string {
  wsCounter += 1
  return `00000000-0000-4000-8000-${String(wsCounter).padStart(12, `0`)}`
}

beforeEach(() => {
  process.env.BETTER_AUTH_SECRET = `test-secret-test-secret-test-secret!`
  listAllInstallationRepos.mockClear()
  assertWorkspaceMember.mockClear()
  githubOAuthConfigured.mockReturnValue(false)
  installationIdForRepo.mockClear()
  selectQueue.length = 0
  inserted.length = 0
  deletes.length = 0
  installUrlStates.length = 0
  connectUrlStates.length = 0
})

describe(`integrations.github.repos scoping`, () => {
  it(`member-gates the workspace-scoped path`, async () => {
    const workspaceId = freshWorkspaceId()
    await callerFor(`user-a`).github.repos({ workspaceId })
    expect(assertWorkspaceMember).toHaveBeenCalledWith(`user-a`, workspaceId)
  })

  it(`rejects a call without a workspaceId (the shim is gone)`, async () => {
    await expect(
      // @ts-expect-error — the input now requires workspaceId; simulate an
      // outdated client sending none.
      callerFor(`user-legacy`).github.repos()
    ).rejects.toThrow()
    expect(assertWorkspaceMember).not.toHaveBeenCalled()
  })

  it(`mints a workspace-bound connect URL`, async () => {
    const workspaceId = freshWorkspaceId()
    const result = await callerFor(`user-a`).github.repos({ workspaceId })
    expect(result.connectUrl).toBe(`https://oauth.example`)
    // The state inside carries the OAuth purpose + the target workspace.
    const state = connectUrlStates.at(-1) ?? null
    expect(
      consumeGithubSetupState(state, `user-a`, { expectOauth: true })
    ).toEqual({ userId: `user-a`, workspaceId })
  })
})

describe(`integrations.github.repos cache`, () => {
  it(`serves a fresh cache entry without re-hitting GitHub (keyed per workspace)`, async () => {
    const caller = callerFor(`user-fresh`)
    const wsA = freshWorkspaceId()
    const wsB = freshWorkspaceId()

    await caller.github.repos({ workspaceId: wsA })
    expect(listAllInstallationRepos).toHaveBeenCalledTimes(1)

    // Second call within the TTL is served from cache — GitHub not re-hit.
    await caller.github.repos({ workspaceId: wsA })
    expect(listAllInstallationRepos).toHaveBeenCalledTimes(1)

    // A different workspace never shares the entry.
    await caller.github.repos({ workspaceId: wsB })
    expect(listAllInstallationRepos).toHaveBeenCalledTimes(2)
  })

  it(`bypasses the cache when refresh is set`, async () => {
    const caller = callerFor(`user-refresh`)
    const workspaceId = freshWorkspaceId()
    await caller.github.repos({ workspaceId })
    expect(listAllInstallationRepos).toHaveBeenCalledTimes(1)

    // refresh: true drops the cached entry before fetching → re-hits GitHub.
    await caller.github.repos({ workspaceId, refresh: true })
    expect(listAllInstallationRepos).toHaveBeenCalledTimes(2)
  })

  it(`re-hits GitHub after invalidateRepoCache (the claim/setup path)`, async () => {
    const caller = callerFor(`user-setup`)
    const workspaceId = freshWorkspaceId()
    await caller.github.repos({ workspaceId })
    expect(listAllInstallationRepos).toHaveBeenCalledTimes(1)

    // The claim callback / setup route invalidates the workspace's entry when
    // a link lands.
    invalidateRepoCache(workspaceId)

    await caller.github.repos({ workspaceId })
    expect(listAllInstallationRepos).toHaveBeenCalledTimes(2)
  })
})

describe(`integrations.github.repos install URL platform marker`, () => {
  it(`marks the minted state mobile only when platform is "mobile"`, async () => {
    const caller = callerFor(`user-platform`)
    const workspaceId = freshWorkspaceId()

    await caller.github.repos({ workspaceId, platform: `mobile` })
    const mobileState = installUrlStates.at(-1) ?? null
    expect(githubSetupStateWantsMobile(mobileState)).toBe(true)
    // The mobile marker rides alongside the dialog flag, never instead of it.
    expect(githubSetupStateWantsDialog(mobileState)).toBe(true)

    // Cached serve (same workspace, within TTL) still mints per-request, so a
    // web call right after a mobile one must NOT inherit the mobile marker.
    await caller.github.repos({ workspaceId })
    const webState = installUrlStates.at(-1) ?? null
    expect(githubSetupStateWantsMobile(webState)).toBe(false)
    expect(githubSetupStateWantsDialog(webState)).toBe(true)

    await caller.github.repos({ workspaceId, platform: `web` })
    expect(githubSetupStateWantsMobile(installUrlStates.at(-1) ?? null)).toBe(
      false
    )
  })
})

describe(`assertRepoInstallationAccess grant gate`, () => {
  // Select order on the live-resolution path: #1 the workspace's linked
  // installations, #2 the grant lookup for (workspace, installation, repo).
  it(`denies a linked-installation repo with NO grant when OAuth is configured`, async () => {
    githubOAuthConfigured.mockReturnValue(true)
    selectQueue.push(DEFAULT_ROWS) // linked installations
    selectQueue.push([]) // grant lookup → none
    await expect(
      assertRepoInstallationAccess(freshWorkspaceId(), `acme/other-private`)
    ).rejects.toThrow(/reconnect GitHub in workspace settings/)
  })

  it(`allows a granted repo and returns the authoritative installation id`, async () => {
    githubOAuthConfigured.mockReturnValue(true)
    selectQueue.push(DEFAULT_ROWS)
    selectQueue.push([{ id: `grant-1` }])
    await expect(
      assertRepoInstallationAccess(freshWorkspaceId(), `acme/repo`)
    ).resolves.toBe(1)
  })

  it(`gates the fallback scan too (live per-repo lookup 404s)`, async () => {
    githubOAuthConfigured.mockReturnValue(true)
    installationIdForRepo.mockResolvedValueOnce(null)
    selectQueue.push(DEFAULT_ROWS) // linked installations
    selectQueue.push([]) // grant lookup after the scan hit → none
    await expect(
      assertRepoInstallationAccess(freshWorkspaceId(), `acme/repo`)
    ).rejects.toThrow(/reconnect GitHub in workspace settings/)
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
      assertRepoInstallationAccess(freshWorkspaceId(), `acme/repo`)
    ).resolves.toBe(1)
  })
})

describe(`integrations.github.repos grant scoping (OAuth configured)`, () => {
  it(`lists only granted repos — never GitHub's installation-wide listing`, async () => {
    githubOAuthConfigured.mockReturnValue(true)
    const workspaceId = freshWorkspaceId()
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
    const result = await callerFor(`user-grant`).github.repos({ workspaceId })
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
    const workspaceId = freshWorkspaceId()
    selectQueue.push(DEFAULT_ROWS) // linked installations
    selectQueue.push([]) // no grants (e.g. a pre-grant legacy link)
    const result = await callerFor(`user-grant`).github.repos({ workspaceId })
    expect(result.repos).toEqual([])
    expect(result.installations[0]).toMatchObject({ needsReauth: true })
    expect(listAllInstallationRepos).not.toHaveBeenCalled()

    // Cached serve preserves the grant-derived list and the needsReauth flag.
    const cached = await callerFor(`user-grant`).github.repos({ workspaceId })
    expect(cached.repos).toEqual([])
    expect(cached.installations[0]).toMatchObject({ needsReauth: true })
  })
})

describe(`integrations.github.claimLinks guards`, () => {
  it(`refuses installation ids outside the ticket's verified set`, async () => {
    const workspaceId = freshWorkspaceId()
    const ticket = mintGithubClaimTicket({
      u: `user-claim`,
      w: workspaceId,
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
      w: freshWorkspaceId(),
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
        workspaceId: freshWorkspaceId(),
        installationId: 1,
      })
    ).rejects.toThrow(/2 connected repositories use this GitHub account/)
    expect(deletes).toHaveLength(0)
  })

  it(`deletes the link once no repos use the installation`, async () => {
    selectQueue.push([]) // in-use check → none
    selectQueue.push([{ id: `gi-row-uuid` }]) // installation row lookup
    const result = await callerFor(`user-unlink`).github.unlink({
      workspaceId: freshWorkspaceId(),
      installationId: 1,
    })
    expect(result).toEqual({ ok: true })
    expect(deletes).toHaveLength(1)
  })

  it(`refuses to unlink the protected dogfood feedback workspace`, async () => {
    await expect(
      callerFor(`user-unlink`).github.unlink({
        workspaceId: FEEDBACK_WS_ID,
        installationId: 1,
      })
    ).rejects.toThrow(/dogfood GitHub connection is protected/)
    expect(deletes).toHaveLength(0)
  })
})
