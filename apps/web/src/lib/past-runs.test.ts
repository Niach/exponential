import { describe, expect, it } from "vitest"
import {
  isLiveRun,
  isLiveRunStatus,
  LIVE_RUN_LABEL,
  pastRunByline,
  pastRunEndedAt,
  pastRunIdentifier,
  pastRunTitle,
  runHasEnded,
  runIsLive,
  runIsStaleEnd,
  selectIssueRuns,
  selectPastRuns,
  PAST_RUN_CAP,
} from "./past-runs"
import type { CodingSession, Issue } from "@/db/schema"

// EXP-746 — the "Recent" section's rules (EXP-886: was "Past"), plus the
// issue's runs behind the Run/Runs label and the run switcher. Every `it`
// name here is mirrored by iOS PastRunsTests, Android AgentRowsTest and the
// desktop `own_ended_runs_*` / `issue_runs_*` tests; a change on one side
// without the others is a cross-client drift, not a tweak.

type PastRun = Pick<
  CodingSession,
  | `id`
  | `status`
  | `startedReason`
  | `userId`
  | `teamId`
  | `issueId`
  | `actionName`
  | `batchIssueIds`
  | `branch`
  | `agent`
  | `endedBy`
  | `endedAt`
  | `updatedAt`
>

const at = (iso: string) => new Date(iso)

function run(over: Partial<PastRun> = {}): PastRun {
  return {
    id: `run-1`,
    status: `ended`,
    startedReason: null,
    userId: `me`,
    teamId: `team-1`,
    issueId: null,
    actionName: null,
    batchIssueIds: null,
    branch: null,
    agent: `claude`,
    endedBy: `agent`,
    endedAt: at(`2026-09-01T10:00:00Z`),
    updatedAt: at(`2026-09-01T09:00:00Z`),
    ...over,
  }
}

// EXP-888: the ONE live/ended predicate, mirrored by desktop
// `run_rows::run_has_ended`, iOS `PastRuns.hasEnded` and Android
// `runHasEnded`. Same `it` name on all four.
describe(`runHasEnded`, () => {
  it(`a sweep end is not an end`, () => {
    expect(runHasEnded(run({ status: `running`, endedBy: null }))).toBe(false)
    expect(runHasEnded(run({ status: `in_review`, endedBy: null }))).toBe(false)
    expect(runHasEnded(run({ status: `ended`, endedBy: `agent` }))).toBe(true)
    expect(runHasEnded(run({ status: `ended`, endedBy: `user` }))).toBe(true)
    expect(runHasEnded(run({ status: `ended`, endedBy: `merge` }))).toBe(true)
    // The staleness sweep's end: the host ignores it and heartbeats the row
    // back to `running`, so the run is LIVE, not past.
    expect(runHasEnded(run({ status: `ended`, endedBy: `stale` }))).toBe(false)
    // A legacy row that stamped no reason still reads as ended.
    expect(runHasEnded(run({ status: `ended`, endedBy: null }))).toBe(true)

    expect(runIsStaleEnd(run({ status: `ended`, endedBy: `stale` }))).toBe(true)
    // `stale` only ever rides an `ended` row; on a live one it means nothing.
    expect(runIsStaleEnd(run({ status: `running`, endedBy: `stale` }))).toBe(
      false
    )
    expect(runIsLive(run({ status: `ended`, endedBy: `stale` }))).toBe(true)
    expect(runIsLive(run({ status: `ended`, endedBy: `agent` }))).toBe(false)

    // The live-badge/ordering twin says the same about a swept row.
    expect(isLiveRun(run({ status: `ended`, endedBy: `stale` }))).toBe(true)
    expect(isLiveRun(run({ status: `ended`, endedBy: `agent` }))).toBe(false)
    expect(isLiveRun(run({ status: `running`, endedBy: null }))).toBe(true)
    expect(isLiveRun(run({ status: `in_review`, endedBy: null }))).toBe(true)
  })
})

