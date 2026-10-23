import { describe, expect, it } from "vitest"
import {
  conversationSessions,
  keyMessageRows,
  sessionConversationIds,
  toTreeMessage,
  type SyncedMessageRow,
} from "@/lib/session-tree"

// NEW-8 (fixture `message-conversation.json`): a message is keyed through its
// conversation, never through the row's own legacy `sessionId`.
const message = (
  id: string,
  conversationId: string,
  createdAt: string,
  legacySessionId?: string
): SyncedMessageRow =>
  ({
    id,
    conversationId,
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
    ...(legacySessionId ? { sessionId: legacySessionId } : {}),
  }) as SyncedMessageRow

const conversations = conversationSessions([
  { id: `c-a`, sessionId: `s-a` },
  { id: `c-b`, sessionId: `s-b` },
  { id: `c-dm`, sessionId: null },
])

describe(`keyMessageRows`, () => {
  it(`keys each message to its conversation's session, oldest first`, () => {
    const keyed = keyMessageRows(
      [
        message(`m2`, `c-b`, `2026-10-23T10:00:02Z`),
        message(`m1`, `c-a`, `2026-10-23T10:00:01Z`),
      ],
      conversations
    )
    expect(keyed.map((row) => [row.id, row.sessionId])).toEqual([
      [`m1`, `s-a`],
      [`m2`, `s-b`],
    ])
    expect(toTreeMessage(keyed[0]!).sessionId).toBe(`s-a`)
  })

  it(`leaves out dm/channel messages and holds back unsynced conversations`, () => {
    const keyed = keyMessageRows(
      [
        message(`dm`, `c-dm`, `2026-10-23T10:00:01Z`),
        message(`late`, `c-unsynced`, `2026-10-23T10:00:02Z`, `s-a`),
      ],
      conversations
    )
    expect(keyed).toEqual([])
  })

  it(`ignores a legacy sessionId on the row`, () => {
    const keyed = keyMessageRows([message(`m`, `c-b`, `2026-10-23T10:00:01Z`, `s-a`)], conversations)
    expect(keyed[0]!.sessionId).toBe(`s-b`)
  })
})

describe(`sessionConversationIds`, () => {
  it(`resolves sessions to their own conversations, sorted`, () => {
    expect(sessionConversationIds(conversations, [`s-b`, `s-a`, `s-none`])).toEqual([`c-a`, `c-b`])
    expect(sessionConversationIds(conversations, [])).toEqual([])
  })
})
