import { describe, expect, it, vi } from "vitest"

vi.mock(`@/lib/collections`, () => ({
  issueCollection: {},
  messageCollection: {},
  pullRequestCollection: {},
  sessionCollection: {},
  sessionIssueCollection: {},
}))

import { holdTitles, titleShapesReady, unsettledTitleRoots } from "@/hooks/use-session-tree"

// EXP-1316: a thread title never flips from the request text to
// `Implement EXP-12`: it falls back past its issues only once the issue
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

  it(`settles agent, action and stored-title rows; holds issue-linked rows until the issues land`, () => {
    const session = (id: string, over: Partial<{ kind: string; actionName: string | null; title: string | null; parentId: string | null }> = {}) => ({
      id,
      parentId: over.parentId ?? null,
      kind: over.kind ?? `thread`,
      actionName: over.actionName ?? null,
      title: over.title ?? null,
    })
    const sessions = [
      session(`agent`, { kind: `agent` }),
      session(`action`, { actionName: `Deploy` }),
      session(`linked`, { title: `Cold start` }),
      session(`named`, { title: `Cold start` }),
      session(`bare`),
      session(`child`, { parentId: `named` }),
    ]
    const links = [{ sessionId: `linked` }]
    const all = { sessions, sessionIssues: links }
    // Everything in: nothing held.
    expect(
      unsettledTitleRoots({ ...all, linksReady: true, issuesReady: true, messagesReady: true })
    ).toEqual(new Set())
    // Issues pending: only the linked row waits.
    expect(
      unsettledTitleRoots({ ...all, linksReady: true, issuesReady: false, messagesReady: true })
    ).toEqual(new Set([`linked`]))
    // Messages pending: only a root with no stored title waits.
    expect(
      unsettledTitleRoots({ ...all, linksReady: true, issuesReady: true, messagesReady: false })
    ).toEqual(new Set([`bare`]))
    // Links unknown: any root that could still gain an issue waits; agents
    // and action runs never do, children never list.
    expect(
      unsettledTitleRoots({
        ...all,
        sessionIssues: [],
        linksReady: false,
        issuesReady: false,
        messagesReady: true,
      })
    ).toEqual(new Set([`linked`, `named`, `bare`]))
  })
})
