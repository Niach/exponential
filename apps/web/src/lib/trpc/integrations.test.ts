import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// SLOP-7: the ONE GitHub flow. The router lists installations and repos LIVE
// off the caller's GitHub token (Better Auth's `github` account row); the
// connect gate proves push access with that same token and resolves the App
// installation through the App JWT. Every GitHub round-trip is stubbed; the
// tests pin the contract every client reads: `linked`/`needsReconnect`, the
// push filter, the per-user cache, and the gate's refusals.

const h = vi.hoisted(() => ({
  tokenState: { state: `ok`, token: `user-tok` } as
    | { state: `none` }
    | { state: `dead` }
    | { state: `ok`; token: string },
  login: `octocat` as string | null,
  installations: [] as Array<{
    id: number
    account: string
    accountType: string
    suspended: boolean
  }>,
  reposByInstallation: new Map<
    number,
    Array<{
      fullName: string
      private: boolean
      defaultBranch: string
      installationId: number
    }>
  >(),
  repoAccess: null as {
    push: boolean
    private: boolean
    defaultBranch: string
  } | null,
  repoInstallationId: 7 as number | null,
  listInstallations: vi.fn(),
  listRepos: vi.fn(),
  member: vi.fn(async (..._args: unknown[]) => ({ role: `member` })),
}))

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/db/auth-schema`, () => ({ accounts: {} }))
vi.mock(`@/lib/auth/api-key-kind`, () => ({
  assertNotApiKeySession: vi.fn(async () => {}),
}))
vi.mock(`@/lib/auth/sign-in-methods`, () => ({
  assertNotLastWayIn: vi.fn(async () => {}),
}))
vi.mock(`@/lib/auth/oauth-revocation`, () => ({
  captureOAuthTokens: vi.fn(async () => []),
  revokeOAuthTokensBestEffort: vi.fn(async () => {}),
}))
vi.mock(`@/lib/team-membership`, () => ({
  assertTeamMember: (...args: unknown[]) => h.member(...args),
}))
vi.mock(`@/lib/integrations/github-user`, () => ({
  GITHUB_PROVIDER_ID: `github`,
  githubUserToken: vi.fn(async () => h.tokenState),
  githubLoginForToken: vi.fn(async () => h.login),
}))
vi.mock(`@/lib/integrations/github-app`, () => {
  class GithubUserApiError extends Error {
    constructor(
      message: string,
      public readonly status: number
    ) {
      super(message)
    }
  }
  return {
    GithubUserApiError,
    githubAppConfigured: () => true,
    githubConnectConfigured: () => true,
    githubAppInstallUrl: () => `https://github.com/apps/exp/installations/new`,
    installationManageUrl: (inst: { installationId: number }) =>
      `https://github.com/settings/installations/${inst.installationId}`,
    installationIdForRepo: vi.fn(async () => h.repoInstallationId),
    fetchUserRepoAccess: vi.fn(async () => h.repoAccess),
    listUserInstallations: (...args: unknown[]) => {
      h.listInstallations(...args)
      return Promise.resolve(h.installations)
    },
    listUserInstallationRepos: (_token: string, installationId: number) => {
      h.listRepos(installationId)
      return Promise.resolve({
        repos: h.reposByInstallation.get(installationId) ?? [],
        hasMore: false,
      })
    },
  }
})

import {
  githubConnectPageUrl,
  integrationsRouter,
  invalidateGithubDiscovery,
  resolveRepoForConnect,
} from "@/lib/trpc/integrations"

const TEAM = `00000000-0000-4000-8000-000000000001`

function callerFor(userId: string) {
  return integrationsRouter.createCaller({
    session: { user: { id: userId }, session: { id: `s1` } },
    db: {},
  } as never)
}

let userCounter = 0
function freshUser(): string {
  userCounter += 1
  return `user-${userCounter}`
}

