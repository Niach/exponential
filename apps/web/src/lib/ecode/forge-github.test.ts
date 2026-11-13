import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { githubAuthorLine, githubForge, parsePipelineOutcome, setGithubForgeDepsForTests } from "@/lib/ecode/forge-github"
import { checkFromGithubStatus } from "@/lib/ecode/github-status"
import { resetGithubClientForTests } from "@/lib/integrations/github-client"
import { resetPrChecksForTests } from "@/lib/integrations/pr-checks"

// EXP-1709: the GitHub half of the forge seam, against a stub fetch.

interface Call {
  url: string
  method: string
  body: unknown
}

let calls: Call[] = []
let routes: Array<{ match: (c: Call) => boolean; status: number; body?: unknown }> = []

function stubFetch(url: string, init: RequestInit): Promise<Response> {
  const call: Call = {
    url,
    method: (init.method ?? `GET`).toUpperCase(),
    body: typeof init.body === `string` ? JSON.parse(init.body) : undefined,
  }
  calls.push(call)
  const route = routes.find((r) => r.match(call))
  const status = route?.status ?? 404
  const body = route?.body === undefined ? `` : JSON.stringify(route.body)
  return Promise.resolve(new Response(status === 204 ? null : body, { status }))
}

const repo = { id: `r1`, fullName: `org/repo` }
const on = (method: string, path: string, status: number, body?: unknown) =>
  routes.push({ match: (c) => c.method === method && c.url.includes(path), status, body })

beforeEach(() => {
  calls = []
  routes = []
  resetGithubClientForTests()
  resetPrChecksForTests()
  setGithubForgeDepsForTests({ deps: { fetch: stubFetch, sleep: async () => {} }, token: `ghs_test` })
})
afterEach(() => setGithubForgeDepsForTests(null))

