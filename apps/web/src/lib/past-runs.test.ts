import { describe, expect, it } from "vitest"
import {
  pastRunByline,
  pastRunEndedAt,
  pastRunTitle,
  selectPastRuns,
  PAST_RUN_CAP,
} from "./past-runs"
import type { CodingSession, Issue } from "@/db/schema"

// EXP-746 — the "Past" section's rules. Every `it` name here is mirrored by
// iOS PastRunsTests, Android AgentRowsTest and the desktop
// `own_ended_runs_*` tests; a change on one side without the others is a
// cross-client drift, not a tweak.

type PastRun = Pick<
  CodingSession,
  | `id`
  | `status`
  | `startedReason`
  | `userId`
  | `teamId`
  | `issueId`
  | `actionName`
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
    branch: null,
    agent: `claude`,
    endedBy: `agent`,
    endedAt: at(`2026-09-01T10:00:00Z`),
    updatedAt: at(`2026-09-01T09:00:00Z`),
    ...over,
  }
}

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

describe(`pastRunTitle`, () => {
  it(`title falls back through issue, action, batch`, () => {
    const issue = { title: `Fix the sync loop` } as Pick<Issue, `title`>
    expect(pastRunTitle(run({ issueId: `issue-1` }), issue)).toBe(
      `Fix the sync loop`
    )
    // The issue row has not synced yet.
    expect(pastRunTitle(run({ issueId: `issue-1` }), undefined)).toBe(
      `Issue syncing…`
    )
    // The action-name SNAPSHOT survives the action's deletion.
    expect(pastRunTitle(run({ actionName: `Release train` }), undefined)).toBe(
      `Release train`
    )
    expect(
      pastRunTitle(run({ branch: `exp/chat-1a2b3c4d` }), undefined)
    ).toBe(`Chat session`)
    expect(
      pastRunTitle(run({ branch: `exp/batch-1a2b3c4d` }), undefined)
    ).toBe(`Batch session`)
    expect(pastRunTitle(run(), undefined)).toBe(`Batch session`)
  })
})

describe(`pastRunByline`, () => {
  it(`the past byline names device, agent and who ended it`, () => {
    const parts = {
      deviceLabel: `macbook`,
      agentLabel: `Claude Code`,
      relativeTime: `2 hours ago`,
    }
    expect(pastRunByline(run({ endedBy: `agent` }), parts)).toBe(
      `macbook · Claude Code · ended by agent · 2 hours ago`
    )
    expect(pastRunByline(run({ endedBy: `user` }), parts)).toBe(
      `macbook · Claude Code · ended by you · 2 hours ago`
    )
    expect(pastRunByline(run({ endedBy: `client` }), parts)).toBe(
      `macbook · Claude Code · ended by the app · 2 hours ago`
    )
    expect(pastRunByline(run({ endedBy: `merge` }), parts)).toBe(
      `macbook · Claude Code · ended by a merge · 2 hours ago`
    )
    expect(pastRunByline(run({ endedBy: `system` }), parts)).toBe(
      `macbook · Claude Code · ended by the system · 2 hours ago`
    )
    // A row from a pre-EXP-637 server names no ender, and a run that named no
    // machine or agent drops those segments instead of printing a placeholder.
    expect(pastRunByline(run({ endedBy: null }), parts)).toBe(
      `macbook · Claude Code · 2 hours ago`
    )
    expect(
      pastRunByline(run({ endedBy: `user` }), {
        deviceLabel: null,
        agentLabel: null,
        relativeTime: `2 hours ago`,
      })
    ).toBe(`ended by you · 2 hours ago`)
  })
})
