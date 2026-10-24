import { describe, expect, it, vi } from "vitest"

vi.mock(`@/lib/collections`, () => ({
  issueCollection: {},
  messageCollection: {},
  pullRequestCollection: {},
  sessionCollection: {},
  sessionIssueCollection: {},
}))

import {
  holdTitles,
  sessionsAwaitingConversation,
  titleShapesReady,
  unsettledTitleRoots,
} from "@/hooks/use-session-tree"

// EXP-1316: a thread title never flips from the request text to
// its issue label: it falls back past its issues only once the issue
// shapes reported ready, and lists hold their row titles until then.
describe(`thread title readiness`, () => {
  const ready = { linksReady: true, issueCount: 1, issuesReady: true, messagesReady: true }

  it(`waits for the links, the linked issues and the messages`, () => {
    expect(titleShapesReady(ready)).toBe(true)
    expect(titleShapesReady({ ...ready, linksReady: false })).toBe(false)
    expect(titleShapesReady({ ...ready, issuesReady: false })).toBe(false)
    expect(titleShapesReady({ ...ready, messagesReady: false })).toBe(false)
  })

  it(`does not wait on issues when none are linked`, () => {
    expect(titleShapesReady({ ...ready, issueCount: 0, issuesReady: false })).toBe(true)
  })

  it(`holds only the rows whose ladder still waits on a shape`, () => {
    const row = (sessionId: string, title: string) => ({
      sessionId,
      teamId: `t`,
      depth: 0,
      parentId: null,
      rootId: sessionId,
      unread: false,
      title,
      private: false,
      deviceId: null,
      busy: true,
      needsYou: false,
      activityAt: `2026-10-14T10:00:00.000Z`,
    })
    const groups = [
      {
        key: `running` as const,
        title: `Running`,
        rows: [row(`a`, `Cold start is at 1.4s on the Pixel 6a`), row(`b`, `Run Deploy`)],
      },
    ]
    const held = holdTitles(groups, new Set([`a`]))
    expect(held[0]!.rows[0]!.title).toBe(``)
    expect(held[0]!.rows[1]!.title).toBe(`Run Deploy`)
    expect(holdTitles(groups, new Set())).toBe(groups)
  })

  it(`follows the title rungs: a title settles, then own issues, then the first prompt`, () => {
    const session = (id: string, over: Partial<{ title: string | null; rootId: string }> = {}) => ({
      id,
      rootId: over.rootId ?? id,
      title: over.title ?? null,
    })
    const sessions = [
      session(`named`, { title: `Cold start` }),
      session(`linked`),
      session(`bare`),
      session(`assigned`, { rootId: `linked` }),
      session(`child`, { rootId: `linked` }),
    ]
    // NEW-1: a row sits on the session holding the issue (root or child).
    const links = [{ sessionId: `linked` }, { sessionId: `assigned` }]
    const all = { sessions, sessionIssues: links }
    // Everything in: nothing held.
    expect(
      unsettledTitleRoots({ ...all, linksReady: true, issuesReady: true, messagesReady: true })
    ).toEqual(new Set())
    // Issues pending: only sessions holding issues wait.
    expect(
      unsettledTitleRoots({ ...all, linksReady: true, issuesReady: false, messagesReady: true })
    ).toEqual(new Set([`linked`, `assigned`]))
    // Messages pending: only an untitled session with no issue.
    expect(
      unsettledTitleRoots({ ...all, linksReady: true, issuesReady: true, messagesReady: false })
    ).toEqual(new Set([`bare`, `child`]))
    // Links unknown: every untitled session could still gain an issue.
    expect(
      unsettledTitleRoots({
        ...all,
        sessionIssues: [],
        linksReady: false,
        issuesReady: false,
        messagesReady: true,
      })
    ).toEqual(new Set([`linked`, `bare`, `assigned`, `child`]))
  })

  // NEW-8: a brand-new session's `conversations` row lags its `sessions`
  // row; its messages query is disabled (ready, empty) until it lands, so
  // the row waits on it instead of flashing "Untitled".
  it(`holds a session whose conversation has not synced yet`, () => {
    const conversations = new Map([
      [`c-old`, `old`],
      [`c-dm`, `other`],
    ])
    expect(sessionsAwaitingConversation(conversations, [`old`, `new`])).toEqual(new Set([`new`]))
    expect(sessionsAwaitingConversation(conversations, [`old`])).toEqual(new Set())
    const sessions = [
      { id: `old`, rootId: `old`, title: null },
      { id: `new`, rootId: `new`, title: null },
      { id: `named`, rootId: `named`, title: `Cold start` },
      { id: `linked`, rootId: `linked`, title: null },
    ]
    const ready = { linksReady: true, issuesReady: true, messagesReady: true }
    const sessionIssues = [{ sessionId: `linked` }]
    // `old` has its conversation and no messages: ready (falls through the
    // rungs); a stored title or own issues never wait on the messages.
    expect(
      unsettledTitleRoots({
        sessions,
        sessionIssues,
        ...ready,
        awaitingConversation: new Set([`new`, `named`, `linked`]),
      })
    ).toEqual(new Set([`new`]))
    expect(unsettledTitleRoots({ sessions, sessionIssues, ...ready })).toEqual(new Set())
  })
})
