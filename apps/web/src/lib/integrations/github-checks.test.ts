import { describe, expect, it } from "vitest"
import {
  evaluateChecks,
  isRequiredCheckRefusal,
  pullRequestChecks,
  pullRequestHeadSha,
} from "@/lib/integrations/github-checks"
import type { GitHubFetch } from "@/lib/integrations/github-pr"

const done = (name: string, conclusion: string) => ({ name, status: `completed`, conclusion })
const running = (name: string) => ({ name, status: `in_progress`, conclusion: null })

describe(`evaluateChecks (EXP-1378)`, () => {
  it(`required checks green: green, whatever else runs`, () => {
    expect(evaluateChecks([`gate`], [done(`gate`, `success`), running(`bench`)], [])).toEqual({
      state: `green`,
    })
  })

  it(`a required check that has not reported yet is pending`, () => {
    expect(evaluateChecks([`gate`], [], [])).toEqual({ state: `pending`, names: [`gate`] })
  })

  it(`red wins over pending`, () => {
    expect(
      evaluateChecks([`gate`, `lint`], [running(`gate`), done(`lint`, `failure`)], [])
    ).toEqual({ state: `red`, names: [`lint`] })
  })

  it(`nothing required: every reported check counts`, () => {
    expect(evaluateChecks([], [done(`a`, `success`), running(`b`)], [])).toEqual({
      state: `pending`,
      names: [`b`],
    })
    expect(evaluateChecks([], [done(`a`, `skipped`), done(`b`, `neutral`)], [])).toEqual({
      state: `green`,
    })
    expect(evaluateChecks([], [], [])).toEqual({ state: `green` })
  })

  it(`the newest run of a name wins (GitHub lists it first)`, () => {
    expect(evaluateChecks([`gate`], [done(`gate`, `success`), done(`gate`, `failure`)], [])).toEqual({
      state: `green`,
    })
  })

  it(`commit statuses count by context`, () => {
    expect(evaluateChecks([`ci/legacy`], [], [{ context: `ci/legacy`, state: `pending` }])).toEqual({
      state: `pending`,
      names: [`ci/legacy`],
    })
    expect(evaluateChecks([], [], [{ context: `ci/legacy`, state: `error` }])).toEqual({
      state: `red`,
      names: [`ci/legacy`],
    })
  })
})

/** Routes by path; a `{ status }` value answers that HTTP status; an array
 *  of bodies = pages 1, 2, …; an unknown path = 404. */
function stub(routes: Record<string, unknown>, seen: string[] = []): GitHubFetch {
  return async (url) => {
    seen.push(url)
    const path = url.replace(`https://api.github.com`, ``).replace(/\?.*$/, ``)
    const page = Number(new URL(url).searchParams.get(`page`) ?? `1`)
    let body = routes[path]
    if (Array.isArray(body) && path.includes(`/commits/`)) body = body[page - 1] ?? {}
    const failed = body !== undefined && typeof body === `object` && body !== null && `status` in body && typeof (body as { status: unknown }).status === `number`
    const status = body === undefined ? 404 : failed ? (body as { status: number }).status : 200
    return {
      ok: status < 300,
      status,
      text: async () => JSON.stringify(body ?? {}),
      json: async () => body,
    }
  }
}

const PULL = { "/repos/o/r/pulls/7": { head: { sha: `abc` }, base: { ref: `master` } } }
const GATE_RULE = {
  "/repos/o/r/rules/branches/master": [
    { type: `required_status_checks`, parameters: { required_status_checks: [{ context: `gate` }] } },
  ],
}

