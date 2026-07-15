import { describe, expect, it } from "vitest"
import { gettingStartedCardOrder } from "./getting-started-cards"

describe(`gettingStartedCardOrder`, () => {
  it(`leads with the widget card on public boards`, () => {
    expect(gettingStartedCardOrder(true)).toEqual([`widget`, `coding`, `mcp`])
  })

  it(`leads with the coding card everywhere else`, () => {
    expect(gettingStartedCardOrder(false)).toEqual([`coding`, `widget`, `mcp`])
    expect(gettingStartedCardOrder(undefined)).toEqual([
      `coding`,
      `widget`,
      `mcp`,
    ])
  })
})