beforeEach(() => {
  process.env.BETTER_AUTH_URL = `https://app.example.com`
  h.tokenState = { state: `ok`, token: `user-tok` }
  h.login = `octocat`
  h.installations = [
    { id: 1, account: `acme`, accountType: `Organization`, suspended: false },
  ]
  h.reposByInstallation = new Map([
    [
      1,
      [
        {
          fullName: `acme/app`,
          private: true,
          defaultBranch: `main`,
          installationId: 1,
        },
      ],
    ],
  ])
  h.repoAccess = { push: true, private: true, defaultBranch: `main` }
  h.repoInstallationId = 1
  h.listInstallations.mockClear()
  h.listRepos.mockClear()
  h.member.mockClear()
})

describe(`githubConnectPageUrl`, () => {
  it(`builds the guided page URL with the team, board and return marker`, () => {
    expect(githubConnectPageUrl({ teamId: TEAM, boardId: `b1`, returnTo: `app` })).toBe(
      `https://app.example.com/integrations/github?team=${TEAM}&board=b1&return=app`
    )
    expect(githubConnectPageUrl({})).toBe(`https://app.example.com/integrations/github`)
  })
})

describe(`integrations.github.status`, () => {
  it(`member-gates the team`, async () => {
    h.member.mockImplementationOnce(async () => {
      throw new TRPCError({ code: `FORBIDDEN` })
    })
    await expect(
      callerFor(freshUser()).github.status({ teamId: TEAM })
    ).rejects.toMatchObject({ code: `FORBIDDEN` })
  })

  it(`reports the linked connection, its login and installations`, async () => {
    const status = await callerFor(freshUser()).github.status({ teamId: TEAM })
    expect(status).toMatchObject({
      configured: true,
      linked: true,
      needsReconnect: false,
      login: `octocat`,
      installed: true,
      installUrl: `https://github.com/apps/exp/installations/new`,
      connectUrl: `https://app.example.com/integrations/github?team=${TEAM}`,
    })
    expect(status.installations).toEqual([
      {
        installationId: 1,
        accountLogin: `acme`,
        accountType: `Organization`,
        manageUrl: `https://github.com/settings/installations/1`,
        suspended: false,
        needsReauth: false,
        stale: false,
        hasMore: false,
      },
    ])
  })

  it(`marks the page URL for a native caller`, async () => {
    const status = await callerFor(freshUser()).github.status({
      teamId: TEAM,
      platform: `mobile`,
    })
    expect(status.connectUrl).toBe(
      `https://app.example.com/integrations/github?team=${TEAM}&return=app`
    )
  })

  it(`an unlinked account is not installed and lists nothing, without touching GitHub`, async () => {
    h.tokenState = { state: `none` }
    const status = await callerFor(freshUser()).github.status({ teamId: TEAM })
    expect(status).toMatchObject({
      linked: false,
      needsReconnect: false,
      installed: false,
      installations: [],
    })
    expect(h.listInstallations).not.toHaveBeenCalled()
  })

  it(`a dead token reads as linked + needsReconnect`, async () => {
    h.tokenState = { state: `dead` }
    const status = await callerFor(freshUser()).github.status({ teamId: TEAM })
    expect(status).toMatchObject({ linked: true, needsReconnect: true, installed: false })
  })
})