describe(`githubForge`, () => {
  it(`reads a PR provider-neutrally, with the author as a commit author line`, async () => {
    on(`GET`, `/repos/org/repo/pulls/7`, 200, {
      state: `open`,
      merged: false,
      draft: true,
      title: `Add x`,
      user: { id: 42, login: `octo` },
      head: { sha: `h1`, ref: `exp/a` },
      base: { ref: `master` },
    })
    const pr = await githubForge.readPullRequest(repo, 7)
    expect(pr).toMatchObject({
      number: 7,
      state: `open`,
      merged: false,
      draft: true,
      headSha: `h1`,
      baseRef: `master`,
      title: `Add x`,
      author: `octo <42+octo@users.noreply.github.com>`,
    })
    expect(await githubForge.readPullRequest(repo, 8)).toBeNull()
  })

  it(`dispatches train.yml on the base with the contract's inputs; a missing workflow says so`, async () => {
    on(`POST`, `/actions/workflows/train.yml/dispatches`, 200, { workflow_run_id: 99 })
    const inputs = { train_id: `t1`, base: `master`, base_sha: `b0`, mode: `test` as const, units: `[]` }
    expect(await githubForge.dispatchPipeline(repo, inputs)).toEqual({ ok: true, runRef: `99` })
    expect(calls[0]!.body).toEqual({ ref: `master`, inputs, return_run_details: true })
    routes = []
    on(`POST`, `/actions/workflows/train.yml/dispatches`, 404, { message: `Not Found` })
    expect(await githubForge.dispatchPipeline(repo, inputs)).toMatchObject({ ok: false, missing: true })
  })

  it(`#FEED-223: an installation without Actions has no pipeline; any other refusal stays an error`, async () => {
    const inputs = { train_id: `t1`, base: `develop`, base_sha: `b0`, mode: `untested` as const, units: `[]` }
    on(`POST`, `/actions/workflows/train.yml/dispatches`, 403, { message: `Resource not accessible by integration` })
    expect(await githubForge.dispatchPipeline(repo, inputs)).toMatchObject({ ok: false, missing: true })
    routes = []
    on(`POST`, `/actions/workflows/train.yml/dispatches`, 422, { message: `Unexpected inputs provided` })
    expect(await githubForge.dispatchPipeline(repo, inputs)).toMatchObject({ ok: false, missing: false })
  })

  it(`reads the exp-train report; a finished run without one is finished with no outcome`, async () => {
    const outcome = { train: `t1`, base_sha: `b0`, head: `c1`, checks: `dispatched`, units: [], error: null }
    on(`GET`, `/commits/exp-train/t1/check-runs`, 200, {
      check_runs: [{ external_id: `t1`, status: `completed`, output: { text: JSON.stringify(outcome) } }],
    })
    expect(await githubForge.readPipeline(repo, `t1`, null)).toEqual({ outcome, finished: true })
    routes = []
    on(`GET`, `/commits/exp-train/t2/check-runs`, 200, { check_runs: [] })
    on(`GET`, `/actions/runs/5`, 200, { status: `completed` })
    expect(await githubForge.readPipeline(repo, `t2`, `5`)).toEqual({ outcome: null, finished: true })
    expect(await githubForge.readPipeline(repo, `t2`, null)).toEqual({ outcome: null, finished: false })
  })

  it(`fast-forwards only from the expected sha`, async () => {
    on(`GET`, `/git/ref/heads/master`, 200, { object: { sha: `b0` } })
    on(`PATCH`, `/git/refs/heads/master`, 200, {})
    expect(await githubForge.fastForward(repo, { branch: `master`, from: `b0`, to: `c1` })).toEqual({ ok: true })
    expect(calls.find((c) => c.method === `PATCH`)!.body).toEqual({ sha: `c1`, force: false })
    calls = []
    expect(await githubForge.fastForward(repo, { branch: `master`, from: `old`, to: `c1` })).toEqual({
      ok: false,
      reason: `moved`,
      current: `b0`,
    })
    expect(calls.some((c) => c.method === `PATCH`)).toBe(false)
    routes = []
    on(`GET`, `/git/ref/heads/master`, 200, { object: { sha: `b0` } })
    on(`PATCH`, `/git/refs/heads/master`, 422, { message: `Repository rule violations found` })
    expect(await githubForge.fastForward(repo, { branch: `master`, from: `b0`, to: `c1` })).toMatchObject({
      ok: false,
      reason: `refused`,
    })
  })

  it(`#FEED-212: reads whether a branch points at or contains a commit`, async () => {
    for (const [status, expected] of [
      [`identical`, true],
      [`ahead`, true],
      [`behind`, false],
      [`diverged`, false],
    ] as const) {
      routes = []
      on(`GET`, `/compare/c1...master`, 200, { status })
      expect(await githubForge.branchContains(repo, `master`, `c1`)).toBe(expected)
    }
    routes = []
    on(`GET`, `/compare/c1...master`, 404, { message: `Not Found` })
    expect(await githubForge.branchContains(repo, `master`, `c1`)).toBe(false)
    routes = []
    on(`GET`, `/compare/c1...master`, 500, { message: `boom` })
    expect(await githubForge.branchContains(repo, `master`, `c1`)).toBeNull()
  })

  it(`closes a landed PR with a comment naming the commit`, async () => {
    on(`POST`, `/issues/7/comments`, 201, {})
    on(`PATCH`, `/pulls/7`, 200, {})
    await githubForge.closeAsLanded(repo, { number: 7, sha: `c1`, base: `master` })
    expect(calls[0]!.body).toEqual({ body: `Landed in \`master\` as c1 by the Exponential merge train.` })
    expect(calls[1]!.body).toEqual({ state: `closed` })
  })

  it(`squash-merges directly with the final title and the expected head`, async () => {
    on(`PUT`, `/pulls/7/merge`, 200, { sha: `m1` })
    expect(await githubForge.squashMerge(repo, { number: 7, title: `Add x`, expectedHead: `h1` })).toEqual({
      ok: true,
      sha: `m1`,
    })
    expect(calls[0]!.body).toEqual({ merge_method: `squash`, commit_title: `Add x (#7)`, sha: `h1` })
  })

  it(`reads check runs and commit statuses at a sha`, async () => {
    on(`GET`, `/commits/c1/check-runs`, 200, {
      total_count: 1,
      check_runs: [{ name: `gate`, status: `completed`, conclusion: `success`, head_sha: `c1` }],
    })
    on(`GET`, `/commits/c1/status`, 200, { statuses: [{ context: `ci/legacy`, state: `pending` }] })
    expect(await githubForge.checksAt(repo, `c1`)).toEqual([
      { name: `gate`, status: `completed`, conclusion: `success` },
      { name: `ci/legacy`, status: `in_progress`, conclusion: null },
    ])
  })
})

describe(`helpers`, () => {
  it(`maps statuses, author lines and outcome JSON`, () => {
    expect(checkFromGithubStatus(`error`)).toEqual({ status: `completed`, conclusion: `failure` })
    expect(checkFromGithubStatus(`success`)).toEqual({ status: `completed`, conclusion: `success` })
    expect(githubAuthorLine({ login: `a` })).toBe(`a <a@users.noreply.github.com>`)
    expect(githubAuthorLine(null)).toBeNull()
    expect(parsePipelineOutcome(`nope`)).toBeNull()
    expect(parsePipelineOutcome(`{"units":[]}`)).toEqual({ units: [] })
  })
})
