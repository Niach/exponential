import { describe, expect, it, vi } from "vitest"

// #EXP-1815: the page's own thread (notification-delivery.json `viewing`).
vi.mock(`@/lib/collections`, () => {
  const sessions = new Map([[`child`, { id: `child`, rootId: `root` }]])
  const issues = [{ id: `i1`, identifier: `EXP-7`, boardId: `b1` }]
  return {
    sessionCollection: { get: (id: string) => sessions.get(id) },
    issueCollection: { toArray: issues, get: (id: string) => issues.find((issue) => issue.id === id) },
    boardCollection: { get: () => undefined },
    teamCollection: { get: () => undefined },
    notificationCollection: {},
  }
})

import { rowThread, viewingThread } from "./use-notification-toasts"

describe(`viewingThread`, () => {
  it(`the Inbox covers every thread`, () => expect(viewingThread(`/t/acme/inbox`)).toBe(`inbox`))
  it(`a session page = its chat`, () => expect(viewingThread(`/t/acme/sessions/child`)).toBe(`session:root`))
  it(`an unsynced session = itself`, () => expect(viewingThread(`/t/acme/sessions/x`)).toBe(`session:x`))
  it(`an issue page by board`, () => expect(viewingThread(`/t/acme/boards/feedback/issues/exp-7`)).toBe(`issue:i1`))
  it(`an issue by identifier`, () => expect(viewingThread(`/t/acme/issues/EXP-7`)).toBe(`issue:i1`))
  it(`a board has no thread`, () => expect(viewingThread(`/t/acme/boards/feedback`)).toBeNull())
})

describe(`rowThread`, () => {
  it(`a session row stacks under its root`, () =>
    expect(rowThread({ type: `agent_message`, issueId: null, sessionId: `child`, deviceId: null } as never)).toBe(`session:root`))
})
