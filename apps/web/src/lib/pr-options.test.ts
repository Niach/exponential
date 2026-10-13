import { describe, expect, it } from "vitest"
import { buildPrOptions, findPrOption } from "@/lib/pr-options"

const pull = (
  id: string,
  number: number,
  extra: Partial<{ teamId: string; state: string; title: string | null }> = {}
) => ({ id, number, teamId: `t1`, state: `open`, title: null, ...extra })

const identifiers = new Map([
  [`i1`, `EXP-1`],
  [`i2`, `EXP-2`],
])

describe(`buildPrOptions`, () => {
  it(`lists the team's open pull requests, newest number first`, () => {
    const options = buildPrOptions(
      [
        pull(`p1`, 7),
        pull(`p2`, 9),
        pull(`p3`, 8, { state: `merged` }),
        pull(`p4`, 10, { teamId: `t2` }),
      ],
      [],
      identifiers,
      `t1`
    )
    expect(options.map((option) => option.pullRequestId)).toEqual([`p2`, `p1`])
  })

  it(`labels a pull request by the issues it closes, else its title`, () => {
    const options = buildPrOptions(
      [pull(`p1`, 7), pull(`p2`, 9, { title: `Bump deps` }), pull(`p3`, 3)],
      [
        { pullRequestId: `p1`, issueId: `i2` },
        { pullRequestId: `p1`, issueId: `i1` },
      ],
      identifiers,
      `t1`
    )
    expect(options.map((option) => option.label)).toEqual([
      `#9 · Bump deps`,
      `#7 · EXP-1, EXP-2`,
      `#3`,
    ])
    expect(options[1]!.linkedIssueIds).toEqual([`i1`, `i2`])
  })

  it(`finds an option by its pull request id or any linked issue`, () => {
    const options = buildPrOptions(
      [pull(`p1`, 7)],
      [{ pullRequestId: `p1`, issueId: `i2` }],
      identifiers,
      `t1`
    )
    expect(findPrOption(options, `p1`)?.pullRequestId).toBe(`p1`)
    expect(findPrOption(options, `i2`)?.pullRequestId).toBe(`p1`)
    expect(findPrOption(options, `nope`)).toBeNull()
  })
})
