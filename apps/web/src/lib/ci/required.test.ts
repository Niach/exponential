import { describe, expect, it } from "vitest"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { requiredChecksState } from "@exp/domain-contract/pr-checks"
import { withRepoRequired } from "@/lib/ci/required"

// #EXP-1617 dogfood guard: our own CI's `ecode …` checks are informational
// on a repo whose required list names only Actions' `gate`: nothing makes
// them required by default, and a red one never holds the merge train.
describe(`our CI's checks stay informational unless a repo names them`, () => {
  it(`an empty ci_config adds no required check`, () => {
    expect(withRepoRequired({}, [`gate`])).toEqual([`gate`])
    expect(withRepoRequired(undefined, [`gate`])).toEqual([`gate`])
    expect(withRepoRequired({ requiredChecks: [] }, [])).toEqual([])
  })

  it(`the dogfood pipeline file asks for no required checks`, () => {
    const file = JSON.parse(readFileSync(join(process.cwd(), `..`, `..`, `.exponential`, `ci.json`), `utf8`)) as { required?: unknown }
    expect(file.required).toBeUndefined()
  })

  it(`a red ecode check never gates a PR whose base requires gate`, () => {
    const rows = [
      { name: `gate`, status: `completed`, conclusion: `success`, required: true },
      { name: `ecode web unit (1)`, status: `completed`, conclusion: `failure`, required: false },
    ]
    expect(requiredChecksState(rows, [`gate`])).toBe(`success`)
    // The protection read failed (no names): the rows' flags decide.
    expect(requiredChecksState(rows, [])).toBe(`success`)
  })
})
