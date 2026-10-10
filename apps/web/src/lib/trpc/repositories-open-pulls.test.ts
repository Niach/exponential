import { beforeEach, describe, expect, it, vi } from "vitest"
import { PgDialect } from "drizzle-orm/pg-core"
import type { SQL } from "drizzle-orm"

// EXP-1244: `repositories.openPulls` lists the team's repos' open PRs that
// NO issue or run links. "Linked" is decided by the PR url across EVERY
// team (two teams on one repository, an issue moved across teams), never
// by the caller's team alone: the exclusion only hides a row. The output
// shape is pinned: desktop 0.14.65 reads it.

const h = vi.hoisted(() => ({
  selectQueue: [] as unknown[][],
  wheres: [] as Array<SQL | undefined>,
  listOpenPulls: vi.fn(
    async (
      _repo: string,
      _token: string | null
    ): Promise<Array<Record<string, unknown>>> => []
  ),
}))

vi.mock(`@/db/connection`, () => ({
  db: {
    select: () => ({ from: () => ({ where: () => ({ limit: async () => [] }) }) }),
    update: () => ({ set: () => ({ where: async () => {} }) }),
  },
}))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/team-membership`, () => ({
  assertTeamMember: vi.fn(async () => ({ role: `member` })),
  getIssueTeamContext: vi.fn(),
}))
vi.mock(`@/lib/trpc/integrations`, () => ({
  resolveRepoForConnect: vi.fn(),
  isInstallationLinkedToTeam: async () => true,
}))
vi.mock(`@/lib/integrations/github-app`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/integrations/github-app")>()),
  githubAppConfigured: () => true,
  resolveRepoInstallationTokenInfo: async () => ({
    token: `tok`,
    installationId: 77,
    expiresAt: null,
  }),
}))
vi.mock(`@/lib/integrations/github-pr`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/integrations/github-pr")>()),
  listOpenPulls: h.listOpenPulls,
}))

import { invalidateOpenPulls, repositoriesRouter } from "@/lib/trpc/repositories"

const TEAM = `11111111-1111-4111-8111-111111111111`

const db = {
  select: vi.fn(() => {
    const rows = h.selectQueue.shift() ?? []
    const builder = {
      from: () => builder,
      innerJoin: () => builder,
      where: (clause?: SQL) => {
        h.wheres.push(clause)
        return builder
      },
      orderBy: () => builder,
      limit: async () => rows,
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      then: (res: any, rej: any) => Promise.resolve(rows).then(res, rej),
    }
    return builder
  }),
}

const caller = repositoriesRouter.createCaller({
  session: { user: { id: `actor` } },
  db,
  request: new Request(`http://localhost/`),
} as never)

const pull = (n: number) => ({
  number: n,
  url: `https://github.com/owner/repo/pull/${n}`,
  title: `PR ${n}`,
  branch: `feature/${n}`,
  baseBranch: `master`,
  draft: false,
  authorLogin: `someone`,
  authorAvatarUrl: null,
  createdAt: `2026-10-10T00:00:00Z`,
})

const repoRow = {
  id: `repo-1`,
  fullName: `owner/repo`,
  teamId: TEAM,
  installationId: 77,
}

const render = (clause: SQL | undefined) =>
  clause ? new PgDialect().sqlToQuery(clause) : { sql: ``, params: [] }

beforeEach(() => {
  h.selectQueue.length = 0
  h.wheres.length = 0
  vi.clearAllMocks()
  invalidateOpenPulls(TEAM)
})

describe(`repositories.openPulls (EXP-1244)`, () => {
  it(`hides a PR any issue or run links, by repository url, in the pinned shape`, async () => {
    // The team's repos, then the linked urls: an issue row (ANY team) and a
    // run row (EXP-734) each carry one of the three open PRs.
    h.selectQueue.push([repoRow])
    h.selectQueue.push([{ prUrl: pull(5).url }])
    h.selectQueue.push([{ prUrl: pull(6).url }])
    h.listOpenPulls.mockResolvedValueOnce([pull(5), pull(6), pull(7)])

    await expect(caller.openPulls({ teamId: TEAM })).resolves.toEqual({
      repos: [{ repositoryId: `repo-1`, fullName: `owner/repo`, pulls: [pull(7)] }],
    })
    expect(h.listOpenPulls).toHaveBeenCalledWith(`owner/repo`, `tok`)
  })

  it(`keys the linked lookups on the repo's PR urls, never on the caller's team`, async () => {
    h.selectQueue.push([repoRow], [], [])
    h.listOpenPulls.mockResolvedValueOnce([pull(7)])

    await caller.openPulls({ teamId: TEAM })

    // The repos read is team-scoped; the two linked-url reads are not.
    const [repos, issueLinks, sessionLinks] = h.wheres.map(render)
    expect(repos?.params).toContain(TEAM)
    for (const linked of [issueLinks, sessionLinks]) {
      expect(linked?.sql).toMatch(/like/)
      expect(linked?.sql).not.toMatch(/team_id/)
      expect(linked?.params).toEqual([`https://github.com/owner/repo/pull/%`])
    }
  })

  it(`makes no linked lookup for a team without repos`, async () => {
    h.selectQueue.push([])

    await expect(caller.openPulls({ teamId: TEAM })).resolves.toEqual({ repos: [] })
    expect(db.select).toHaveBeenCalledTimes(1)
    expect(h.listOpenPulls).not.toHaveBeenCalled()
  })
})
