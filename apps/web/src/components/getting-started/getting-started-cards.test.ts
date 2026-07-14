import { describe, expect, it } from "vitest"
import { gettingStartedCardOrder } from "./getting-started-cards"

describe(`gettingStartedCardOrder`, () => {
  it(`leads with the widget card on feedback boards`, () => {
    expect(gettingStartedCardOrder(`feedback`)).toEqual([
      `widget`,
      `coding`,
      `mcp`,
    ])
  })

  it(`leads with the coding card everywhere else`, () => {
    expect(gettingStartedCardOrder(`dev`)).toEqual([`coding`, `widget`, `mcp`])
    expect(gettingStartedCardOrder(`tasks`)).toEqual([
      `coding`,
      `widget`,
      `mcp`,
    ])
    expect(gettingStartedCardOrder(undefined)).toEqual([
      `coding`,
      `widget`,
      `mcp`,
    ])
  })
})
