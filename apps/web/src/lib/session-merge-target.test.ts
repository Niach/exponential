import { describe, expect, it } from "vitest"
import type { CodingSession, Issue } from "@/db/schema"
import { resolveSessionMergeTarget } from "@/lib/session-merge-target"

// EXP-1165: a batch run's Merge resolves to the covered issue carrying the
// run's open PR (the stack dialog and "Fix conflicts" live on that path).

const PR = `https://github.com/acme/web/pull/42`

const session = (over: Partial<CodingSession> = {}) =>
  ({
    id: `s1`,
    issueId: null,
    actionName: null,
    batchIssueIds: [`i1`, `i2`],
    prUrl: PR,
    prNumber: 42,
    prState: `open`,
    ...over,
  }) as CodingSession

const issue = (id: string, over: Partial<Issue> = {}) =>
  ({ id, prUrl: PR, prState: `open`, ...over }) as Issue

describe(`resolveSessionMergeTarget`, () => {
  it(`merges an issue run through its issue`, () => {
    const own = issue(`i9`)
    expect(
      resolveSessionMergeTarget(session({ issueId: `i9` }), own, [])
    ).toEqual({ kind: `issue`, issue: own })
  })

  it(`waits for an issue run's issue row`, () => {
    expect(
      resolveSessionMergeTarget(session({ issueId: `i9` }), undefined, [])
    ).toBeUndefined()
  })

  it(`resolves a batch run to the covered issue carrying its open PR`, () => {
    const other = issue(`i1`, { prUrl: `https://github.com/acme/web/pull/7` })
    const carrier = issue(`i2`)
    expect(
      resolveSessionMergeTarget(session(), undefined, [other, carrier])
    ).toEqual({ kind: `issue`, issue: carrier })
  })

  it(`keeps the session target when no covered issue carries the open PR`, () => {
    const s = session()
    expect(
      resolveSessionMergeTarget(s, undefined, [
        issue(`i1`, { prState: `merged` }),
      ])
    ).toEqual({ kind: `session`, session: s })
    expect(resolveSessionMergeTarget(s, undefined, [])).toEqual({
      kind: `session`,
      session: s,
    })
  })

  it(`has no target for a run without a PR`, () => {
    expect(
      resolveSessionMergeTarget(
        session({ prUrl: null, prNumber: null }),
        undefined,
        [issue(`i1`)]
      )
    ).toBeUndefined()
  })
})