describe(`selectPastRuns`, () => {
  it(`own person-started ended runs only`, () => {
    const mine = run({ id: `mine` })
    const rows = [
      mine,
      run({ id: `still-running`, status: `running` }),
      run({ id: `in-review`, status: `in_review` }),
      run({ id: `someone-else`, userId: `you` }),
      run({ id: `other-team`, teamId: `team-2` }),
    ]
    expect(selectPastRuns(rows, `me`, `team-1`).map((r) => r.id)).toEqual([
      `mine`,
    ])
    // Without a signed-in user or an active team there is nothing to list.
    expect(selectPastRuns(rows, undefined, `team-1`)).toEqual([])
    expect(selectPastRuns(rows, `me`, undefined)).toEqual([])
  })

  it(`a swept run stays out of Past`, () => {
    // EXP-888: the sweep ends a silent row with `ended_by = stale`; its host
    // revives it on the next heartbeat. Listing it under Recent would grey
    // the composer out and offer Resume on a run that is still alive.
    const rows = [
      run({ id: `really-ended`, endedBy: `agent` }),
      run({ id: `swept`, endedBy: `stale` }),
    ]
    expect(selectPastRuns(rows, `me`, `team-1`).map((r) => r.id)).toEqual([
      `really-ended`,
    ])
  })

  it(`a scheduled run never lists under Past`, () => {
    const rows = [
      run({ id: `person` }),
      run({ id: `scheduled`, startedReason: `schedule` }),
      run({ id: `evented`, startedReason: `event` }),
      run({ id: `child`, startedReason: `agent` }),
    ]
    // Automated runs belong to the Automations tab's "Recent automated runs"
    // (EXP-676) — the only finished-runs list keyed on started_reason.
    expect(selectPastRuns(rows, `me`, `team-1`).map((r) => r.id)).toEqual([
      `person`,
    ])
  })

  it(`newest first by ended_at, falling back to updated_at`, () => {
    const rows = [
      run({ id: `old`, endedAt: at(`2026-09-01T08:00:00Z`) }),
      // Ended by a server too old to stamp ended_at — the heartbeat stamp is
      // the fallback key, and it is the newest row here.
      run({
        id: `no-end-stamp`,
        endedAt: null,
        updatedAt: at(`2026-09-01T12:00:00Z`),
      }),
      run({ id: `newer`, endedAt: at(`2026-09-01T11:00:00Z`) }),
    ]
    expect(selectPastRuns(rows, `me`, `team-1`).map((r) => r.id)).toEqual([
      `no-end-stamp`,
      `newer`,
      `old`,
    ])
    expect(pastRunEndedAt(rows[1])).toBe(at(`2026-09-01T12:00:00Z`).getTime())
  })

  it(`the list is capped at twenty`, () => {
    const rows = Array.from({ length: 30 }, (_, i) =>
      run({
        id: `run-${i}`,
        endedAt: at(`2026-09-01T00:00:00Z`),
        updatedAt: new Date(at(`2026-09-01T00:00:00Z`).getTime() + i * 1000),
      })
    )
    expect(PAST_RUN_CAP).toBe(20)
    const past = selectPastRuns(rows, `me`, `team-1`)
    expect(past).toHaveLength(20)
    expect(selectPastRuns(rows, `me`, `team-1`, 5)).toHaveLength(5)
  })
})

describe(`selectIssueRuns`, () => {
  it(`lists only the caller's own runs of that issue`, () => {
    const rows = [
      run({ id: `mine`, issueId: `issue-1` }),
      // Automated runs of the issue are its runs too.
      run({ id: `scheduled`, issueId: `issue-1`, startedReason: `schedule` }),
      // So are the live ones — the switcher offers them beside the ended.
      run({
        id: `running`,
        issueId: `issue-1`,
        status: `running`,
        endedAt: null,
        updatedAt: at(`2026-09-01T12:00:00Z`),
      }),
      run({
        id: `in-review`,
        issueId: `issue-1`,
        status: `in_review`,
        endedAt: null,
        updatedAt: at(`2026-09-01T11:00:00Z`),
      }),
      run({ id: `someone-else`, issueId: `issue-1`, userId: `you` }),
      run({ id: `other-issue`, issueId: `issue-2` }),
      run({ id: `batch`, issueId: null }),
    ]
    expect(selectIssueRuns(rows, `me`, `issue-1`).map((r) => r.id)).toEqual([
      `running`,
      `in-review`,
      `mine`,
      `scheduled`,
    ])
    expect(selectIssueRuns(rows, undefined, `issue-1`)).toEqual([])
    expect(selectIssueRuns(rows, `me`, undefined)).toEqual([])
    expect(isLiveRunStatus(`running`)).toBe(true)
    expect(isLiveRunStatus(`in_review`)).toBe(true)
    expect(isLiveRunStatus(`ended`)).toBe(false)
  })

  it(`puts live runs first, then newest end first, uncapped`, () => {
    const rows = Array.from({ length: 30 }, (_, i) =>
      run({
        id: `run-${i}`,
        issueId: `issue-1`,
        endedAt: new Date(at(`2026-09-01T00:00:00Z`).getTime() + i * 1000),
      })
    )
    rows.push(
      run({
        id: `no-end-stamp`,
        issueId: `issue-1`,
        endedAt: null,
        updatedAt: at(`2026-09-02T00:00:00Z`),
      })
    )
    // A live run with an OLD heartbeat (its machine went quiet) still leads:
    // liveness is by status, the stamp only orders within a group.
    rows.push(
      run({
        id: `stale-live`,
        issueId: `issue-1`,
        status: `running`,
        endedAt: null,
        updatedAt: at(`2026-08-01T00:00:00Z`),
      })
    )
    const ids = selectIssueRuns(rows, `me`, `issue-1`).map((r) => r.id)
    expect(ids).toHaveLength(32)
    expect(ids.slice(0, 4)).toEqual([
      `stale-live`,
      `no-end-stamp`,
      `run-29`,
      `run-28`,
    ])
    expect(ids.at(-1)).toBe(`run-0`)
    expect(LIVE_RUN_LABEL).toBe(`Live`)
  })
})

