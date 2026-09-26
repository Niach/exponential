import { describe, expect, it, vi } from "vitest"
import { PgDialect } from "drizzle-orm/pg-core"

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))

import { findLiveRunForIssues, liveRunConflictMessage } from "@/lib/trpc/coding-sessions"

// FEED-57/46/47: the live-run probe behind `codingSessions.liveForIssue` and
// the fresh `steer.startSession` refusal. A batch run holds every issue its
// `batch_issue_ids` names, not just an issue run its own issue.

function fakeDb(results: unknown[][]) {
  const queue = [...results]
  const wheres: unknown[] = []
  const chain = (rows: unknown[]) => {
    const p = Promise.resolve(rows) as Promise<unknown[]> & Record<string, (arg?: unknown) => unknown>
    for (const m of [`from`, `orderBy`, `limit`]) p[m] = () => p
    p.where = (cond?: unknown) => {
      wheres.push(cond)
      return p
    }
    return p
  }
  return { db: { select: () => chain(queue.shift() ?? []) } as never, wheres }
}

const RUN = {
  id: `run-1`,
  deviceLabel: `studio`,
  userId: `u-1`,
  startedReason: null,
  branch: `exp/batch-1a2b3c4d`,
  issueId: null,
  batchIssueIds: [`b`, `c`],
}

describe(`findLiveRunForIssues`, () => {
  it(`asks nothing for no issues`, async () => {
    const { db, wheres } = fakeDb([])
    expect(await findLiveRunForIssues(db, [])).toBeNull()
    expect(wheres).toEqual([])
  })

  it(`matches the issue's own run OR a batch covering it, live and fresh`, async () => {
    const { db, wheres } = fakeDb([[]])
    expect(await findLiveRunForIssues(db, [`a`, `b`])).toBeNull()
    const query = new PgDialect().sqlToQuery(wheres[0] as never)
    expect(query.sql).toContain(`"issue_id" in`)
    expect(query.sql).toContain(`"batch_issue_ids" ?|`)
    expect(query.sql).toContain(`"status" in`)
    expect(query.sql).toContain(`"updated_at" >=`)
  })

  it(`names the asked-for issues the batch run covers, and the run's owner`, async () => {
    const { db, wheres } = fakeDb([
      [RUN],
      [{ id: `b`, identifier: `EXP-2` }],
      [{ name: `Dana`, email: `dana@example.com` }],
    ])
    const live = await findLiveRunForIssues(db, [`a`, `b`])
    expect(live).toEqual({
      id: `run-1`,
      deviceLabel: `studio`,
      userId: `u-1`,
      startedReason: null,
      branch: `exp/batch-1a2b3c4d`,
      identifiers: [`EXP-2`],
      owner: { name: `Dana`, email: `dana@example.com` },
    })
    // The owner lookup keys on the run's user, not the caller.
    const ownerQuery = new PgDialect().sqlToQuery(wheres[2] as never)
    expect(ownerQuery.sql).toContain(`"id" =`)
    expect(ownerQuery.params).toEqual([`u-1`])
    expect(liveRunConflictMessage(live!)).toBe(
      `EXP-2 already has a live run on studio (session run-1, started by a person, branch exp/batch-1a2b3c4d). Stop it or let it end before starting another.`
    )
  })

  it(`reads a vanished owner row as no owner`, async () => {
    const { db } = fakeDb([[RUN], [{ id: `b`, identifier: `EXP-2` }], []])
    const live = await findLiveRunForIssues(db, [`b`])
    expect(live?.owner).toBeNull()
  })
})

// EXP-312: only the owner can stop a live run, so the refusal tells everyone
// else WHOSE run it is instead of asking them to stop it.
describe(`liveRunConflictMessage`, () => {
  const live = {
    id: `run-1`,
    deviceLabel: `studio`,
    userId: `u-1`,
    startedReason: `workflow`,
    branch: `exp/batch-1a2b3c4d`,
    identifiers: [`EXP-2`],
    owner: { name: `Dana`, email: `dana@example.com` },
  }

  it(`tells the owner to stop it`, () => {
    expect(liveRunConflictMessage(live, `u-1`)).toBe(
      `EXP-2 already has a live run on studio (session run-1, started by workflow, branch exp/batch-1a2b3c4d). Stop it or let it end before starting another.`
    )
  })

  it(`tells anyone else who owns it`, () => {
    expect(liveRunConflictMessage(live, `u-2`)).toBe(
      `EXP-2 already has a live run on studio (session run-1, started by workflow, branch exp/batch-1a2b3c4d). Dana (dana@example.com) owns it: only they can stop it, or let it end before starting another.`
    )
  })

  it(`falls back to the email, then to "another member"`, () => {
    expect(liveRunConflictMessage({ ...live, owner: { name: ``, email: `dana@example.com` } }, `u-2`)).toContain(
      ` dana@example.com owns it:`
    )
    expect(liveRunConflictMessage({ ...live, owner: null, deviceLabel: null, branch: null }, `u-2`)).toBe(
      `EXP-2 already has a live run (session run-1, started by workflow). another member owns it: only they can stop it, or let it end before starting another.`
    )
  })
})