describe(`integrations.github.repos`, () => {
  it(`lists the push-able repos across installations, deduped and sorted`, async () => {
    h.installations = [
      { id: 1, account: `acme`, accountType: `Organization`, suspended: false },
      { id: 2, account: `octocat`, accountType: `User`, suspended: false },
    ]
    h.reposByInstallation = new Map([
      [
        1,
        [
          { fullName: `acme/zeta`, private: false, defaultBranch: `main`, installationId: 1 },
          { fullName: `acme/app`, private: true, defaultBranch: `main`, installationId: 1 },
        ],
      ],
      [
        2,
        [
          { fullName: `acme/app`, private: true, defaultBranch: `main`, installationId: 2 },
          { fullName: `octocat/site`, private: false, defaultBranch: `master`, installationId: 2 },
        ],
      ],
    ])
    const result = await callerFor(freshUser()).github.repos({ teamId: TEAM })
    expect(result.repos.map((r) => r.fullName)).toEqual([
      `acme/app`,
      `acme/zeta`,
      `octocat/site`,
    ])
    expect(result.installations.map((i) => i.installationId)).toEqual([1, 2])
  })

  it(`serves the cache within the window and bypasses it on refresh`, async () => {
    const caller = callerFor(freshUser())
    await caller.github.repos({ teamId: TEAM })
    await caller.github.repos({ teamId: TEAM })
    expect(h.listInstallations).toHaveBeenCalledTimes(1)
    expect(h.listRepos).toHaveBeenCalledTimes(1)
    await caller.github.repos({ teamId: TEAM, refresh: true })
    expect(h.listInstallations).toHaveBeenCalledTimes(2)
  })

  it(`keys the cache per user — two members never share an entry`, async () => {
    await callerFor(freshUser()).github.repos({ teamId: TEAM })
    await callerFor(freshUser()).github.repos({ teamId: TEAM })
    expect(h.listInstallations).toHaveBeenCalledTimes(2)
  })

  it(`invalidateGithubDiscovery drops the user's entry`, async () => {
    const user = freshUser()
    await callerFor(user).github.repos({ teamId: TEAM })
    invalidateGithubDiscovery(user)
    await callerFor(user).github.repos({ teamId: TEAM })
    expect(h.listInstallations).toHaveBeenCalledTimes(2)
  })

  it(`a suspended installation stays listed but contributes no repos`, async () => {
    h.installations = [
      { id: 1, account: `acme`, accountType: `Organization`, suspended: true },
    ]
    const result = await callerFor(freshUser()).github.repos({ teamId: TEAM })
    expect(result.repos).toEqual([])
    expect(result.installations[0]).toMatchObject({ installationId: 1, suspended: true })
    expect(h.listRepos).not.toHaveBeenCalled()
  })

  it(`an unlinked account gets the empty listing and the connect page`, async () => {
    h.tokenState = { state: `none` }
    const result = await callerFor(freshUser()).github.repos({ teamId: TEAM })
    expect(result).toMatchObject({ linked: false, installed: false, repos: [] })
    expect(result.connectUrl).toContain(`/integrations/github?team=`)
  })
})

describe(`resolveRepoForConnect (the connect gate)`, () => {
  it(`returns the App installation and GitHub's metadata for a push-able repo`, async () => {
    await expect(resolveRepoForConnect(freshUser(), `acme/app`)).resolves.toEqual({
      installationId: 1,
      private: true,
      defaultBranch: `main`,
    })
  })

  it(`refuses without a GitHub connection`, async () => {
    h.tokenState = { state: `none` }
    await expect(resolveRepoForConnect(freshUser(), `acme/app`)).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: expect.stringContaining(`Connect GitHub first`),
    })
  })

  it(`refuses a dead connection with the reconnect hint`, async () => {
    h.tokenState = { state: `dead` }
    await expect(resolveRepoForConnect(freshUser(), `acme/app`)).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: expect.stringContaining(`Reconnect GitHub`),
    })
  })

  it(`refuses a repo the user cannot see`, async () => {
    h.repoAccess = null
    await expect(resolveRepoForConnect(freshUser(), `acme/hidden`)).rejects.toMatchObject({
      code: `NOT_FOUND`,
    })
  })

  it(`refuses a repo without push access`, async () => {
    h.repoAccess = { push: false, private: true, defaultBranch: `main` }
    await expect(resolveRepoForConnect(freshUser(), `acme/app`)).rejects.toMatchObject({
      code: `FORBIDDEN`,
      message: expect.stringContaining(`push access`),
    })
  })

  it(`refuses a repo the App is not installed on, naming the owner`, async () => {
    h.repoInstallationId = null
    await expect(resolveRepoForConnect(freshUser(), `acme/app`)).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: expect.stringContaining(`Install it for acme`),
    })
  })
})

describe(`integrations.github.lookupRepo`, () => {
  it(`returns a picker row through the gate`, async () => {
    await expect(
      callerFor(freshUser()).github.lookupRepo({ teamId: TEAM, fullName: `acme/app` })
    ).resolves.toEqual({
      fullName: `acme/app`,
      private: true,
      defaultBranch: `main`,
      installationId: 1,
    })
  })

  it(`rejects a name that is not owner/name`, async () => {
    await expect(
      callerFor(freshUser()).github.lookupRepo({ teamId: TEAM, fullName: `nope` })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })
})
