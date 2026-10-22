// FEED-131 — the command engine both renderers share: `commandScore` is
// cmdk 1.1.1's `command-score` (the numbers below are cmdk's own, taken
// from its dist), `filterCommandItems` the visible order.

import { describe, expect, it } from "bun:test"
import { commandScore, filterCommandItems } from "./command"

describe(`commandScore (cmdk's command-score)`, () => {
  it(`reproduces cmdk's numbers`, () => {
    const cases: [string, string, string[] | undefined, number][] = [
      [`Apple`, `apple`, undefined, 0.9999],
      [`Apple`, `app`, undefined, 0.989901],
      [`Pineapple`, `apple`, undefined, 0.17],
      [`Apple pie tool`, `tool`, undefined, 0.9],
      [`Apple pie tool`, `apt`, undefined, 0.8900199891],
      [`Hammer`, `nail`, [`nail`], 0.9],
      [`Hammer`, `nail`, undefined, 0],
      [`Settings`, `stg`, undefined, 0.028493877878829454],
      [`git-commit`, `gc`, undefined, 0.891],
      [`Open File`, `of`, undefined, 0.8908218089100001],
      [`Open File`, `fo`, undefined, 0],
      [`abc`, `acb`, undefined, 0.1],
      [`Banana`, `nn`, undefined, 0.099],
      [`Create issue`, `create-is`, undefined, 0.9898020099],
      [`x/y_z`, `xyz`, undefined, 0.6400000000000001],
    ]
    for (const [value, query, keywords, score] of cases) expect([value, query, commandScore(value, query, keywords)]).toEqual([value, query, score])
  })

  it(`ranks exact > prefix > word boundary > fuzzy > none`, () => {
    const s = (v: string) => commandScore(v, `rep`)
    expect(s(`rep`)).toBe(1)
    expect(s(`report`)).toBeGreaterThan(s(`bug report`))
    expect(s(`bug report`)).toBeGreaterThan(s(`bug/report`))
    expect(s(`bug/report`)).toBeGreaterThan(s(`prepare`))
    expect(s(`prepare`)).toBeGreaterThan(0)
    expect(s(`board`)).toBe(0)
  })
})

describe(`filterCommandItems`, () => {
  const items = [
    { id: `a`, value: `Apple`, group: `fruit` },
    { id: `b`, value: `Banana`, group: `fruit` },
    { id: `p`, value: `Pineapple`, group: `fruit`, disabled: true },
    { id: `h`, value: `Hammer`, group: `tools`, keywords: [`nail`] },
    { id: `t`, value: `Apple pie tool`, group: `tools` },
    { id: `x`, value: `Settings` },
  ]

  it(`an empty query keeps the registration order, ungrouped first`, () => {
    const r = filterCommandItems(items, ``)
    expect(r.count).toBe(6)
    expect(r.groups).toEqual([
      { group: undefined, ids: [`x`] },
      { group: `fruit`, ids: [`a`, `b`, `p`] },
      { group: `tools`, ids: [`h`, `t`] },
    ])
  })

  it(`a query keeps the matches, by score, groups by their best item`, () => {
    expect(filterCommandItems(items, `apple`).groups).toEqual([
      { group: `fruit`, ids: [`a`, `p`] },
      { group: `tools`, ids: [`t`] },
    ])
    expect(filterCommandItems(items, `tool`).groups).toEqual([{ group: `tools`, ids: [`t`] }])
    expect(filterCommandItems(items, `nail`).groups).toEqual([{ group: `tools`, ids: [`h`] }])
    const pie = filterCommandItems(items, `pie`)
    expect(pie.groups).toEqual([
      { group: `tools`, ids: [`t`] },
      { group: `fruit`, ids: [`p`] },
    ])
  })

  it(`counts the matches; forced items always show but never count`, () => {
    expect(filterCommandItems(items, `zzz`)).toMatchObject({ count: 0, groups: [] })
    const forced = filterCommandItems([...items, { id: `f`, value: `Help`, forceMount: true }], `zzz`)
    expect(forced.count).toBe(0)
    expect(forced.groups).toEqual([{ group: undefined, ids: [`f`] }])
  })

  it(`takes a custom filter`, () => {
    const r = filterCommandItems(items, `an`, (v, q) => (v.toLowerCase().includes(q) ? 1 : 0))
    expect(r.groups).toEqual([{ group: `fruit`, ids: [`b`] }])
  })
})
