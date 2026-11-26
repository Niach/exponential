// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/elicitation-card.json" with { type: "json" }
import {
  elicitationAction,
  elicitationCard,
  elicitationFeedCard,
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
  describe(`elicitationFeedCard: a live frame draws the posted card`, () => {
    const card = (questionId: string, elicitation = true) => ({
      kind: `question`,
      questionId,
      options: [`Submit`, `Decline`, `Cancel`],
      ...(elicitation ? { elicitation: { server: `acme`, mode: `form`, required: [], url: null } } : {}),
    })
    const messages = [
      { id: `m-plain`, answeredAt: null, card: card(`ask-1#0`, false) },
      { id: `m-form`, answeredAt: null, card: card(`mcp-1#submit`) },
      { id: `m-done`, answeredAt: `2026-11-26T17:00:00.000Z`, card: card(`mcp-2#submit`) },
      { id: `m-text`, answeredAt: null, card: null },
    ]
    test(`a waiting frame of an elicitation card = that card`, () => {
      expect(elicitationFeedCard(messages, `mcp-1#submit`, false)).toBe(`m-form`)
    })
    test(`an answered card stays the card once its frame resolved`, () => {
      expect(elicitationFeedCard(messages, `mcp-2#submit`, true)).toBe(`m-done`)
    })
    test(`a withdrawn frame (resolved, card never answered) stays the frame`, () => {
      expect(elicitationFeedCard(messages, `mcp-1#submit`, true)).toBeNull()
    })
    test(`an engine question without the sidecar, or no card, stays the frame`, () => {
      expect(elicitationFeedCard(messages, `ask-1#0`, false)).toBeNull()
      expect(elicitationFeedCard(messages, `mcp-9#submit`, false)).toBeNull()
    })
  })
})
