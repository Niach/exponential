import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/message-session.json"
import { keyMessageRows, toTreeMessage, type SyncedMessageRow } from "@/lib/session-tree"

// #EXP-1730 (one-tree model): a message names its session itself
// (`messages.session_id`, trigger-kept from its conversation); the web no
// longer resolves it through the `conversations` shape.
const message = (id: string, sessionId: string | null, createdAt: string): SyncedMessageRow =>
  ({
    id,
    conversationId: `c-${sessionId ?? `dm`}`,
    sessionId,
    rootId: `root`,
    teamId: `team`,
    authorKind: `user`,
    authorId: null,
    authorSessionId: null,
    body: id,
    card: null,
    answeredAt: null,
    consumedAt: null,
    createdAt: new Date(createdAt),
    updatedAt: new Date(createdAt),
  }) as SyncedMessageRow

describe(`keyMessageRows`, () => {
  it(`keys each message to its own session, oldest first`, () => {
    const keyed = keyMessageRows([
      message(`m2`, `s-b`, `2026-10-23T10:00:02Z`),
      message(`m1`, `s-a`, `2026-10-23T10:00:01Z`),
    ])
    expect(keyed.map((row) => [row.id, row.sessionId])).toEqual([
      [`m1`, `s-a`],
      [`m2`, `s-b`],
    ])
    expect(toTreeMessage(keyed[0]!).sessionId).toBe(`s-a`)
  })

  it(`leaves out dm/channel messages (no session)`, () => {
    const keyed = keyMessageRows([
      message(`dm`, null, `2026-10-23T10:00:01Z`),
      message(`m1`, `s-a`, `2026-10-23T10:00:02Z`),
    ])
    expect(keyed.map((row) => row.id)).toEqual([`m1`])
  })

  it(`breaks a createdAt tie by id and never mutates its input`, () => {
    const rows = [
      message(`b`, `s-a`, `2026-10-23T10:00:01Z`),
      message(`a`, `s-a`, `2026-10-23T10:00:01Z`),
    ]
    expect(keyMessageRows(rows).map((row) => row.id)).toEqual([`a`, `b`])
    expect(rows.map((row) => row.id)).toEqual([`b`, `a`])
  })
})

// The ×4 rule (fixtures/message-session.json), replayed through the web's keying.
describe(`message-session.json`, () => {
  for (const entry of fixture.cases) {
    it(entry.name, () => {
      const rows = entry.messages.map((m, i) => ({ ...message(m.id, m.sessionId ?? null, `2026-10-23T10:00:0${i}Z`) }))
      expect(keyMessageRows(rows).map(({ id, sessionId }) => ({ id, sessionId }))).toEqual(entry.expected)
    })
  }
})
