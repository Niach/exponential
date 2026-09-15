import { describe, expect, it } from "vitest"
import {
  mergeStackBody,
  MERGE_STACK_LABEL,
  MERGE_STACK_TITLE,
  nestPrStacks,
  stackChain,
  stackedOnCaption,
  stackPosition,
  stackPositionLine,
} from "./pr-stack"

// EXP-897 — the PR stack rules. Every `it` name here is mirrored by iOS
// PrStackTests, Android PrStackTest and the desktop `nest_review_entries`
// tests; a change on one side without the others is a cross-client drift.

const member = (id: string, branch: string | null, base: string | null) => ({
  id,
  identifier: id.toUpperCase(),
  branch,
  prBaseBranch: base,
})

/** A three-deep stack: bottom → middle → top. */
const bottom = member(`bottom`, `exp/BOTTOM`, `main`)
const middle = member(`middle`, `exp/MIDDLE`, `exp/BOTTOM`)
const top = member(`top`, `exp/TOP`, `exp/MIDDLE`)
const chain = [top, bottom, middle]

describe(`stackPosition`, () => {
  it(`numbers a member from the bottom of the chain`, () => {
    expect(stackChain(middle, chain).map((row) => row.id)).toEqual([
      `bottom`,
      `middle`,
      `top`,
    ])
    const at = stackPosition(middle, chain)!
    expect(at.position).toBe(2)
    expect(at.size).toBe(3)
    expect(at.below?.id).toBe(`bottom`)
    expect(at.above?.id).toBe(`top`)
    expect(stackPosition(bottom, chain)!.position).toBe(1)
    expect(stackPosition(top, chain)!.position).toBe(3)
  })

  it(`stops at a base nobody in the list owns`, () => {
    // `main` is the repository's default branch, owned by no issue.
    expect(stackPosition(bottom, [bottom])).toBeNull()
    // An empty branch is never an edge either.
    const blank = member(`blank`, ``, ``)
    expect(stackPosition(blank, [blank, bottom])).toBeNull()
    expect(stackChain(bottom, [bottom, middle]).map((row) => row.id)).toEqual([
      `bottom`,
      `middle`,
    ])
  })

  it(`breaks a cycle where it first appears`, () => {
    const a = member(`a`, `exp/A`, `exp/B`)
    const b = member(`b`, `exp/B`, `exp/A`)
    expect(stackChain(a, [a, b]).map((row) => row.id)).toEqual([`b`, `a`])
    expect(stackPosition(a, [a, b])!.size).toBe(2)
  })

  it(`renders the position line and the merge copy`, () => {
    expect(stackPositionLine(2, 3, `ABC-12`)).toBe(`2 of 3 · on top of #ABC-12`)
    expect(stackPositionLine(1, 3, null)).toBe(`1 of 3`)
    expect(stackedOnCaption(`ABC-12`)).toBe(`on top of #ABC-12`)
    expect(MERGE_STACK_LABEL).toBe(`Merge stack`)
    expect(MERGE_STACK_TITLE).toBe(`Merge the whole stack?`)
    expect(mergeStackBody(3)).toBe(`3 pull requests, bottom-up.`)
  })
})

describe(`nestPrStacks`, () => {
  const entry = (issue: ReturnType<typeof member>) => ({ key: issue.id, issue })
  const shape = (rows: ReturnType<typeof nestPrStacks>) =>
    rows.map((row) => `${row.entry.issue.id}@${row.depth}${row.hasChildren ? `+` : ``}`)

  it(`nests an upper entry under the one it is stacked on`, () => {
    expect(shape(nestPrStacks([entry(top), entry(bottom), entry(middle)]))).toEqual(
      [`bottom@0+`, `middle@1+`, `top@2`]
    )
  })

  it(`keeps the caller's root order`, () => {
    const lone = member(`lone`, `exp/LONE`, `main`)
    const other = member(`other`, `exp/OTHER`, null)
    expect(shape(nestPrStacks([entry(other), entry(lone), entry(bottom), entry(middle)]))).toEqual([
      `other@0`,
      `lone@0`,
      `bottom@0+`,
      `middle@1`,
    ])
  })

  it(`breaks a cycle where it first appears`, () => {
    const a = member(`a`, `exp/A`, `exp/B`)
    const b = member(`b`, `exp/B`, `exp/A`)
    expect(shape(nestPrStacks([entry(a), entry(b)]))).toEqual([`a@0+`, `b@1`])
  })
})
