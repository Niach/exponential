import { afterEach, beforeEach, describe, expect, it } from "vitest"
import forgeFixture from "@exp/ecode/fixtures/forge.json"
import {
  byCodePoint,
  capabilityValues,
  forge as reference,
  hydrate,
  reasonCode,
  supports,
  type ForgeGiven,
  type ForgeReason,
  type ForgeState,
} from "@exp/ecode"
import { ForgeError, type ForgeRepo } from "@/lib/ecode/forge"
import { githubForge, setGithubForgeDepsForTests } from "@/lib/ecode/forge-github"
import { resetGithubClientForTests } from "@/lib/integrations/github-client"

// #EXP-1614: eCode's forge spec (packages/ecode fixtures/forge.json) replays
// against `githubForge`. Behind it sits a FAKE GITHUB: GitHub's REST surface
// (the paths, bodies, statuses and refusal messages GitHub sends) over an
// in-memory repository, which is eCode's reference model itself. So every
// step proves the op mapping both ways: githubForge sends the request GitHub
// expects, and reads GitHub's answer (or refusal) back into the spec's result
// (or reason).
//
// Where GitHub differs, by design:
// - `pr.mergeability`: GitHub computes clean/dirty but names no paths
//   (`conflicts` is always []).
// - the fake merges as a repo with "delete head branches" on: GitHub then
//   retargets the children and deletes the head (forge.json's squash merge).
//   Without it, the server's pr-sync does both after the merge webhook.
// - `push` and `branch.rebase_onto` are the device's git, not a forge call
//   (GitHub lacks `rebase_onto`): they apply to the fake repo directly.
// - events arrive as GitHub's webhooks, not as op results: not compared.
// - `cycle` (and closing a closed PR) are eCode refusals GitHub has no
//   message for; the fake answers them with a 422 naming the reason.

const REPO: ForgeRepo = { id: `r1`, fullName: `o/r` }
const API = `https://api.github.com/repos/o/r`

type Json = Record<string, unknown>

/** The refusal GitHub sends for a reason: [status, body]. */
function refusal(reason: ForgeReason, op: string, args: Json): [number, Json] {
  const validation = (field: string) => ({
    message: `Validation Failed`,
    errors: [{ resource: `PullRequest`, field, code: `invalid` }],
  })
  switch (reason) {
    case `pr`:
      return [404, { message: `Not Found` }]
    case `head`:
      return [422, validation(`head`)]
    case `base`:
      return [422, validation(`base`)]
    case `same_branch`:
      return [422, { message: `Validation Failed`, errors: [{ message: `No commits between ${args.head} and ${args.base}` }] }]
    case `exists`:
      return [422, { message: `Validation Failed`, errors: [{ message: `A pull request already exists for o:${args.head}.` }] }]
    case `cycle`:
      return [422, { message: `Validation Failed`, errors: [{ message: `base would create a cycle` }] }]
    case `draft`:
      return [405, { message: `Pull Request is still a draft` }]
    case `head_moved`:
      return [409, { message: `Head branch was modified. Review and try the merge again.` }]
    case `dirty`:
      return [405, { message: `Pull Request is not mergeable` }]
    case `not_open`:
      return op === `pr.retarget`
        ? [422, { message: `Cannot change the base branch of a closed pull request.` }]
        : op === `pr.close`
          ? [422, { message: `Pull request is already closed` }]
          : [405, { message: `Pull Request is not open` }]
    default:
      return [422, { message: `Validation Failed: ${reason}` }]
  }
}

/** GitHub's REST surface over the reference model. */
class FakeGithub {
  constructor(public state: ForgeState) {}
  requests: string[] = []

  private run(op: string, args: Json): { status: number; body: Json | null; result?: Json } {
    const outcome = reference(this.state, op, args)
    if (outcome.failure) {
      const [status, body] = refusal(outcome.failure.reason, op, args)
      return { status, body }
    }
    this.state = outcome.state
    return { status: 200, body: null, result: outcome.result as Json }
  }

  private pull(number: number): Json | null {
    const pr = this.state.prs.find((p) => p.number === number)
    if (!pr) return null
    const live = pr.state === `open` || pr.state === `draft`
    const mergeable = live
      ? (reference(this.state, `pr.mergeability`, { number }).result as { mergeability?: string } | undefined)
          ?.mergeability === `clean`
      : null
    return {
      number,
      state: live ? `open` : `closed`,
      merged: pr.state === `merged`,
      draft: pr.state === `draft`,
      title: pr.title,
      html_url: `https://github.com/o/r/pull/${number}`,
      head: { ref: pr.head, sha: live ? (this.state.branches[pr.head] ?? null) : pr.headSha },
      base: { ref: pr.base },
      mergeable,
      user: null,
    }
  }

