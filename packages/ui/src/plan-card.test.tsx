import { describe, expect, it } from "vitest"
import { cardMediaAllowed, closedPicks, questionAnswer, stripPendingUploads, togglePick } from "./plan-card"

// EXP-1280: the rich card's pure helpers (mirrors desktop `plan_card.rs`).
describe(`plan card helpers`, () => {
  it(`a pending upload renders nothing, a resolved picture stays`, () => {
    expect(stripPendingUploads(`Before\n\n![a](/api/attachments/x)\n\n![b](upload:b) after`)).toBe(
      `Before\n\n![a](/api/attachments/x)\n\n after`
    )
    expect(stripPendingUploads(`no pictures`)).toBe(`no pictures`)
  })

  it(`toggles picks: single swaps (again = none), multi keeps option order`, () => {
    expect(togglePick([], `a`, false, [`a`, `b`])).toEqual([`a`])
    expect(togglePick([`a`], `b`, false, [`a`, `b`])).toEqual([`b`])
    expect(togglePick([`a`], `a`, false, [`a`, `b`])).toEqual([])
    expect(togglePick([`b`], `a`, true, [`a`, `b`])).toEqual([`a`, `b`])
    expect(togglePick([`a`, `b`], `a`, true, [`a`, `b`])).toEqual([`b`])
  })

  it(`free text replaces a single pick and joins multi picks`, () => {
    expect(questionAnswer([`a`], `  `, false)).toEqual([`a`])
    expect(questionAnswer([`a`], `other`, false)).toEqual([`other`])
    expect(questionAnswer([`a`], `other`, true)).toEqual([`a`, `other`])
  })

  it(`a closed card reads answers, else an answer naming an option`, () => {
    const options = [{ label: `A` }, { label: `B` }]
    expect(closedPicks(options, `A, B`, [`A`, `B`])).toEqual([`A`, `B`])
    expect(closedPicks(options, `B`, null)).toEqual([`B`])
    expect(closedPicks(options, `free`, null)).toEqual([])
  })

  it(`surface media = own attachments or https`, () => {
    expect(cardMediaAllowed(`/api/attachments/1`)).toBe(true)
    expect(cardMediaAllowed(`https://example.com/a.png`)).toBe(true)
    expect(cardMediaAllowed(`http://example.com/a.png`)).toBe(false)
    expect(cardMediaAllowed(`/api/teams`)).toBe(false)
  })
})
