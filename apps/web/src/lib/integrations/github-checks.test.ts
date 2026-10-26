import { describe, expect, it } from "vitest"
import {
  evaluateChecks,
  isRequiredCheckRefusal,
  pullRequestChecks,
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

function stub(routes: Record<string, unknown>): GitHubFetch {
  return async (url) => {
    const path = url.replace(`https://api.github.com`, ``).replace(/\?.*$/, ``)
    const body = routes[path]
    return {
      ok: body !== undefined,
      status: body !== undefined ? 200 : 404,
      text: async () => JSON.stringify(body ?? {}),
      json: async () => body,
    }
  }
}

describe(`pullRequestChecks`, () => {
  it(`reads the head's checks against the base's rulesets`, async () => {
    const verdict = await pullRequestChecks({
      repo: `o/r`,
      prNumber: 7,
      token: `t`,
      fetchImpl: stub({
        "/repos/o/r/pulls/7": { head: { sha: `abc` }, base: { ref: `master` } },
        "/repos/o/r/rules/branches/master": [
          { type: `required_status_checks`, parameters: { required_status_checks: [{ context: `gate` }] } },
        ],
        "/repos/o/r/commits/abc/check-runs": { check_runs: [running(`gate`)] },
        "/repos/o/r/commits/abc/status": { statuses: [] },
      }),
    })
    expect(verdict).toEqual({ state: `pending`, names: [`gate`] })
  })

  it(`no readable checks (the App lacks the permission): no verdict`, async () => {
    const verdict = await pullRequestChecks({
      repo: `o/r`,
      prNumber: 7,
      token: `t`,
      fetchImpl: stub({ "/repos/o/r/pulls/7": { head: { sha: `abc` }, base: { ref: `master` } } }),
    })
    expect(verdict).toBeNull()
  })
})

describe(`isRequiredCheckRefusal`, () => {
  it(`matches GitHub's ruleset wording`, () => {
    expect(isRequiredCheckRefusal(`Required status check "gate" is expected.`)).toBe(true)
    expect(isRequiredCheckRefusal(`Squash merges are not allowed`)).toBe(false)
  })
})
