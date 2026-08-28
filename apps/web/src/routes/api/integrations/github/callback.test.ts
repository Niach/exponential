import { beforeEach, describe, expect, it, vi } from "vitest"
import {
  githubSetupStateWantsMobile,
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
// Rows the mock `select()` resolves per ROOT table (the `.from()` argument) —
// the self-heal (EXP-365) reads links/grants/repositories before deleting.
const selectRows: Record<string, unknown[]> = {}

function tableName(table: unknown): string {
  return (table as { __name?: string } | undefined)?.__name ?? `unknown`
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
  const select = () => {
    let rootTable = `unknown`
    const chain: Record<string, unknown> = {}
    chain.from = (table: unknown) => {
      rootTable = tableName(table)
      return chain
    }
    chain.innerJoin = () => chain
    chain.where = () => Promise.resolve(selectRows[rootTable] ?? [])
    return chain
  }
  const dbLike = {
    insert,
    delete: del,
    select,
    transaction: async <T>(fn: (tx: unknown) => Promise<T>) => fn(dbLike),
  }
  return dbLike
}

vi.mock(`@/db/connection`, () => ({ db: mockDb() }))

vi.mock(`@/db/schema`, () => ({
  githubInstallations: { __name: `github_installations` },
  githubInstallationLinks: { __name: `github_installation_links` },
  githubInstallationRepoGrants: { __name: `github_installation_repo_grants` },
  repositories: { __name: `repositories` },
}))

const resolveSessionUserId = vi.fn(async () => `user-1` as string | null)
vi.mock(`@/lib/auth/resolve-bearer`, () => ({
  resolveSessionUserId: () => resolveSessionUserId(),
}))

// The callback gates on plain membership on both the link path and the reap
// (an ex-member reaps nothing). Default: an owner-member, overridden per
// test.
const getTeamMember = vi.fn(
  async (): Promise<{ role: string } | undefined> => ({ role: `owner` })
)
vi.mock(`@/lib/team-membership`, () => ({
  getTeamMember: () => getTeamMember(),
  assertTeamMember: async () => {
    const member = await getTeamMember()
    if (!member) throw new Error(`FORBIDDEN`)
    return member
  },
}))

vi.mock(`@/lib/trpc/integrations`, () => ({
  invalidateRepoCache: () => {},
  // The self-heal reap is a link DELETER, so it locks its candidate rows first
  // (EXP-371). The lock's own semantics are covered in integrations.test.ts;
  // here it resolves to "every candidate is still there".
  lockInstallationLinks: async (_tx: unknown, linkIds: string[]) => linkIds,
}))

const listUserInstallations = vi.fn(async (): Promise<AppInstallation[]> => [])
const defaultRepoListing = async (_token: string, installationId: number) => ({
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
const listUserInstallationRepos = vi.fn(defaultRepoListing)
const getAuthenticatedGithubUser = vi.fn(
  async (): Promise<{ id: number | null; login: string } | null> => ({
    id: 4242,
    login: `octocat`,
  })
)
const getUserOrgMembershipState = vi.fn(
  async (_token: string, _org: string): Promise<OrgMembershipState> =>
    `not-member`
)
const defaultInstallUrl = (state?: string): string | null =>
  `https://github.com/apps/test-app/installations/new?state=${state ?? ``}`
const githubAppInstallUrl = vi.fn(defaultInstallUrl)

vi.mock(`@/lib/integrations/github-app`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/integrations/github-app")>()),
  exchangeGithubOAuthCode: async () => `user-tok`,
  listUserInstallations: () => listUserInstallations(),
  listUserInstallationRepos: (token: string, id: number) =>
    listUserInstallationRepos(token, id),
  getAuthenticatedGithubUser: () => getAuthenticatedGithubUser(),
  getUserOrgMembershipState: (_token: string, org: string) =>
    getUserOrgMembershipState(_token, org),
  githubAppInstallUrl: (state?: string) => githubAppInstallUrl(state),
}))

// EXP-617: the callback is the one place that can prove which GitHub account
// an app user controls, so it records the mapping the PR webhooks resolve
// against.
const recordGithubIdentity = vi.fn(
  async (_args: {
    userId: string
    githubUserId: number
    githubLogin: string
  }): Promise<void> => {}
)
vi.mock(`@/lib/integrations/github-identity`, () => ({
  recordGithubIdentity: (args: {
    userId: string
    githubUserId: number
    githubLogin: string
  }) => recordGithubIdentity(args),
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
    for (const key of Object.keys(selectRows)) delete selectRows[key]
    resolveSessionUserId.mockResolvedValue(`user-1`)
    getTeamMember.mockResolvedValue({ role: `owner` })
    getAuthenticatedGithubUser.mockResolvedValue({ id: 4242, login: `octocat` })
    recordGithubIdentity.mockClear()
    getUserOrgMembershipState.mockResolvedValue(`not-member`)
    listUserInstallations.mockResolvedValue([])
    listUserInstallationRepos.mockReset()
    listUserInstallationRepos.mockImplementation(defaultRepoListing)
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
    // Non-empty first capture ⇒ no spurious retry (EXP-365).
    expect(listUserInstallationRepos).toHaveBeenCalledTimes(1)
  })

  // EXP-617: the code exchange is the proof of control, so the identity is
  // recorded on every completed callback — including the ones that end in a
  // refusal, because whose GitHub account this is does not depend on whether
  // an installation got linked.
  it(`records the GitHub identity of the connecting user`, async () => {
    listUserInstallations.mockResolvedValue([userInst(11, `OctoCat`)])

    await handleCallback(callbackRequest(oauthState()))

    expect(recordGithubIdentity).toHaveBeenCalledWith({
      userId: `user-1`,
      githubUserId: 4242,
      githubLogin: `octocat`,
    })
  })

  it(`skips the identity write when GitHub returns no numeric id`, async () => {
    getAuthenticatedGithubUser.mockResolvedValue({ id: null, login: `octocat` })
    listUserInstallations.mockResolvedValue([userInst(11, `OctoCat`)])

    const res = await handleCallback(callbackRequest(oauthState()))

    // The claim still succeeds — the identity is a bonus, never a gate.
    expect(res.headers.get(`location`)).toBe(`/`)
    expect(recordGithubIdentity).not.toHaveBeenCalled()
  })

  it(`refuses a collaborator-only enumeration: no link, no mirror, no grants — redirects to notowner with the install link`, async () => {
    // The exact EXP-363 leak: the OAuth user can reach one repo of a
    // stranger's installation, so GitHub enumerates it — but it is NOT theirs.
    listUserInstallations.mockResolvedValue([userInst(11, `Niach`)])
    getAuthenticatedGithubUser.mockResolvedValue({
      id: 4242,
      login: `LukeTechMech`,
    })

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
    getAuthenticatedGithubUser.mockResolvedValue(null)

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

// EXP-365: a pre-EXP-363 claim could link a stranger's installation to a team.
// Such a link can never leave on its own (the linking user doesn't control it,
// so a re-auth scrubs-only and `needsReauth` warns forever) — the callback now
// reaps the team's links that the current re-auth proved stale, but ONLY when
// they are this user's own residue and nothing depends on them.
describe(`github OAuth callback — stale-link self-heal (EXP-365)`, () => {
  const staleLink = (overrides: Partial<Record<string, unknown>> = {}) => ({
    linkId: `link-x`,
    createdByUserId: `user-1`,
    installationId: 999,
    suspendedAt: null,
    ...overrides,
  })

  beforeEach(() => {
    process.env.BETTER_AUTH_SECRET = `test-secret-test-secret-test-secret!`
    insertedRows.length = 0
    deletedTables.length = 0
    for (const key of Object.keys(selectRows)) delete selectRows[key]
    resolveSessionUserId.mockResolvedValue(`user-1`)
    getTeamMember.mockResolvedValue({ role: `owner` })
    getAuthenticatedGithubUser.mockResolvedValue({ id: 4242, login: `octocat` })
    recordGithubIdentity.mockClear()
    getUserOrgMembershipState.mockResolvedValue(`not-member`)
    listUserInstallations.mockResolvedValue([userInst(11, `octocat`)])
    listUserInstallationRepos.mockReset()
    listUserInstallationRepos.mockImplementation(defaultRepoListing)
  })

  it(`reaps a stale link: same creator, uncontrolled, zero grants, zero repos`, async () => {
    selectRows[`github_installation_links`] = [staleLink()]

    const res = await handleCallback(callbackRequest(oauthState()))

    expect(res.headers.get(`location`)).toBe(`/`)
    expect(deletedTables).toContain(`github_installation_links`)
  })

  it(`reaps on the dead-end path too — a notowner re-auth still proves non-control`, async () => {
    // The EXP-365 prod shape: the only enumeration is the stranger's
    // installation the pre-fix flow linked; the victim's re-auth controls
    // nothing but still erases their own residue.
    listUserInstallations.mockResolvedValue([userInst(999, `Niach`)])
    getAuthenticatedGithubUser.mockResolvedValue({
      id: 4242,
      login: `LukeTechMech`,
    })
    selectRows[`github_installation_links`] = [staleLink()]

    const res = await handleCallback(callbackRequest(oauthState()))

    expect(res.headers.get(`location`)).toContain(`error=notowner`)
    expect(deletedTables).toContain(`github_installation_links`)
  })

  it(`never reaps a teammate's link (different creator)`, async () => {
    selectRows[`github_installation_links`] = [
      staleLink({ createdByUserId: `user-2` }),
    ]

    await handleCallback(callbackRequest(oauthState()))

    expect(deletedTables).not.toContain(`github_installation_links`)
  })

  it(`never reaps a link the user still controls`, async () => {
    selectRows[`github_installation_links`] = [
      staleLink({ installationId: 11 }),
    ]

    await handleCallback(callbackRequest(oauthState()))

    expect(deletedTables).not.toContain(`github_installation_links`)
  })

  it(`never reaps while org membership is undetermined (members-read not approved)`, async () => {
    listUserInstallations.mockResolvedValue([
      userInst(11, `octocat`),
      orgInst(999, `acme`),
    ])
    getUserOrgMembershipState.mockResolvedValue(`permission-missing`)
    selectRows[`github_installation_links`] = [staleLink()]

    await handleCallback(callbackRequest(oauthState()))

    expect(deletedTables).not.toContain(`github_installation_links`)
  })

  it(`never reaps while any member holds grants for the installation`, async () => {
    selectRows[`github_installation_links`] = [staleLink()]
    selectRows[`github_installation_repo_grants`] = [{ installationId: 999 }]

    await handleCallback(callbackRequest(oauthState()))

    expect(deletedTables).not.toContain(`github_installation_links`)
  })

  it(`never reaps while active repositories use the installation`, async () => {
    selectRows[`github_installation_links`] = [staleLink()]
    selectRows[`repositories`] = [{ installationId: 999 }]

    await handleCallback(callbackRequest(oauthState()))

    expect(deletedTables).not.toContain(`github_installation_links`)
  })

  it(`never reaps a suspended installation's link (REV2-29: links survive suspension)`, async () => {
    selectRows[`github_installation_links`] = [
      staleLink({ suspendedAt: new Date() }),
    ]

    await handleCallback(callbackRequest(oauthState()))

    expect(deletedTables).not.toContain(`github_installation_links`)
  })

  it(`skips silently when the acting user is no longer a member`, async () => {
    // Two controlled installations reach the ticket hand-off, which does not
    // gate on membership — isolating the self-heal's own permission guard.
    listUserInstallations.mockResolvedValue([
      userInst(11, `octocat`),
      orgInst(21, `acme`),
    ])
    getUserOrgMembershipState.mockResolvedValue(`active`)
    getTeamMember.mockResolvedValue(undefined)
    selectRows[`github_installation_links`] = [staleLink()]

    const res = await handleCallback(callbackRequest(oauthState()))

    expect(res.headers.get(`location`)).toContain(
      `/integrations/github/claim?ticket=`
    )
    expect(deletedTables).not.toContain(`github_installation_links`)
  })

  // EXP-639: NULL creators are never this user's residue. The column is ON
  // DELETE SET NULL, so a link whose creator deleted their account reads NULL
  // while still being a teammate's live connection — owners disconnect those
  // by hand from the stale card instead.
  it(`never reaps a NULL-creator link, not even for an owner`, async () => {
    selectRows[`github_installation_links`] = [
      staleLink({ createdByUserId: null }),
    ]

    await handleCallback(callbackRequest(oauthState()))

    expect(deletedTables).not.toContain(`github_installation_links`)
  })

  it(`a plain member still reaps their OWN residue`, async () => {
    getTeamMember.mockResolvedValue({ role: `member` })
    selectRows[`github_installation_links`] = [staleLink()]

    await handleCallback(callbackRequest(oauthState()))

    expect(deletedTables).toContain(`github_installation_links`)
  })
})

// EXP-365: GitHub's user-installation repo listing runs empty for a few
// seconds right after a fresh install — a zero-grant capture stranded the
// connect (empty picker, FORBIDDEN adds) until a manual reconnect.
describe(`github OAuth callback — empty-capture retry (EXP-365)`, () => {
  beforeEach(() => {
    process.env.BETTER_AUTH_SECRET = `test-secret-test-secret-test-secret!`
    insertedRows.length = 0
    deletedTables.length = 0
    for (const key of Object.keys(selectRows)) delete selectRows[key]
    resolveSessionUserId.mockResolvedValue(`user-1`)
    getTeamMember.mockResolvedValue({ role: `owner` })
    getAuthenticatedGithubUser.mockResolvedValue({ id: 4242, login: `octocat` })
    recordGithubIdentity.mockClear()
    getUserOrgMembershipState.mockResolvedValue(`not-member`)
    listUserInstallations.mockResolvedValue([userInst(11, `octocat`)])
    listUserInstallationRepos.mockReset()
    listUserInstallationRepos.mockImplementation(defaultRepoListing)
  })

  it(`retries once when a controlled installation lists zero repos and keeps the second result`, async () => {
    vi.useFakeTimers()
    try {
      listUserInstallationRepos.mockResolvedValueOnce({
        repos: [],
        hasMore: false,
      })

      const pending = handleCallback(callbackRequest(oauthState()))
      await vi.advanceTimersByTimeAsync(2000)
      const res = await pending

      expect(res.headers.get(`location`)).toBe(`/`)
      expect(listUserInstallationRepos).toHaveBeenCalledTimes(2)
      expect(grantInserts()).toHaveLength(1)
      expect(linkInserts()).toHaveLength(1)
    } finally {
      vi.useRealTimers()
    }
  })

  it(`captures honestly empty when the retry is empty too — and still links`, async () => {
    vi.useFakeTimers()
    try {
      listUserInstallationRepos.mockResolvedValue({ repos: [], hasMore: false })

      const pending = handleCallback(callbackRequest(oauthState()))
      await vi.advanceTimersByTimeAsync(2000)
      const res = await pending

      expect(res.headers.get(`location`)).toBe(`/`)
      expect(listUserInstallationRepos).toHaveBeenCalledTimes(2)
      expect(grantInserts()).toHaveLength(0)
      expect(linkInserts()).toHaveLength(1)
    } finally {
      vi.useRealTimers()
    }
  })
})

// EXP-365: native clients are bearer-only — the browser hop carries no cookie
// session. Mobile-minted states act as their embedded user, and every error
// branch hands back to the app via the deep-link page instead of stranding
// the user on a web route behind the login wall.
describe(`github OAuth callback — cookie-less mobile flows (EXP-365)`, () => {
  const mobileState = (userId = `user-1`, teamId = `team-1`): string =>
    mintGithubSetupState(userId, { teamId, oauth: true, mobile: true })!

  beforeEach(() => {
    process.env.BETTER_AUTH_SECRET = `test-secret-test-secret-test-secret!`
    insertedRows.length = 0
    deletedTables.length = 0
    for (const key of Object.keys(selectRows)) delete selectRows[key]
    resolveSessionUserId.mockResolvedValue(null)
    getTeamMember.mockResolvedValue({ role: `owner` })
    getAuthenticatedGithubUser.mockResolvedValue({ id: 4242, login: `octocat` })
    recordGithubIdentity.mockClear()
    getUserOrgMembershipState.mockResolvedValue(`not-member`)
    listUserInstallations.mockResolvedValue([userInst(11, `octocat`)])
    listUserInstallationRepos.mockReset()
    listUserInstallationRepos.mockImplementation(defaultRepoListing)
    githubAppInstallUrl.mockReset()
    githubAppInstallUrl.mockImplementation(defaultInstallUrl)
  })

  it(`completes a single-install claim with NO session (state-only trust)`, async () => {
    const res = await handleCallback(callbackRequest(mobileState()))

    expect(res.status).toBe(200)
    expect(await res.text()).toContain(`exponential://github-connected`)
    expect(linkInserts()).toHaveLength(1)
    expect(linkInserts()[0].values.createdByUserId).toBe(`user-1`)
    // Grant rows are one batched insert of row objects.
    const grantRows = grantInserts()[0]?.values as unknown as Array<{
      grantedByUserId: string
    }>
    expect(grantRows[0]?.grantedByUserId).toBe(`user-1`)
  })

  it(`a web-minted state with no session still dead-ends on the session error`, async () => {
    const res = await handleCallback(callbackRequest(oauthState()))

    expect(res.status).toBe(302)
    expect(res.headers.get(`location`)).toContain(`error=session`)
    expect(linkInserts()).toHaveLength(0)
  })

  it(`a PRESENT mismatching session rejects a mobile state — via the deep-link page`, async () => {
    resolveSessionUserId.mockResolvedValue(`attacker`)

    const res = await handleCallback(callbackRequest(mobileState(`victim`)))

    expect(res.status).toBe(200)
    expect(await res.text()).toContain(`github-connected?error=session`)
    expect(linkInserts()).toHaveLength(0)
  })

  // EXP-390: `error=none`'s deep link closed the in-app browser instantly and
  // both mobile apps dropped the query — a zero-installation user had NO path
  // to GitHub's install page. Installable dead ends now continue there in the
  // same browser sheet instead of bouncing back to the app.
  it(`zero installations continue to the install page instead of the error deep link`, async () => {
    listUserInstallations.mockResolvedValue([])

    const res = await handleCallback(callbackRequest(mobileState()))

    expect(res.status).toBe(302)
    const location = res.headers.get(`location`)!
    expect(location).toContain(`https://github.com/apps/test-app/installations/new`)
    // The re-minted state keeps the mobile marker so the post-install
    // setup → OAuth round trip stays a cookie-less mobile flow.
    const state = new URL(location).searchParams.get(`state`)
    expect(githubSetupStateWantsMobile(state)).toBe(true)
  })

  it(`a collaborator-only enumeration (notowner) also continues to the install page`, async () => {
    listUserInstallations.mockResolvedValue([userInst(11, `Niach`)])
    getAuthenticatedGithubUser.mockResolvedValue({
      id: 4242,
      login: `LukeTechMech`,
    })

    const res = await handleCallback(callbackRequest(mobileState()))

    expect(res.status).toBe(302)
    expect(res.headers.get(`location`)).toContain(
      `https://github.com/apps/test-app/installations/new`
    )
    expect(linkInserts()).toHaveLength(0)
  })

  it(`orgperm keeps the error deep link — installing can't fix a pending org approval`, async () => {
    listUserInstallations.mockResolvedValue([orgInst(21, `acme`)])
    getUserOrgMembershipState.mockResolvedValue(`permission-missing`)

    const res = await handleCallback(callbackRequest(mobileState()))

    expect(res.status).toBe(200)
    expect(await res.text()).toContain(`github-connected?error=orgperm`)
  })

  it(`falls back to the error deep link when no install URL can be minted`, async () => {
    listUserInstallations.mockResolvedValue([])
    githubAppInstallUrl.mockReturnValue(null)

    const res = await handleCallback(callbackRequest(mobileState()))

    expect(res.status).toBe(200)
    expect(await res.text()).toContain(`github-connected?error=none`)
  })
})
