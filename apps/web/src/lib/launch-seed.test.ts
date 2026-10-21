import { describe, expect, it } from "vitest"
import { searchFromSeed, seedFromSearch, seedHasSubject } from "@/lib/launch-seed"

// EXP-825: the Agent page's one-shot preselection, both ways across the URL.
// NEW-2: no `action`/`pr` keys any more (prompts are picked in the `/` menu).

const a = `11111111-1111-4111-8111-111111111111`
const b = `22222222-2222-4222-8222-222222222222`

describe(`seedFromSearch`, () => {
  it(`is null when nothing is set`, () => {
    expect(seedFromSearch({})).toBeNull()
    expect(seedFromSearch({ issues: `` })).toBeNull()
  })

  it(`parses the csv issue list and drops anything that is not an id`, () => {
    expect(seedFromSearch({ issues: `${a}, nope ,${b}` })).toEqual({
      issueIds: [a, b],
      deviceId: undefined,
      text: undefined,
    })
    // A list of junk is no seed at all.
    expect(seedFromSearch({ issues: `x,y` })).toBeNull()
  })

  it(`carries device and text`, () => {
    expect(seedFromSearch({ device: `dev-1`, text: `Label new issues` })).toEqual({
      issueIds: [],
      deviceId: `dev-1`,
      text: `Label new issues`,
    })
  })
})

describe(`searchFromSeed`, () => {
  it(`writes only what was picked`, () => {
    expect(searchFromSeed({ issueIds: [a, b] })).toEqual({ issues: `${a},${b}` })
    expect(searchFromSeed({ issueIds: [] })).toEqual({})
    expect(searchFromSeed({ text: `Nightly triage`, deviceId: `dev-1` })).toEqual({
      device: `dev-1`,
      text: `Nightly triage`,
    })
  })

  it(`round-trips through the URL`, () => {
    const seed = { issueIds: [a], deviceId: `dev-1`, text: `hi` }
    expect(seedFromSearch(searchFromSeed(seed))).toEqual(seed)
  })
})

// EXP-1019: the one predicate that decides dialog vs. the Agent page.
describe(`seedHasSubject`, () => {
  it(`is true for issues`, () => {
    expect(seedHasSubject({ issueIds: [a] })).toBe(true)
  })

  it(`is false for a chat, however it is seeded`, () => {
    expect(seedHasSubject({})).toBe(false)
    expect(seedHasSubject({ issueIds: [] })).toBe(false)
    // A device or a prefilled sentence is not a subject — there is still
    // nothing to "Implement".
    expect(seedHasSubject({ deviceId: `d1`, text: `hi` })).toBe(false)
  })
})
