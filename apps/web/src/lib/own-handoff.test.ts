import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/message-delivery.json"
import { MESSAGE_DELIVERY_HANDOFF_MAX_MS } from "@exp/domain-contract/message-delivery"
import { sessionOwnHandoff } from "./own-handoff"

// #FEED-237 (message-delivery.json HANDOFF): the web view's wrapper replays
// the fixture with the echoes as feed items (main lane only).
describe(`sessionOwnHandoff`, () => {
  for (const c of fixture.handoffCases) {
    it(`handoffCases: ${c.name}`, () => {
      const result = sessionOwnHandoff({
        viewerId: c.input.viewerId,
        now: Date.parse(c.input.now),
        feedEmpty: c.input.feedEmpty,
        rows: c.input.rows,
        feed: c.input.echoes.map((echo) => ({ kind: `user_message`, text: echo.text, at: echo.at ?? undefined })),
      })
      expect(result.order).toEqual(c.expected)
      expect([...result.ids]).toEqual(c.expected)
    })
  }

  const row = {
    id: `m1`,
    authorKind: `user`,
    authorId: `u1`,
    body: `ship it`,
    createdAt: `2026-11-13T10:00:00.000Z`,
    consumedAt: `2026-11-13T10:00:05.000Z`,
  }
  const now = Date.parse(`2026-11-13T10:00:10.000Z`)

  it(`a held row expires handoffMaxMs after its consumedAt`, () => {
    const result = sessionOwnHandoff({ viewerId: `u1`, now, feedEmpty: false, rows: [row], feed: [] })
    expect(result.expiresAt).toBe(Date.parse(row.consumedAt) + MESSAGE_DELIVERY_HANDOFF_MAX_MS)
  })

  it(`a subagent's user_message is no echo; no viewer holds nothing`, () => {
    const sub = [{ kind: `user_message`, text: `ship it`, subagentId: `a1` }]
    expect(sessionOwnHandoff({ viewerId: `u1`, now, feedEmpty: false, rows: [row], feed: sub }).order).toEqual([`m1`])
    expect(sessionOwnHandoff({ viewerId: null, now, feedEmpty: false, rows: [row], feed: [] }).order).toEqual([])
  })
})
