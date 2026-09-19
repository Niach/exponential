import { describe, expect, it } from "vitest"
import {
  treeGuideCentre,
  treeGuideIsEmpty,
  treeGuides,
  TREE_BASE,
  TREE_INDENT,
} from "./tree-guides"

// EXP-965: the connector rule, ×4 (desktop `domain::tree_guides`, iOS
// `TreeGuides.swift`, Android `TreeGuides.kt`) — same four vectors.

const shape = (depths: number[]) =>
  treeGuides(depths).map((guide) => [
    guide.elbowAt,
    guide.tee,
    guide.passThrough,
  ])

describe(`treeGuides`, () => {
  it(`draws nothing for a flat list`, () => {
    expect(shape([0, 0, 0])).toEqual([
      [null, false, []],
      [null, false, []],
      [null, false, []],
    ])
    expect(treeGuides([])).toEqual([])
  })

  it(`elbows each link of a chain, never teeing`, () => {
    expect(shape([0, 1, 2])).toEqual([
      [null, false, []],
      [0, false, []],
      [1, false, []],
    ])
  })

  it(`tees every sibling but the last`, () => {
    expect(shape([0, 1, 1, 1])).toEqual([
      [null, false, []],
      [0, true, []],
      [0, true, []],
      [0, false, []],
    ])
  })

  it(`passes an ancestor's line through its grandchildren`, () => {
    // root
    //  ├ a
    //  │  ├ a1
    //  │  └ a2
    //  └ b
    expect(shape([0, 1, 2, 2, 1])).toEqual([
      [null, false, []],
      [0, true, []],
      [1, true, [0]],
      [1, false, [0]],
      [0, false, []],
    ])
  })

  it(`stops the pass-through once the ancestor's subtree ends`, () => {
    // root ▸ a ▸ a1 ▸ a1a , then a2 — the LAST child of the last root has no
    // line above it at any level.
    expect(shape([0, 1, 2, 3, 2])).toEqual([
      [null, false, []],
      [0, false, []],
      [1, true, []],
      [2, false, [1]],
      [1, false, []],
    ])
  })

  it(`treats a deeper row after a shallow one as a new subtree`, () => {
    // Two roots, each with one child: neither child passes anything through.
    expect(shape([0, 1, 0, 1])).toEqual([
      [null, false, []],
      [0, false, []],
      [null, false, []],
      [0, false, []],
    ])
  })
})

describe(`treeGuideCentre`, () => {
  it(`centres each gutter on its level's glyph box`, () => {
    expect(treeGuideCentre(0)).toBe(TREE_BASE + TREE_INDENT / 2)
    expect(treeGuideCentre(1)).toBe(TREE_BASE + TREE_INDENT + TREE_INDENT / 2)
    expect(treeGuideCentre(2)).toBe(TREE_BASE + TREE_INDENT * 2 + TREE_INDENT / 2)
  })
})

describe(`treeGuideIsEmpty`, () => {
  it(`is true only with no elbow and no pass-through`, () => {
    expect(treeGuideIsEmpty(null)).toBe(true)
    expect(treeGuideIsEmpty(undefined)).toBe(true)
    expect(
      treeGuideIsEmpty({ elbowAt: null, tee: false, passThrough: [] })
    ).toBe(true)
    expect(treeGuideIsEmpty({ elbowAt: 0, tee: false, passThrough: [] })).toBe(
      false
    )
    expect(
      treeGuideIsEmpty({ elbowAt: null, tee: false, passThrough: [0] })
    ).toBe(false)
  })
})
