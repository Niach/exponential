// EXP-1459: the locked state (fixtures/sealed-content.json `placeholder`),
// replayed through openMessage, buildSessionThread and chatPreview.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/sealed-content.json" with { type: "json" }
import thread from "../fixtures/session-thread.json" with { type: "json" }
import { chatPreview } from "./chats"
import { lockedReason, openMessage, openTitle, SEALED_PLACEHOLDER } from "./sealed-content"
import { buildSessionThread, type TreeMessage } from "./session-tree"

const hex = (text: string) => new Uint8Array(Buffer.from(text, `hex`))
const keysOf = (keys: Record<string, string>) => new Map(Object.entries(keys).map(([e, s]) => [Number(e), hex(s)]))
const ph = fixture.placeholder

describe(`placeholder`, () => {
  test(`every non-open status maps to a reason with copy`, () => {
    for (const status of [`noKey`, `unsupported`, `malformed`, `refused`] as const) {
      expect(SEALED_PLACEHOLDER.copy[lockedReason(status)!]).toBeString()
    }
    expect(lockedReason(`plain`)).toBeNull()
    expect(lockedReason(`opened`)).toBeNull()
  })

  for (const c of ph.messageCases) {
    test(`message: ${c.name}`, () => {
      const opened = openMessage(c.message as { body: string; card: Record<string, unknown> | null }, keysOf(c.keys))
      expect({ locked: opened.locked, body: opened.body, card: opened.card }).toEqual(c.expected as never)
    })
  }

  test(`sealed title opens, else null`, () => {
    expect(openTitle(fixture.body.envelope, keysOf({ 1: fixture.tree.secret }))).toBe(fixture.body.text)
    expect(openTitle(fixture.body.envelope, new Map())).toBeNull()
    expect(openTitle(null, new Map())).toBeNull()
  })

  const base = thread.cases[0]!.input
  for (const c of ph.rowCases) {
    test(`row: ${c.name}`, () => {
      const message = {
        sessionId: base.root.id,
        authorId: null,
        body: ``,
        card: null,
        answeredAt: null,
        consumedAt: `2026-10-13T10:00:00.000Z`,
        createdAt: `2026-10-13T10:05:00.000Z`,
        ...c.message,
      } as TreeMessage
      const view = buildSessionThread({ ...(base as never), messages: [message] })
      expect(view.rows.filter((row) => row.id === c.message.id)).toEqual([c.expected as never])
    })
  }

  for (const c of ph.previewCases) {
    test(`preview: ${c.name}`, () => {
      const messages = c.messages.map((m) => ({
        sessionId: `r`,
        authorId: null,
        card: null,
        answeredAt: null,
        consumedAt: null,
        ...m,
      })) as TreeMessage[]
      expect(chatPreview({ rootId: `r`, messages, needsYou: c.needsYou, busy: c.busy })).toEqual(c.expected as never)
    })
  }
})
