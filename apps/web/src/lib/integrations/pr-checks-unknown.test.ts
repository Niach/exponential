import { beforeEach, describe, expect, it, vi } from "vitest"
import prChecks from "@exp/domain-contract/fixtures/pr-checks.json"

// #FEED-198: `state none, total 0` says WHY. Driven through the repair read
// `exponential_pr` runs, with a fake GitHub and a fake row store.
const store = vi.hoisted(() => ({
  pr: {
    id: `p1`,
    head: `feature/a`,
    base: `main`,
    state: `open`,
    checksState: `none`,
    fullName: `o/r`,
    installationId: 7,
  },
  token: `tok` as string | null,
  isChecks: (_table: unknown): boolean => false,
}))
function chain(rows: unknown[]) {
  const link: Record<string, unknown> = {
    then: (resolve: (value: unknown[]) => void) => resolve(rows),
  }
  for (const method of [`innerJoin`, `where`, `limit`, `for`]) link[method] = () => link
  return link
}
vi.mock(`@/db/connection`, () => ({
  db: {
    select: () => ({ from: (table: unknown) => chain(store.isChecks(table) ? [] : [store.pr]) }),
    delete: () => chain([]),
    transaction: async () => null,
    update: () => ({ set: () => chain([]) }),
  },
}))
vi.mock(`@/lib/trpc`, () => ({ generateTxId: vi.fn() }))
vi.mock(`@/lib/sessions/messages`, () => ({ insertMessage: vi.fn(), inSession: vi.fn() }))
vi.mock(`@/lib/integrations/github-app`, () => ({
  githubApiHeaders: () => ({}),
  resolveRepoInstallationToken: async () => store.token,
}))

import { pullRequestChecks } from "@/db/schema"
import { resetGithubClientForTests } from "@/lib/integrations/github-client"
import { pullRequestChecksBrief, repairPullRequestChecks, resetPrChecksForTests } from "@/lib/integrations/pr-checks"

function github(status: number, body: unknown) {
  const reply = vi.fn(async () => new Response(JSON.stringify(body), { status }))
  vi.stubGlobal(`fetch`, reply)
  return reply
}

describe(`#FEED-198: a PR with no checks says why`, () => {
  beforeEach(() => {
    vi.spyOn(console, `warn`).mockImplementation(() => undefined)
    vi.spyOn(console, `error`).mockImplementation(() => undefined)
    resetGithubClientForTests()
    resetPrChecksForTests()
    store.token = `tok`
    store.isChecks = (table) => table === pullRequestChecks
  })

  it(`GitHub refuses the read: no_permission, the fix in the note`, async () => {
    github(403, { message: `Resource not accessible by integration` })
    expect(await pullRequestChecksBrief(`p1`)).toEqual({
      state: `none`,
      total: 0,
      failing: [],
      pending: [],
      unknown: `no_permission`,
      note: prChecks.unknownText.no_permission,
    })
  })

  it(`GitHub answers with no runs: no_runs`, async () => {
    github(200, { total_count: 0, check_runs: [] })
    expect((await pullRequestChecksBrief(`p1`)).unknown).toBe(`no_runs`)
  })

  it(`no installation reaches the repo: not_installed, and GitHub is not asked`, async () => {
    store.token = null
    const reply = github(200, {})
    expect((await pullRequestChecksBrief(`p1`)).unknown).toBe(`not_installed`)
    expect(reply).not.toHaveBeenCalled()
  })

  it(`a failed read is unreadable, not "no CI"`, async () => {
    github(404, {})
    expect((await pullRequestChecksBrief(`p1`)).unknown).toBe(`unreadable`)
  })

  it(`a throttled second op keeps the reason`, async () => {
    const reply = github(403, { message: `Resource not accessible by integration` })
    await pullRequestChecksBrief(`p1`)
    expect((await pullRequestChecksBrief(`p1`)).unknown).toBe(`no_permission`)
    expect(reply).toHaveBeenCalledTimes(1)
  })

  it(`a read that finds runs drops the reason`, async () => {
    github(200, { total_count: 0, check_runs: [] })
    await pullRequestChecksBrief(`p1`)
    github(200, {
      total_count: 1,
      check_runs: [{ name: `check`, status: `queued`, conclusion: null, head_sha: `sha1` }],
    })
    await repairPullRequestChecks(`p1`, Date.now() + 120_000)
    expect((await pullRequestChecksBrief(`p1`, { repair: false })).unknown).toBeUndefined()
  })
})
