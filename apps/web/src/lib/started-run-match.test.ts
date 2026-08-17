import { describe, expect, it } from "vitest"
import {
  findStartedRun,
  matchesStartedRun,
  startedRunKeyForIssues,
  type StartedRunCandidate,
  type StartedRunKey,
} from "@/lib/started-run-match"

// EXP-536: every start surface opens the run it just launched, so the row the
// desktop inserts has to be recognizable from the client. The natives mirror
// these rules (`domain/StartedRunMatch.kt`, `Domain/StartedRunMatch.swift`).

const NOW = Date.parse(`2026-07-17T12:00:00Z`)
const CUTOFF = NOW - 120_000

type Row = StartedRunCandidate & { id: string }

function session(over: Partial<Row> = {}): Row {
  return {
    id: `sess-1`,
    issueId: null,
    actionName: null,
    userId: `user-1`,
    startedAt: new Date(NOW - 30_000).toISOString(),
    ...over,
  }
}

const match = (row: Row, key: StartedRunKey) =>
  matchesStartedRun(row, key, `user-1`, CUTOFF)

describe(`startedRunKeyForIssues`, () => {
  it(`is a single run for one id and a batch for two or more`, () => {
    expect(startedRunKeyForIssues([])).toBeNull()
    expect(startedRunKeyForIssues([`a`])).toEqual({ kind: `issue`, issueId: `a` })
    expect(startedRunKeyForIssues([`a`, `b`])).toEqual({ kind: `batch` })
  })
})

describe(`matchesStartedRun`, () => {
  it(`matches a single run on its own issue row`, () => {
    expect(match(session({ issueId: `issue-1` }), { kind: `issue`, issueId: `issue-1` })).toBe(true)
    expect(match(session({ issueId: `issue-2` }), { kind: `issue`, issueId: `issue-1` })).toBe(false)
  })

  it(`matches a batch on the issueless, actionless row`, () => {
    expect(match(session(), { kind: `batch` })).toBe(true)
    // A single-issue run is not the batch we started…
    expect(match(session({ issueId: `issue-1` }), { kind: `batch` })).toBe(false)
    // …and neither is an action run, which also carries no issue.
    expect(match(session({ actionName: `Fix merge conflicts` }), { kind: `batch` })).toBe(false)
  })

  it(`matches an action run on the name snapshot, never the id`, () => {
    const key = { kind: `action`, actionName: `Fix merge conflicts` } as const
    expect(match(session({ actionName: `Fix merge conflicts` }), key)).toBe(true)
    expect(match(session({ actionName: `Create action` }), key)).toBe(false)
  })

  it(`does not mistake an action run on the issue for the issue run`, () => {
    expect(
      match(session({ issueId: `issue-1`, actionName: `Fix merge conflicts` }), {
        kind: `issue`,
        issueId: `issue-1`,
      })
    ).toBe(false)
  })

  it(`ignores teammates' runs and rows older than the skew window`, () => {
    const key = { kind: `issue`, issueId: `issue-1` } as const
    expect(match(session({ issueId: `issue-1`, userId: `user-2` }), key)).toBe(false)
    expect(
      match(
        session({ issueId: `issue-1`, startedAt: new Date(NOW - 3_600_000).toISOString() }),
        key
      )
    ).toBe(false)
  })

  it(`fails closed on an unparseable startedAt`, () => {
    expect(match(session({ issueId: `issue-1`, startedAt: `nonsense` }), {
      kind: `issue`,
      issueId: `issue-1`,
    })).toBe(false)
  })
})

describe(`findStartedRun`, () => {
  it(`picks the matching row out of the collection`, () => {
    const rows = [
      session({ id: `other`, issueId: `issue-9` }),
      session({ id: `mine`, issueId: `issue-1` }),
    ]
    expect(
      findStartedRun(rows, { kind: `issue`, issueId: `issue-1` }, `user-1`, CUTOFF)?.id
    ).toBe(`mine`)
    expect(
      findStartedRun(rows, { kind: `batch` }, `user-1`, CUTOFF)
    ).toBeUndefined()
  })
})