describe(`pastRunTitle`, () => {
  it(`a row titles itself from whatever it has`, () => {
    const issue = { title: `Fix the sync loop` } as Pick<Issue, `title`>
    expect(pastRunTitle(run({ issueId: `issue-1` }), issue)).toBe(
      `Fix the sync loop`
    )
    // A synced issue with a blank title still names the row.
    expect(
      pastRunTitle(run({ issueId: `issue-1` }), { title: `  ` } as Pick<
        Issue,
        `title`
      >)
    ).toBe(`Untitled issue`)
    // The issue row has not synced yet.
    expect(pastRunTitle(run({ issueId: `issue-1` }), undefined)).toBe(
      `Issue syncing…`
    )
    // The action-name SNAPSHOT survives the action's deletion, and a chat run
    // carries "Chat" as that snapshot (EXP-615) — no branch sniffing.
    expect(pastRunTitle(run({ actionName: `Release train` }), undefined)).toBe(
      `Release train`
    )
    expect(
      pastRunTitle(
        run({ actionName: `Chat`, branch: `exp/chat-1a2b3c4d` }),
        undefined
      )
    ).toBe(`Chat`)
    // Byte-identical ×4: iOS `PastRuns.title`, Android `pastRunTitle`,
    // desktop `session_title` name an issue-less run the same way.
    expect(
      pastRunTitle(run({ branch: `exp/batch-1a2b3c4d` }), undefined)
    ).toBe(`Batch run`)
    expect(pastRunTitle(run({ actionName: `  ` }), undefined)).toBe(`Batch run`)
    expect(pastRunTitle(run(), undefined)).toBe(`Batch run`)
  })

  // EXP-876: a batch names itself after the issues it covered, so two of them
  // in one list are told apart. Mirrored ×4.
  it(`a batch row names itself after its issues`, () => {
    const covered = [
      {
        id: `i-1`,
        identifier: `EXP-874`,
        title: `Session list fixes`,
        branch: `exp/batch-1a2b3c4d`,
        createdAt: at(`2026-09-01T10:00:00Z`),
      },
      {
        id: `i-2`,
        identifier: `EXP-876`,
        title: `Batch run names`,
        branch: `exp/batch-1a2b3c4d`,
        createdAt: at(`2026-09-02T10:00:00Z`),
      },
    ]
    const batch = run({ batchIssueIds: [`i-1`, `i-2`] })
    expect(pastRunTitle(batch, undefined, covered)).toBe(`Session list fixes`)
    expect(pastRunIdentifier(batch, undefined, covered)).toBe(`EXP-874 +1`)
    // One issue is a batch of one — no `+0` suffix.
    expect(
      pastRunIdentifier(run({ batchIssueIds: [`i-2`] }), undefined, covered)
    ).toBe(`EXP-876`)
    // Nothing covered and nothing stored: the generic label, as before.
    expect(pastRunIdentifier(run(), undefined, covered)).toBe(null)
    expect(pastRunTitle(run(), undefined, covered)).toBe(`Batch run`)
    // An issue run keeps ITS identifier; an action run has none.
    expect(
      pastRunIdentifier(run({ issueId: `i-1` }), { identifier: `EXP-874` })
    ).toBe(`EXP-874`)
    expect(pastRunIdentifier(run({ actionName: `Chat` }), undefined)).toBe(null)
  })
})

describe(`pastRunByline`, () => {
  it(`the past byline names the device and when it ended`, () => {
    expect(
      pastRunByline({ deviceLabel: `macbook`, relativeTime: `2 hours ago` })
    ).toBe(`macbook · 2 hours ago`)
    // EXP-833: neither the agent nor who ended the run is part of the byline
    // any more — the sentence is device and time, nothing else.
    expect(pastRunByline({ deviceLabel: `macbook`, relativeTime: `2 hours ago` })).not.toMatch(
      /ended by|Claude/
    )
    // A run that named no machine, or one with no honest time, drops that
    // segment instead of printing a placeholder.
    expect(
      pastRunByline({ deviceLabel: null, relativeTime: `2 hours ago` })
    ).toBe(`2 hours ago`)
    expect(pastRunByline({ deviceLabel: `macbook`, relativeTime: `` })).toBe(
      `macbook`
    )
  })
})
