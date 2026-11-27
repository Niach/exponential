import { beforeEach, describe, expect, it, vi } from "vitest"

// REV-22: the same GitHub repo can be registered by several teams
// (unique(team_id, full_name)), so the webhook's branch-identifier fallback
// can find the SAME identifier in more than one team. findIssueIdByBranch
// must resolve only a UNIQUE match and refuse (null) on ambiguity — a
// mis-link writes the wrong team's issue, flips its status, and notifies its
// subscribers. #EXP-2171: the identifier is looked up across every board of
// the registering teams (a board's repository is only its default).

const h = vi.hoisted(() => ({
  // Each db.select()/selectDistinct() call consumes the next result set, in
  // call order: repositories → issues → issue_events (fallback).
  queue: [] as unknown[][],
  calls: 0,
}))

vi.mock(`@/db/connection`, () => {
  const chain = () => {
    h.calls += 1
    const rows = h.queue.shift() ?? []
    const c: Record<string, unknown> = {}
    c.from = () => c
    c.where = () => c
    c.limit = () => Promise.resolve(rows)
    c.then = (
      resolve: (value: unknown) => unknown,
      reject: (reason: unknown) => unknown
    ) => Promise.resolve(rows).then(resolve, reject)
    return c
  }
  return { db: { select: chain, selectDistinct: chain } }
})

import { findIssueIdByBranch } from "@/lib/integrations/pr-sync"

const REPO = `org/app`
const BRANCH = `exp/APP-12`

beforeEach(() => {
  h.queue = []
  h.calls = 0
})

describe(`findIssueIdByBranch`, () => {
  it(`resolves the unique identifier match in the registering team`, async () => {
    h.queue = [[{ teamId: `team-1` }], [{ id: `issue-1` }]]
    await expect(findIssueIdByBranch(REPO, BRANCH)).resolves.toBe(`issue-1`)
    expect(h.calls).toBe(2)
  })

  it(`refuses to link when the identifier matches issues in two teams`, async () => {
    h.queue = [[{ teamId: `team-a` }, { teamId: `team-b` }], [{ id: `issue-a` }, { id: `issue-b` }]]
    await expect(findIssueIdByBranch(REPO, BRANCH)).resolves.toBeNull()
    // Ambiguity is terminal: the moved-identifier fallback must not run and
    // hand back an equally arbitrary pick.
    expect(h.calls).toBe(2)
  })

  it(`returns null for an unregistered repo without further queries`, async () => {
    h.queue = [[]]
    await expect(findIssueIdByBranch(REPO, BRANCH)).resolves.toBeNull()
    expect(h.calls).toBe(1)
  })

  it(`returns null for an unparseable branch without touching the db`, async () => {
    await expect(findIssueIdByBranch(REPO, `main`)).resolves.toBeNull()
    expect(h.calls).toBe(0)
  })

  it(`falls back to a uniquely-resolved retired identifier (board_moved)`, async () => {
    h.queue = [[{ teamId: `team-1` }], [], [{ issueId: `issue-9` }]]
    await expect(findIssueIdByBranch(REPO, BRANCH)).resolves.toBe(`issue-9`)
    expect(h.calls).toBe(3)
  })

  it(`refuses the fallback when retired identifiers collide across teams`, async () => {
    h.queue = [[{ teamId: `team-a` }, { teamId: `team-b` }], [], [{ issueId: `issue-a` }, { issueId: `issue-b` }]]
    await expect(findIssueIdByBranch(REPO, BRANCH)).resolves.toBeNull()
  })
})
