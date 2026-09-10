import { describe, expect, it } from "vitest"
import { searchFromSeed, seedFromSearch } from "@/lib/launch-seed"

// EXP-825: the Agent page's one-shot preselection, both ways across the URL.

const a = `11111111-1111-4111-8111-111111111111`
const b = `22222222-2222-4222-8222-222222222222`

describe(`seedFromSearch`, () => {
  it(`is null when nothing is set`, () => {
    expect(seedFromSearch({})).toBeNull()
    expect(seedFromSearch({ issues: ``, action: `` })).toBeNull()
  })

  it(`parses the csv issue list and drops anything that is not an id`, () => {
    expect(seedFromSearch({ issues: `${a}, nope ,${b}` })).toEqual({
      issueIds: [a, b],
      actionId: undefined,
      deviceId: undefined,
      prIssueId: undefined,
      text: undefined,
      icon: undefined,
    })
    // A list of junk is no seed at all.
    expect(seedFromSearch({ issues: `x,y` })).toBeNull()
  })

  it(`carries action, pr, device, text and icon`, () => {
    expect(
      seedFromSearch({
        action: `builtin:fix-conflicts`,
        pr: a,
        device: `dev-1`,
        text: `Label new issues`,
        icon: `sparkles`,
      })
    ).toEqual({
      issueIds: [],
      actionId: `builtin:fix-conflicts`,
      deviceId: `dev-1`,
      prIssueId: a,
      text: `Label new issues`,
      icon: `sparkles`,
    })
    // A malformed `pr` is dropped, not refused.
    expect(seedFromSearch({ action: `x`, pr: `nope` })?.prIssueId).toBeUndefined()
  })
})

describe(`searchFromSeed`, () => {
  it(`writes only what was picked`, () => {
    expect(searchFromSeed({ issueIds: [a, b] })).toEqual({ issues: `${a},${b}` })
    expect(searchFromSeed({ issueIds: [] })).toEqual({})
    expect(
      searchFromSeed({
        actionId: `builtin:create-action`,
        text: `Nightly triage`,
        icon: `sparkles`,
        deviceId: `dev-1`,
        prIssueId: a,
      })
    ).toEqual({
      action: `builtin:create-action`,
      pr: a,
      device: `dev-1`,
      text: `Nightly triage`,
      icon: `sparkles`,
    })
  })

  it(`round-trips through the URL`, () => {
    const seed = { issueIds: [a], deviceId: `dev-1`, text: `hi` }
    expect(seedFromSearch(searchFromSeed(seed))).toEqual({
      ...seed,
      actionId: undefined,
      prIssueId: undefined,
      icon: undefined,
    })
  })
})
