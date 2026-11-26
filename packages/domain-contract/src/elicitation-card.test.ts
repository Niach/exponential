// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/elicitation-card.json" with { type: "json" }
import {
  elicitationAction,
  elicitationCard,
  elicitationContent,
  elicitationMissing,
  elicitationUrl,
  type ElicitationRequest,
  type ElicitationSchema,
} from "./elicitation-card"

describe(`elicitation card (fixtures/elicitation-card.json)`, () => {
  test.each(fixture.cardCases)(`card: $name`, (c) => {
    expect(elicitationCard(c.request as ElicitationRequest)).toEqual(c.expected as never)
  })
  test.each(fixture.urlCases)(`url: $url`, (c) => {
    expect(elicitationUrl(c.url)).toEqual(c.expected as never)
  })
  test.each(fixture.missingCases)(`missing: $name`, (c) => {
    expect(elicitationMissing(c, c.surfaceData as Record<string, unknown> | null)).toEqual(c.expected)
  })
  test.each(fixture.actionCases)(`action: $name`, (c) => {
    expect(elicitationAction(c.card)).toBe(c.expected as never)
  })
  test.each(fixture.contentCases)(`content: $name`, (c) => {
    expect(elicitationContent(c.schema as ElicitationSchema, c.surfaceData as Record<string, unknown> | null)).toEqual(
      c.expected as never
    )
  })
  test(`the card's icons are concepts every platform knows`, async () => {
    const icons = (await import(`../../icons/icons.json`, { with: { type: `json` } })).default as {
      icons?: Record<string, unknown>
    }
    const names = JSON.stringify(icons)
    for (const icon of Object.values(fixture.icons)) expect(names).toContain(`"${icon}"`)
  })
})
