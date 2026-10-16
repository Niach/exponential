import { describe, expect, test } from "bun:test"
import designTokens from "../../design-tokens/tokens.json" with { type: "json" }
import fixture from "../fixtures/text-shimmer.json" with { type: "json" }
import { TEXT_SHIMMER, textShimmer } from "./text-shimmer"

describe(`EXP-1324 text shimmer`, () => {
  test(`the numbers are packages/design-tokens motion.shimmer`, () => {
    const { $comment: _, lightAlphaFactor: __, ...ours } = TEXT_SHIMMER
    expect(ours).toEqual(designTokens.motion.shimmer)
  })

  test(`fixtures/text-shimmer.json replays`, () => {
    for (const c of fixture.cases) expect(textShimmer(c.color, c.dark), c.name).toEqual(c.expected as never)
  })

  test(`dark lifts the highlight, light fades it`, () => {
    expect(textShimmer(`#000000`, false).highlight).toBe(`#00000033`)
    expect(textShimmer(`#fafafa`, true).highlight).toBe(`#ffffff`)
  })
})