describe(`pullRequestChecks`, () => {
  it(`reads the head's checks against the base's rulesets, with the sha it judged`, async () => {
    const verdict = await pullRequestChecks({
      repo: `o/r`,
      prNumber: 7,
      token: `t`,
      fetchImpl: stub({
        ...PULL,
        ...GATE_RULE,
        "/repos/o/r/commits/abc/check-runs": { check_runs: [running(`gate`)] },
        "/repos/o/r/commits/abc/status": { statuses: [] },
      }),
    })
    expect(verdict).toEqual({ state: `pending`, names: [`gate`], sha: `abc` })
  })

  it(`no protection at all (404s): every reported check counts`, async () => {
    const verdict = await pullRequestChecks({
      repo: `o/r`,
      prNumber: 7,
      token: `t`,
      fetchImpl: stub({
        ...PULL,
        "/repos/o/r/commits/abc/check-runs": { check_runs: [done(`gate`, `success`)] },
        "/repos/o/r/commits/abc/status": { statuses: [] },
      }),
    })
    expect(verdict).toEqual({ state: `green`, sha: `abc` })
  })

  it(`no readable checks (the App lacks the permission): no verdict`, async () => {
    const verdict = await pullRequestChecks({ repo: `o/r`, prNumber: 7, token: `t`, fetchImpl: stub(PULL) })
    expect(verdict).toBeNull()
  })

  for (const status of [403, 429, 502]) {
    it(`a partial read fails closed: check runs answer ${status} while statuses answer empty`, async () => {
      const verdict = await pullRequestChecks({
        repo: `o/r`,
        prNumber: 7,
        token: `t`,
        fetchImpl: stub({
          ...PULL,
          "/repos/o/r/commits/abc/check-runs": { status },
          "/repos/o/r/commits/abc/status": { statuses: [] },
        }),
      })
      expect(verdict).toBeNull()
    })
  }

  it(`statuses unreadable while the check runs are green: no verdict`, async () => {
    const verdict = await pullRequestChecks({
      repo: `o/r`,
      prNumber: 7,
      token: `t`,
      fetchImpl: stub({
        ...PULL,
        "/repos/o/r/commits/abc/check-runs": { check_runs: [done(`gate`, `success`)] },
        "/repos/o/r/commits/abc/status": { status: 403 },
      }),
    })
    expect(verdict).toBeNull()
  })

  it(`required checks unreadable (403 on protection) never read as "none required"`, async () => {
    const verdict = await pullRequestChecks({
      repo: `o/r`,
      prNumber: 7,
      token: `t`,
      fetchImpl: stub({
        ...PULL,
        "/repos/o/r/branches/master/protection/required_status_checks": { status: 403 },
        "/repos/o/r/commits/abc/check-runs": { check_runs: [done(`lint`, `success`)] },
        "/repos/o/r/commits/abc/status": { statuses: [] },
      }),
    })
    expect(verdict).toBeNull()
  })

  it(`pages through the check runs: a required check on page 2 counts`, async () => {
    const seen: string[] = []
    const first = Array.from({ length: 100 }, (_, i) => done(`job-${i}`, `success`))
    const verdict = await pullRequestChecks({
      repo: `o/r`,
      prNumber: 7,
      token: `t`,
      fetchImpl: stub(
        {
          ...PULL,
          ...GATE_RULE,
          "/repos/o/r/commits/abc/check-runs": [
            { total_count: 101, check_runs: first },
            { total_count: 101, check_runs: [done(`gate`, `failure`)] },
          ],
          "/repos/o/r/commits/abc/status": { statuses: [] },
        },
        seen
      ),
    })
    expect(verdict).toEqual({ state: `red`, names: [`gate`], sha: `abc` })
    expect(seen.filter((url) => url.includes(`check-runs`))).toHaveLength(2)
  })
})

describe(`pullRequestHeadSha`, () => {
  it(`reads the head; unreadable = null`, async () => {
    expect(await pullRequestHeadSha({ repo: `o/r`, prNumber: 7, token: `t`, fetchImpl: stub(PULL) })).toBe(`abc`)
    expect(await pullRequestHeadSha({ repo: `o/r`, prNumber: 8, token: `t`, fetchImpl: stub(PULL) })).toBeNull()
  })
})

describe(`isRequiredCheckRefusal`, () => {
  it(`matches GitHub's ruleset wording`, () => {
    expect(isRequiredCheckRefusal(`Required status check "gate" is expected.`)).toBe(true)
    expect(isRequiredCheckRefusal(`Squash merges are not allowed`)).toBe(false)
  })
})
