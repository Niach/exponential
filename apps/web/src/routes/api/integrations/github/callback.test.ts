import { beforeEach, describe, expect, it, vi } from "vitest"
import {
  mintGithubSetupState,
  readGithubClaimTicket,
} from "@/lib/integrations/github-setup-state"
import type {
  AppInstallation,
  OrgMembershipState,
} from "@/lib/integrations/github-app"

// --- Mocked module boundaries ------------------------------------------------
// EXP-363: the OAuth callback must link ONLY installations the OAuth user
// CONTROLS (owns the User-type account / is an active org member) — GitHub's
// /user/installations enumeration also lists installations the user merely
// collaborates on, and linking one of those branded a stranger's GitHub
// account as the team's connection. The db mock records inserts/deletes per
// table so each case can assert exactly which rows the route creates.
const insertedRows: Array<{ table: string; values: Record<string, unknown> }> =
  []
const deletedTables: string[] = []

function tableName(table: unknown): string {
  return (table as { __name?: string }).__name ?? `unknown`
}

function mockDb() {
  const insert = (table: unknown) => {
    const name = tableName(table)
    const chain: Record<string, unknown> = {}
    const record = (values: Record<string, unknown>) => {
      insertedRows.push({ table: name, values })
      return chain
    }
    chain.values = record
    chain.onConflictDoUpdate = () => chain
    chain.onConflictDoNothing = () => chain
    chain.returning = async () => {
      const last = insertedRows[insertedRows.length - 1]
      return [{ id: `row-${String(last?.values.installationId)}` }]
    }
    chain.then = (resolve: (v: unknown) => unknown) => resolve(undefined)
    return chain
  }
  const del = (table: unknown) => {
    deletedTables.push(tableName(table))
    return {
      where: async () => undefined,
    }
  }
  const dbLike = {
    insert,
    delete: del,
    transaction: async <T>(fn: (tx: unknown) => Promise<T>) => fn(dbLike),
  }
  return dbLike
}

vi.mock(`@/db/connection`, () => ({ db: mockDb() }))

vi.mock(`@/db/schema`, () => ({
  githubInstallations: { __name: `github_installations` },
  githubInstallationLinks: { __name: `github_installation_links` },
  githubInstallationRepoGrants: { __name: `github_installation_repo_grants` },
}))

const resolveSessionUserId = vi.fn(async () => `user-1` as string | null)
vi.mock(`@/lib/auth/resolve-bearer`, () => ({
  resolveSessionUserId: () => resolveSessionUserId(),
}))

const assertCanManageRepos = vi.fn(async () => {})
vi.mock(`@/lib/trpc/integrations`, () => ({
  assertCanManageRepos: () => assertCanManageRepos(),
  invalidateRepoCache: () => {},
}))

const listUserInstallations = vi.fn(async (): Promise<AppInstallation[]> => [])
const listUserInstallationRepos = vi.fn(
  async (_token: string, installationId: number) => ({
    repos: [
      {
        fullName: `acme/repo`,
        private: true,
        defaultBranch: `main`,
        installationId,
      },
    ],
    hasMore: false,
  })
)
const getAuthenticatedGithubLogin = vi.fn(
  async (): Promise<string | null> => `octocat`
)
const getUserOrgMembershipState = vi.fn(
  async (_token: string, _org: string): Promise<OrgMembershipState> =>
    `not-member`
)

vi.mock(`@/lib/integrations/github-app`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/integrations/github-app")>()),
  exchangeGithubOAuthCode: async () => `user-tok`,
  listUserInstallations: () => listUserInstallations(),
  listUserInstallationRepos: (token: string, id: number) =>
    listUserInstallationRepos(token, id),
  getAuthenticatedGithubLogin: () => getAuthenticatedGithubLogin(),
  getUserOrgMembershipState: (_token: string, org: string) =>
    getUserOrgMembershipState(_token, org),
  githubAppInstallUrl: (state?: string) =>
    `https://github.com/apps/test-app/installations/new?state=${state ?? ``}`,
}))

