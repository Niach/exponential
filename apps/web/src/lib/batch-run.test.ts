import { describe, expect, it } from "vitest"
import {
  BATCH_RUN_FALLBACK,
  batchRunIssues,
  batchRunName,
  isBatchRun,
  type BatchRunIssue,
  type BatchRunSession,
} from "./batch-run"

// EXP-876 — how a batch run is NAMED. Every `it` name here is mirrored by the
// desktop `batch_run_*` tests, iOS `BatchRunTests` and Android
// `BatchRunTest`: a change on one side without the others is cross-client
// drift, not a tweak.

function session(over: Partial<BatchRunSession> = {}): BatchRunSession {
  return {
    issueId: null,
    actionName: null,
    batchIssueIds: null,
    ...over,
  }
}

function issue(over: Partial<BatchRunIssue> & { id: string }): BatchRunIssue {
  return {
    identifier: `EXP-1`,
    title: `An issue`,
    ...over,
  }
}

const first = issue({
  id: `i-1`,
  identifier: `EXP-874`,
  title: `Session list fixes`,
})
const second = issue({
  id: `i-2`,
  identifier: `EXP-876`,
  title: `Batch run names`,
})
const third = issue({
  id: `i-3`,
  identifier: `EXP-889`,
  title: `Diff on the issue page`,
})
const pool = [third, first, second]

describe(`isBatchRun`, () => {
  it(`is an issue-less, action-less row`, () => {
    expect(isBatchRun(session())).toBe(true)
    expect(isBatchRun(session({ issueId: `i-1` }))).toBe(false)
    // A chat run carries the reserved snapshot, an action run its own.
    expect(isBatchRun(session({ actionName: `Chat` }))).toBe(false)
    expect(isBatchRun(session({ actionName: `Nightly triage` }))).toBe(false)
  })
})

describe(`batchRunIssues`, () => {
  it(`keeps the order the run stored`, () => {
    // The composer's order, NOT the pool's — the first issue is what names
    // the row, so a re-ordered pool must never re-name it.
    expect(
      batchRunIssues(session({ batchIssueIds: [`i-2`, `i-1`] }), pool)
    ).toEqual([second, first])
  })

  it(`skips an id whose issue has not synced`, () => {
    expect(
      batchRunIssues(session({ batchIssueIds: [`gone`, `i-1`] }), pool)
    ).toEqual([first])
  })

  it(`is empty without stored ids`, () => {
    // EXP-972: NULL = nothing to name the batch by. The branch-mates fallback
    // is gone — every row from before the column was backfilled once.
    expect(batchRunIssues(session(), pool)).toEqual([])
    expect(batchRunIssues(session({ batchIssueIds: [] }), pool)).toEqual([])
  })

  it(`is empty for every other subject`, () => {
    expect(
      batchRunIssues(
        session({ issueId: `i-1`, batchIssueIds: [`i-1`] }),
        pool
      )
    ).toEqual([])
    expect(
      batchRunIssues(
        session({ actionName: `Chat`, batchIssueIds: [`i-1`] }),
        pool
      )
    ).toEqual([])
  })
})

describe(`batchRunName`, () => {
  it(`names a batch after its covered issues`, () => {
    expect(batchRunName(session({ batchIssueIds: [`i-1`, `i-2`] }), pool)).toEqual(
      { identifier: `EXP-874 +1`, subject: `Session list fixes` }
    )
    expect(
      batchRunName(session({ batchIssueIds: [`i-1`, `i-2`, `i-3`] }), pool)
    ).toEqual({ identifier: `EXP-874 +2`, subject: `Session list fixes` })
  })

  it(`drops the suffix for a batch of one`, () => {
    expect(batchRunName(session({ batchIssueIds: [`i-2`] }), pool)).toEqual({
      identifier: `EXP-876`,
      subject: `Batch run names`,
    })
  })

  it(`counts the issues the run stored, not the ones that synced`, () => {
    // Two of three synced: still "+2". Understating the set would make a
    // three-issue run look like a two-issue one for as long as sync lags.
    expect(
      batchRunName(session({ batchIssueIds: [`i-1`, `gone`, `i-3`] }), pool)
    ).toEqual({ identifier: `EXP-874 +2`, subject: `Session list fixes` })
  })

  it(`falls back to Batch run`, () => {
    // Nothing stored — the old label, unchanged.
    expect(batchRunName(session(), pool)).toEqual({
      identifier: null,
      subject: BATCH_RUN_FALLBACK,
    })
    expect(batchRunName(session({ batchIssueIds: [`gone`] }), pool)).toEqual({
      identifier: null,
      subject: `Batch run`,
    })
    expect(batchRunName(session({ batchIssueIds: [`i-1`] }), [])).toEqual({
      identifier: null,
      subject: `Batch run`,
    })
  })

  it(`names an untitled first issue`, () => {
    const blank = issue({ id: `i-9`, identifier: `EXP-900`, title: `   ` })
    expect(batchRunName(session({ batchIssueIds: [`i-9`] }), [blank])).toEqual({
      identifier: `EXP-900`,
      subject: `Untitled issue`,
    })
  })
})