  private listed(filter: (pr: ForgeState[`prs`][number]) => boolean): Json[] {
    // GitHub lists newest first.
    return this.state.prs
      .filter((pr) => (pr.state === `open` || pr.state === `draft`) && filter(pr))
      .sort((a, b) => b.number - a.number)
      .map((pr) => this.pull(pr.number)!)
  }

  handle(url: string, init: RequestInit): Response {
    const method = (init.method ?? `GET`).toUpperCase()
    const body = typeof init.body === `string` ? (JSON.parse(init.body) as Json) : {}
    this.requests.push(`${method} ${url.replace(API, ``)}`)
    const reply = (status: number, json: unknown) => new Response(JSON.stringify(json), { status })
    if (!url.startsWith(API)) return reply(404, { message: `Not Found` })
    const parsed = new URL(url)
    const path = parsed.pathname.replace(`/repos/o/r`, ``)
    const q = parsed.searchParams

    if (path === `/pulls` && method === `POST`) {
      const out = this.run(`pr.open`, { head: body.head, base: body.base, title: body.title, draft: body.draft === true })
      if (!out.result) return reply(out.status, out.body)
      return reply(201, this.pull(out.result.number as number))
    }
    if (path === `/pulls` && method === `GET`) {
      const base = q.get(`base`)
      const head = q.get(`head`)?.split(`:`)[1]
      return reply(200, this.listed((pr) => (!base || pr.base === base) && (!head || pr.head === head)))
    }
    const pull = path.match(/^\/pulls\/(\d+)(\/merge|\/files)?$/)
    if (pull) {
      const number = Number(pull[1])
      if (!pull[2] && method === `GET`) {
        const json = this.pull(number)
        return json ? reply(200, json) : reply(404, { message: `Not Found` })
      }
      if (!pull[2] && method === `PATCH`) {
        const out =
          body.state === `closed`
            ? this.run(`pr.close`, { number })
            : this.run(`pr.retarget`, { number, base: body.base })
        return out.result ? reply(200, this.pull(number)) : reply(out.status, out.body)
      }
      if (pull[2] === `/merge` && method === `PUT`) {
        const out = this.run(`pr.squash_merge`, { number, expectedHead: body.sha ?? null })
        return out.result ? reply(200, { sha: out.result.sha, merged: true }) : reply(out.status, out.body)
      }
      if (pull[2] === `/files` && method === `GET`) {
        const out = this.run(`pr.files`, { number })
        if (!out.result) return reply(out.status, out.body)
        // GitHub's own order (here: reversed), its own field names.
        const files = (out.result.files as Array<{ path: string; change: string }>)
          .map((f) => ({ filename: f.path, status: f.change, additions: 1, deletions: 0 }))
          .reverse()
        return reply(200, files)
      }
    }
    return reply(404, { message: `Not Found` })
  }
}

interface Step {
  op: string
  args: Json
  result?: Json
  failure?: { code: string; reason: ForgeReason }
}

/** One fixture step through githubForge: its result, or its refusal. */
async function viaGithub(fake: FakeGithub, step: Step): Promise<{ result?: Json; failure?: Json }> {
  const a = step.args
  const n = a.number as number
  try {
    switch (step.op) {
      case `pr.open`: {
        const opened = await githubForge.openPullRequest(REPO, {
          head: a.head as string,
          base: a.base as string,
          title: a.title as string,
          ...(a.draft ? { draft: true } : {}),
        })
        return { result: { number: opened.number } }
      }
      case `pr.list_by_base`:
        return { result: { numbers: (await githubForge.listOpenByBase(REPO, a.base as string)).map((p) => p.number) } }
      case `pr.retarget`:
        await githubForge.retarget(REPO, { number: n, base: a.base as string })
        return { result: {} }
      case `pr.mergeability`: {
        const { mergeability } = await githubForge.mergeability(REPO, n)
        return { result: { mergeability } }
      }
      case `pr.squash_merge`: {
        const title = fake.state.prs.find((p) => p.number === n)?.title ?? ``
        const merged = await githubForge.squashMerge(REPO, {
          number: n,
          title,
          expectedHead: (a.expectedHead as string | undefined) ?? null,
        })
        if (merged.ok) return { result: { sha: merged.sha } }
        return { failure: { code: reasonCode(merged.reason!), reason: merged.reason } }
      }
      case `pr.merge_ordered`: {
        const out = await githubForge.mergeOrdered(REPO, { numbers: a.numbers as number[] })
        return {
          result: {
            merged: out.merged,
            failed: out.failed && {
              number: out.failed.number,
              code: reasonCode(out.failed.reason!),
              reason: out.failed.reason,
            },
          },
        }
      }
      case `pr.close`:
        await githubForge.close(REPO, { number: n })
        return { result: {} }
      case `pr.files`:
        return {
          result: { files: (await githubForge.files(REPO, n)).map((f) => ({ path: f.path, change: f.change })) },
        }
      default: {
        // The device's git (push, rebase --onto): straight onto the repo.
        const outcome = reference(fake.state, step.op, a)
        if (outcome.failure) return { failure: { ...outcome.failure } }
        fake.state = outcome.state
        return { result: outcome.result as Json }
      }
    }
  } catch (e) {
    if (!(e instanceof ForgeError) || !e.reason) throw e
    return { failure: { code: reasonCode(e.reason), reason: e.reason } }
  }
}

