import { describe, expect, it, vi } from "vitest"

vi.mock(`@/lib/collections`, () => ({
  issueCollection: {},
  messageCollection: {},
  pullRequestCollection: {},
  sessionCollection: {},
  sessionIssueCollection: {},
}))

import { holdTitles, titleShapesReady } from "@/hooks/use-session-tree"

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

  it(`holds every row title until ready`, () => {
    const groups = [
      {
        key: `running` as const,
        title: `Running`,
        rows: [
          {
            sessionId: `a`,
            teamId: `t`,
            title: `Cold start is at 1.4s on the Pixel 6a`,
            private: false,
            busy: true,
            needsYou: false,
            activityAt: `2026-10-14T10:00:00.000Z`,
          },
        ],
      },
    ]
    expect(holdTitles(groups, false)[0]!.rows[0]!.title).toBe(``)
    expect(holdTitles(groups, true)).toBe(groups)
  })
})
