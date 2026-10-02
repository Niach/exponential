import { describe, expect, it } from "vitest"
import { stackChain } from "./pr-stack"

// EXP-897: the PR stack edge the "Related work" badge draws.

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

const ids = (rows: { id: string }[]) => rows.map((row) => row.id)

describe(`stackChain`, () => {
  it(`numbers a member from the bottom of the chain`, () => {
    expect(ids(stackChain(middle, chain))).toEqual([`bottom`, `middle`, `top`])
    expect(ids(stackChain(bottom, chain))).toEqual([`bottom`, `middle`, `top`])
    expect(ids(stackChain(top, chain))).toEqual([`bottom`, `middle`, `top`])
  })

  it(`stops at a base nobody in the list owns`, () => {
    // `main` is the repository's default branch, owned by no issue.
    expect(ids(stackChain(bottom, [bottom]))).toEqual([`bottom`])
    // An empty branch is never an edge either.
    const blank = member(`blank`, ``, ``)
    expect(ids(stackChain(blank, [blank, bottom]))).toEqual([`blank`])
    expect(ids(stackChain(bottom, [bottom, middle]))).toEqual([
      `bottom`,
      `middle`,
    ])
  })

  it(`breaks a cycle where it first appears`, () => {
    const a = member(`a`, `exp/A`, `exp/B`)
    const b = member(`b`, `exp/B`, `exp/A`)
    expect(ids(stackChain(a, [a, b]))).toEqual([`b`, `a`])
  })

  it(`takes the first fork by identifier`, () => {
    const left = member(`left`, `exp/LEFT`, `exp/BOTTOM`)
    const right = member(`right`, `exp/RIGHT`, `exp/BOTTOM`)
    expect(ids(stackChain(bottom, [right, bottom, left]))).toEqual([
      `bottom`,
      `left`,
    ])
  })
})