/** The fixture's expected result, minus what GitHub cannot say. */
function expectedOf(step: Step): { result?: Json; failure?: Json } {
  if (step.failure) return { failure: { ...step.failure } }
  if (step.op === `pr.mergeability`) return { result: { mergeability: step.result!.mergeability } }
  return { result: step.result ?? {} }
}

const sortKeys = <T,>(record: Record<string, T>): Record<string, T> =>
  Object.fromEntries(Object.entries(record).sort(([x], [y]) => byCodePoint(x, y)))

let fake: FakeGithub

beforeEach(() => {
  resetGithubClientForTests()
  setGithubForgeDepsForTests({
    deps: { fetch: async (url, init) => fake.handle(url, init), sleep: async () => {} },
    token: `ghs_test`,
  })
})
afterEach(() => setGithubForgeDepsForTests(null))

describe(`githubForge conforms to forge.json (#EXP-1614)`, () => {
  for (const entry of forgeFixture.cases as unknown as Array<{ name: string; given: ForgeGiven; steps: Step[]; expect: Json }>) {
    it(entry.name, async () => {
      const start = hydrate({ ...entry.given, provider: `github` })
      fake = new FakeGithub(start)
      for (const [index, step] of entry.steps.entries()) {
        const got = await viaGithub(fake, step)
        expect(got, `step ${index}: ${step.op} ${JSON.stringify(step.args)}`).toEqual(expectedOf(step))
      }
      const commits: Json = {}
      for (const [id, commit] of Object.entries(fake.state.commits)) if (!start.commits[id]) commits[id] = commit
      expect({
        branches: sortKeys(fake.state.branches),
        prs: [...fake.state.prs].sort((x, y) => x.number - y.number),
        commits: sortKeys(commits),
      }).toEqual(entry.expect)
    })
  }

  it(`declares the capabilities of the github provider`, () => {
    expect([...githubForge.capabilities]).toEqual(capabilityValues.filter((c) => supports(`github`, c)))
    for (const row of forgeFixture.capabilities.filter((r) => r.provider === `github`)) {
      expect(githubForge.capabilities.includes(row.capability as never), row.capability).toBe(row.supported)
    }
  })

  it(`maps each op onto GitHub's REST call`, async () => {
    fake = new FakeGithub(
      hydrate({
        provider: `github`,
        commits: { c1: { changes: { "a.txt": `1` } }, c2: { parent: `c1`, changes: { "b.txt": `x` } } },
        branches: { main: `c1`, topic: `c2` },
      })
    )
    await githubForge.openPullRequest(REPO, { head: `topic`, base: `main`, title: `T`, draft: true })
    await githubForge.listOpenByBase(REPO, `main`)
    await githubForge.retarget(REPO, { number: 1, base: `main` })
    await githubForge.files(REPO, 1)
    await githubForge.squashMerge(REPO, { number: 1, title: `T`, expectedHead: `c2` }) // a draft: refused
    await githubForge.close(REPO, { number: 1 })
    expect(fake.requests).toEqual([
      `POST /pulls`,
      `GET /pulls?state=open&base=main&per_page=100`,
      `PATCH /pulls/1`,
      `GET /pulls/1/files?per_page=100`,
      `PUT /pulls/1/merge`,
      `PATCH /pulls/1`,
    ])
  })
})