// Imported AFTER the mocks are registered.
const { handleCallback } = await import("./callback")

function callbackRequest(state: string): Request {
  const url = new URL(`https://app.example/api/integrations/github/callback`)
  url.searchParams.set(`code`, `oauth-code`)
  url.searchParams.set(`state`, state)
  return new Request(url.toString())
}

function oauthState(userId = `user-1`, teamId = `team-1`): string {
  return mintGithubSetupState(userId, { teamId, oauth: true })!
}

function linkInserts() {
  return insertedRows.filter((r) => r.table === `github_installation_links`)
}

function grantInserts() {
  return insertedRows.filter(
    (r) => r.table === `github_installation_repo_grants`
  )
}

function mirrorInserts() {
  return insertedRows.filter((r) => r.table === `github_installations`)
}

const userInst = (id: number, account: string): AppInstallation => ({
  id,
  account,
  accountType: `User`,
})
const orgInst = (id: number, account: string): AppInstallation => ({
  id,
  account,
  accountType: `Organization`,
})

describe(`github OAuth callback — control-verified claiming (EXP-363)`, () => {
  beforeEach(() => {
    process.env.BETTER_AUTH_SECRET = `test-secret-test-secret-test-secret!`
    insertedRows.length = 0
    deletedTables.length = 0
    resolveSessionUserId.mockResolvedValue(`user-1`)
    assertCanManageRepos.mockResolvedValue(undefined)
    getAuthenticatedGithubLogin.mockResolvedValue(`octocat`)
    getUserOrgMembershipState.mockResolvedValue(`not-member`)
    listUserInstallations.mockResolvedValue([])
    listUserInstallationRepos.mockClear()
  })

  it(`links the user's OWN installation (case-insensitive login match) and captures its grants`, async () => {
    listUserInstallations.mockResolvedValue([userInst(11, `OctoCat`)])

    const res = await handleCallback(callbackRequest(oauthState()))

    expect(res.status).toBe(302)
    expect(res.headers.get(`location`)).toBe(`/`)
    expect(linkInserts()).toHaveLength(1)
    expect(linkInserts()[0].values.githubInstallationId).toBe(`row-11`)
    expect(grantInserts()).toHaveLength(1)
    expect(listUserInstallationRepos).toHaveBeenCalledWith(`user-tok`, 11)
  })

  it(`refuses a collaborator-only enumeration: no link, no mirror, no grants — redirects to notowner with the install link`, async () => {
    // The exact EXP-363 leak: the OAuth user can reach one repo of a
    // stranger's installation, so GitHub enumerates it — but it is NOT theirs.
    listUserInstallations.mockResolvedValue([userInst(11, `Niach`)])
    getAuthenticatedGithubLogin.mockResolvedValue(`LukeTechMech`)

    const res = await handleCallback(callbackRequest(oauthState()))

    expect(linkInserts()).toHaveLength(0)
    expect(mirrorInserts()).toHaveLength(0)
    expect(grantInserts()).toHaveLength(0)
    expect(listUserInstallationRepos).not.toHaveBeenCalled()
    // The stale-grant scrub still ran for the uncontrolled installation.
    expect(deletedTables).toContain(`github_installation_repo_grants`)
    expect(res.status).toBe(302)
    const location = res.headers.get(`location`)!
    expect(location).toContain(`/integrations/github/claim?error=notowner`)
    expect(location).toContain(`login=LukeTechMech`)
    expect(location).toContain(
      encodeURIComponent(`https://github.com/apps/test-app/installations/new`)
    )
  })

  it(`links an Organization installation when the user's membership is active`, async () => {
    listUserInstallations.mockResolvedValue([orgInst(21, `acme`)])
    getUserOrgMembershipState.mockResolvedValue(`active`)

    const res = await handleCallback(callbackRequest(oauthState()))

    expect(res.headers.get(`location`)).toBe(`/`)
    expect(linkInserts()).toHaveLength(1)
    expect(getUserOrgMembershipState).toHaveBeenCalledWith(`user-tok`, `acme`)
  })

  it(`orgperm when the only enumeration is an org blocked on the members-read permission`, async () => {
    listUserInstallations.mockResolvedValue([orgInst(21, `acme`)])
    getUserOrgMembershipState.mockResolvedValue(`permission-missing`)

    const res = await handleCallback(callbackRequest(oauthState()))

    expect(linkInserts()).toHaveLength(0)
    expect(res.headers.get(`location`)).toContain(`error=orgperm`)
    // Undetermined ≠ uncontrolled: an unverifiable membership must not scrub
    // the user's existing grants for that installation.
    expect(deletedTables).not.toContain(`github_installation_repo_grants`)
  })

  it(`notowner (not orgperm) when the org lookup is a clean not-member`, async () => {
    listUserInstallations.mockResolvedValue([orgInst(21, `acme`)])
    getUserOrgMembershipState.mockResolvedValue(`not-member`)

    const res = await handleCallback(callbackRequest(oauthState()))

    expect(linkInserts()).toHaveLength(0)
    expect(res.headers.get(`location`)).toContain(`error=notowner`)
    // An affirmative not-member IS uncontrolled — the stale-grant scrub runs.
    expect(deletedTables).toContain(`github_installation_repo_grants`)
  })

  it(`multi-installation ticket carries ONLY the controlled ids`, async () => {
    listUserInstallations.mockResolvedValue([
      userInst(11, `octocat`),
      userInst(12, `stranger`),
      orgInst(21, `acme`),
    ])
    getUserOrgMembershipState.mockResolvedValue(`active`)

    const res = await handleCallback(callbackRequest(oauthState()))

    const location = res.headers.get(`location`)!
    expect(location).toContain(`/integrations/github/claim?ticket=`)
    const ticket = new URL(location, `https://app.example`).searchParams.get(
      `ticket`
    )
    const payload = readGithubClaimTicket(ticket, `user-1`)
    expect(payload?.ids).toEqual([11, 21])
    expect(payload?.w).toBe(`team-1`)
    // No auto-link on the multi path; grants captured only for controlled.
    expect(linkInserts()).toHaveLength(0)
    expect(listUserInstallationRepos).toHaveBeenCalledTimes(2)
  })

  it(`one controlled among several enumerated → auto-links just the controlled one`, async () => {
    listUserInstallations.mockResolvedValue([
      userInst(11, `octocat`),
      userInst(12, `stranger`),
    ])

    const res = await handleCallback(callbackRequest(oauthState()))

    expect(res.headers.get(`location`)).toBe(`/`)
    expect(linkInserts()).toHaveLength(1)
    expect(linkInserts()[0].values.githubInstallationId).toBe(`row-11`)
    expect(mirrorInserts().map((r) => r.values.installationId)).toEqual([11])
  })

  it(`fails closed to the exchange error when GET /user is unreadable`, async () => {
    listUserInstallations.mockResolvedValue([userInst(11, `octocat`)])
    getAuthenticatedGithubLogin.mockResolvedValue(null)

    const res = await handleCallback(callbackRequest(oauthState()))

    expect(linkInserts()).toHaveLength(0)
    expect(grantInserts()).toHaveLength(0)
    expect(res.headers.get(`location`)).toContain(`error=exchange`)
  })

  it(`zero installations → none, still offering the install link`, async () => {
    const res = await handleCallback(callbackRequest(oauthState()))

    expect(res.headers.get(`location`)).toContain(`error=none`)
    expect(res.headers.get(`location`)).toContain(
      encodeURIComponent(`https://github.com/apps/test-app/installations/new`)
    )
  })
})
