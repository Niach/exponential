import { beforeEach, describe, expect, it, vi } from "vitest"

// #EXP-1614: `repositories.credentials` hands a device the repo's git remote
// and an HTTP basic credential for it (GitHub: the https remote,
// `x-access-token` and the installation token), so devices stop building
// GitHub URLs. `installationToken` stays as its one-release shim, its
// output unchanged (old desktop builds read exactly it).

const h = vi.hoisted(() => ({
  selectQueue: [] as unknown[][],
  writes: [] as Array<Record<string, unknown>>,
}))

function selectBuilder() {
  const rows = h.selectQueue.shift() ?? []
  const builder = {
    from: () => builder,
    innerJoin: () => builder,
    where: () => builder,
    limit: async () => rows,
  }
  return builder
}

vi.mock(`@/db/connection`, () => ({
  db: {
    select: () => selectBuilder(),
    update: () => ({
      set: (values: Record<string, unknown>) => ({
        where: async () => {
          h.writes.push(values)
        },
      }),
    }),
  },
}))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))
const { ecodeCredentials, ecodeRepositoryIdForRemote } = vi.hoisted(() => ({
  ecodeCredentials: vi.fn(async () => ({
    token: `expg_test`,
    expiresAt: new Date(0),
    remoteUrl: `exp://team/app`,
    username: `exp`,
    fullName: `team/app`,
    defaultBranch: `main`,
  })),
  ecodeRepositoryIdForRemote: vi.fn(async () => `00000000-0000-4000-8000-000000000001`),
}))
vi.mock(`@/lib/ecode/repos`, () => ({ ecodeCredentials, ecodeRepositoryIdForRemote }))
vi.mock(`@/lib/team-membership`, () => ({
  assertTeamMember: vi.fn(async () => ({ role: `member` })),
  getIssueTeamContext: vi.fn(),
}))
vi.mock(`@/lib/trpc/integrations`, () => ({ resolveRepoForConnect: vi.fn() }))
vi.mock(`@/lib/integrations/github-app`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/integrations/github-app")>()),
  githubAppConfigured: () => true,
  resolveRepoInstallationTokenInfo: vi.fn(async () => ({
    token: `ghs_minted`,
    installationId: 77,
    expiresAt: Date.parse(`2026-11-13T05:00:00Z`),
  })),
  resolveRepoDefaultBranch: vi.fn(async () => `master`),
}))

import { db } from "@/db/connection"
import { repositoriesRouter } from "@/lib/trpc/repositories"

const REPO_ID = `22222222-2222-4222-8222-222222222222`
const row = (extra: Record<string, unknown> = {}) => ({
  id: REPO_ID,
  teamId: `t1`,
  provider: `github`,
  fullName: `acme/app`,
  remoteUrl: null,
  defaultBranch: `master`,
  defaultBranchOverride: null,
  installationId: 77,
  inaccessibleAt: null,
  sharedByUserId: null,
  archivedAt: null,
  ...extra,
})

const caller = repositoriesRouter.createCaller({
  session: { user: { id: `actor` } },
  db,
  request: new Request(`http://localhost/`),
} as never)

beforeEach(() => {
  h.selectQueue.length = 0
  h.writes.length = 0
})

describe(`repositories.credentials (#EXP-1614)`, () => {
  it(`GitHub: the https remote, x-access-token and the installation token`, async () => {
    h.selectQueue.push([row()])
    await expect(caller.credentials({ repositoryId: REPO_ID })).resolves.toEqual({
      remoteUrl: `https://github.com/acme/app.git`,
      username: `x-access-token`,
      token: `ghs_minted`,
      expiresAt: new Date(`2026-11-13T05:00:00Z`),
      fullName: `acme/app`,
      defaultBranch: `master`,
    })
  })

  it(`a row's own remote_url wins`, async () => {
    h.selectQueue.push([row({ remoteUrl: `https://git.example/acme/app.git` })])
    const out = await caller.credentials({ repositoryId: REPO_ID })
    expect(out.remoteUrl).toBe(`https://git.example/acme/app.git`)
  })

  it(`installationToken (the shim) keeps its exact output`, async () => {
    h.selectQueue.push([row()])
    await expect(caller.installationToken({ repositoryId: REPO_ID })).resolves.toEqual({
      token: `ghs_minted`,
      fullName: `acme/app`,
      defaultBranch: `master`,
      expiresAt: new Date(`2026-11-13T05:00:00Z`),
    })
  })

  it(`a legacy row learns its installation (provider_config)`, async () => {
    h.selectQueue.push([row({ installationId: null })])
    await caller.credentials({ repositoryId: REPO_ID })
    expect(h.writes[0]).toMatchObject({ inaccessibleAt: null })
    expect(h.writes[0]!.providerConfig).toBeDefined()
  })

  it(`an eCode repository gets its exp:// remote and a repo token (#EXP-1616)`, async () => {
    h.selectQueue.push([row({ provider: `ecode`, fullName: `team/app` })])
    const out = await caller.credentials({ repositoryId: REPO_ID })
    expect(ecodeCredentials).toHaveBeenCalledWith(expect.any(String), REPO_ID)
    expect(out).toMatchObject({ remoteUrl: `exp://team/app`, username: `exp`, token: `expg_test`, defaultBranch: `main` })
  })

  it(`git-remote-exp names the repo by its exp:// URL (#EXP-1616)`, async () => {
    h.selectQueue.push([row({ provider: `ecode`, fullName: `team/app` })])
    await caller.credentials({ remote: `exp://team/app` })
    expect(ecodeRepositoryIdForRemote).toHaveBeenCalledWith(`exp://team/app`)
    await expect(caller.credentials({} as never)).rejects.toThrow()
  })
})
