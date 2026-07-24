import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// Triple-subject coding sessions: `start` takes EXACTLY ONE of
// issueId/teamId/actionId (zod refine). The issue path denormalizes
// teamId/boardId from the issue's context; the batch path asserts
// membership against the given team and inserts with teamId only —
// issueId/boardId must stay absent so the row never leaks through the
// anonymous board-scoped shape clause; the action path (EXP-253) resolves
// the action row and inserts batch-shaped plus actionId + the actionName
// snapshot. The router runs against ctx.db (no transaction/generateTxId),
// so a fake db with an insert recorder is enough.

const h = vi.hoisted(() => ({
  assertTeamMember: vi.fn(
    async (..._args: unknown[]) => ({ role: `member` }) as unknown
  ),
  getIssueTeamContext: vi.fn(async () => ({
    issueId: `issue-1`,
    boardId: `proj-1`,
    teamId: `ws-issue`,
  })),
}))

// lib/trpc.ts + lib/admin.ts import db/auth at module scope; runtime here only
// needs the exports to exist.
vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))

vi.mock(`@/lib/team-membership`, () => ({
  assertTeamMember: h.assertTeamMember,
  getIssueTeamContext: h.getIssueTeamContext,
}))

import { codingSessionsRouter } from "@/lib/trpc/coding-sessions"
import { codingSessions } from "@/db/schema"

const ISSUE_ID = `11111111-1111-4111-8111-111111111111`
const TEAM_ID = `22222222-2222-4222-8222-222222222222`
const SESSION_ID = `33333333-3333-4333-8333-333333333333`
const ACTION_ID = `44444444-4444-4444-8444-444444444444`

const inserts: { table: unknown; values: Record<string, unknown> }[] = []
const updates: { table: unknown; values: Record<string, unknown> }[] = []
// Queued results for successive db.select(...).limit(1) calls (heartbeat
// reads the session row, then — on the issue-scoped re-create — the issue).
const selectResults: unknown[][] = []

const fakeDb = {
  insert: (table: unknown) => ({
    values: (values: Record<string, unknown>) => {
      inserts.push({ table, values })
      return {
        returning: async () => [{ id: SESSION_ID, ...values }],
        // The heartbeat re-create insert is awaited without .returning().
        then: (resolve: (value: unknown) => unknown) =>
          Promise.resolve(undefined).then(resolve),
      }
    },
  }),
  select: () => ({
    from: () => ({
      where: () => ({
        limit: async () => selectResults.shift() ?? [],
      }),
    }),
  }),
  update: (table: unknown) => ({
    set: (values: Record<string, unknown>) => ({
      where: () => ({
        returning: async () => {
          updates.push({ table, values })
          return [{ id: SESSION_ID }]
        },
      }),
    }),
  }),
}

const caller = codingSessionsRouter.createCaller({
  session: { user: { id: `actor` } },
  db: fakeDb,
  request: new Request(`http://localhost/`),
} as never)

async function rejectionOf(promise: Promise<unknown>): Promise<unknown> {
  return promise.then(
    () => undefined,
    (e: unknown) => e
  )
}

beforeEach(() => {
  inserts.length = 0
  updates.length = 0
  selectResults.length = 0
  h.assertTeamMember.mockClear()
  h.assertTeamMember.mockResolvedValue({ role: `member` })
  h.getIssueTeamContext.mockClear()
  h.getIssueTeamContext.mockResolvedValue({
    issueId: `issue-1`,
    boardId: `proj-1`,
    teamId: `ws-issue`,
  })
})

describe(`codingSessions.start — exactly-one-subject refine`, () => {
  it(`rejects BOTH issueId and teamId as input validation`, async () => {
    const error = await rejectionOf(
      caller.start({ issueId: ISSUE_ID, teamId: TEAM_ID })
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect((error as TRPCError).message).toContain(
      `Exactly one of issueId/teamId/actionId is required`
    )
    expect(inserts).toHaveLength(0)
    expect(h.getIssueTeamContext).not.toHaveBeenCalled()
    expect(h.assertTeamMember).not.toHaveBeenCalled()
  })

  it(`rejects issueId + actionId as input validation`, async () => {
    const error = await rejectionOf(
      caller.start({ issueId: ISSUE_ID, actionId: ACTION_ID })
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect(inserts).toHaveLength(0)
  })

  it(`rejects NEITHER id as input validation`, async () => {
    const error = await rejectionOf(caller.start({}))
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect((error as TRPCError).message).toContain(
      `Exactly one of issueId/teamId/actionId is required`
    )
    expect(inserts).toHaveLength(0)
    expect(h.assertTeamMember).not.toHaveBeenCalled()
  })
})

describe(`codingSessions.start — issue path`, () => {
  it(`inserts with issueId + denormalized teamId/boardId after asserting membership`, async () => {
    const result = await caller.start({
      issueId: ISSUE_ID,
      deviceLabel: `MacBook`,
    })

    expect(h.getIssueTeamContext).toHaveBeenCalledWith(ISSUE_ID)
    expect(h.assertTeamMember).toHaveBeenCalledWith(`actor`, `ws-issue`)
    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.table).toBe(codingSessions)
    expect(inserts[0]!.values).toEqual({
      issueId: ISSUE_ID,
      teamId: `ws-issue`,
      boardId: `proj-1`,
      userId: `actor`,
      deviceLabel: `MacBook`,
      status: `running`,
    })
    expect(result.session).toMatchObject({ id: SESSION_ID, issueId: ISSUE_ID })
  })

  it(`refuses a non-member of the issue's team before inserting`, async () => {
    h.assertTeamMember.mockRejectedValueOnce(
      new TRPCError({ code: `FORBIDDEN` })
    )

    const error = await rejectionOf(caller.start({ issueId: ISSUE_ID }))
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`FORBIDDEN`)
    expect(inserts).toHaveLength(0)
  })
})

describe(`codingSessions.start — batch path`, () => {
  it(`inserts with the teamId and NO issueId/boardId`, async () => {
    const result = await caller.start({
      teamId: TEAM_ID,
      deviceLabel: `MacBook`,
    })

    // Membership is asserted against the given team.
    expect(h.assertTeamMember).toHaveBeenCalledWith(`actor`, TEAM_ID)
    expect(h.getIssueTeamContext).not.toHaveBeenCalled()
    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.table).toBe(codingSessions)
    expect(inserts[0]!.values).toEqual({
      teamId: TEAM_ID,
      userId: `actor`,
      deviceLabel: `MacBook`,
      status: `running`,
    })
    // A batch run spans boards: issue_id/board_id must be ABSENT so the
    // populate triggers no-op and the anonymous board-scoped shape clause
    // can never match the row.
    expect(`issueId` in inserts[0]!.values).toBe(false)
    expect(`boardId` in inserts[0]!.values).toBe(false)
    expect(result.session).toMatchObject({
      id: SESSION_ID,
      teamId: TEAM_ID,
    })
  })

  it(`refuses a non-member of the team before inserting`, async () => {
    h.assertTeamMember.mockRejectedValueOnce(
      new TRPCError({ code: `FORBIDDEN` })
    )

    const error = await rejectionOf(
      caller.start({ teamId: TEAM_ID })
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`FORBIDDEN`)
    expect(inserts).toHaveLength(0)
  })

  it(`nulls an omitted deviceLabel`, async () => {
    await caller.start({ teamId: TEAM_ID })

    expect(inserts[0]!.values.deviceLabel).toBeNull()
  })
})

describe(`codingSessions.start — action path (EXP-253)`, () => {
  it(`inserts batch-shaped plus actionId + the server-resolved name snapshot`, async () => {
    selectResults.push([
      { id: ACTION_ID, teamId: TEAM_ID, name: `Code review` },
    ])

    const result = await caller.start({
      actionId: ACTION_ID,
      deviceLabel: `MacBook`,
    })

    // Membership is asserted against the ACTION's team, not client input.
    expect(h.assertTeamMember).toHaveBeenCalledWith(`actor`, TEAM_ID)
    expect(h.getIssueTeamContext).not.toHaveBeenCalled()
    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.values).toEqual({
      teamId: TEAM_ID,
      actionId: ACTION_ID,
      actionName: `Code review`,
      userId: `actor`,
      deviceLabel: `MacBook`,
      status: `running`,
    })
    // Action rows are batch-shaped: issue_id/board_id absent so the populate
    // triggers no-op and no board-scoped clause can ever match the row.
    expect(`issueId` in inserts[0]!.values).toBe(false)
    expect(`boardId` in inserts[0]!.values).toBe(false)
    expect(result.session).toMatchObject({
      id: SESSION_ID,
      actionId: ACTION_ID,
    })
  })

  it(`404s a missing action before any membership check or insert`, async () => {
    selectResults.push([]) // action row gone

    const error = await rejectionOf(caller.start({ actionId: ACTION_ID }))
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`NOT_FOUND`)
    expect(h.assertTeamMember).not.toHaveBeenCalled()
    expect(inserts).toHaveLength(0)
  })

  it(`refuses a non-member of the action's team before inserting`, async () => {
    selectResults.push([
      { id: ACTION_ID, teamId: TEAM_ID, name: `Code review` },
    ])
    h.assertTeamMember.mockRejectedValueOnce(
      new TRPCError({ code: `FORBIDDEN` })
    )

    const error = await rejectionOf(caller.start({ actionId: ACTION_ID }))
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`FORBIDDEN`)
    expect(inserts).toHaveLength(0)
  })
})

// EXP-194: `in_review` (PR open, terminal still alive) heartbeats like
// `running`, but the ping only ever advances updated_at — it can never
// downgrade the status — and an `ended` row stays final.
describe(`codingSessions.heartbeat — in_review liveness`, () => {
  it(`advances updated_at for an in_review row without touching status`, async () => {
    selectResults.push([{ userId: `actor`, status: `in_review` }])

    const result = await caller.heartbeat({ id: SESSION_ID })

    expect(result).toEqual({ alive: true })
    expect(updates).toHaveLength(1)
    expect(Object.keys(updates[0]!.values)).toEqual([`updatedAt`])
  })

  it(`reports an ended row as dead without any write`, async () => {
    selectResults.push([{ userId: `actor`, status: `ended` }])

    const result = await caller.heartbeat({ id: SESSION_ID })

    expect(result).toEqual({ alive: false })
    expect(updates).toHaveLength(0)
    expect(inserts).toHaveLength(0)
  })

  it(`re-creates a swept issue-scoped row as in_review when the issue is parked in review`, async () => {
    selectResults.push([]) // session row gone (swept)
    selectResults.push([{ status: `in_review` }]) // the issue's own status

    const result = await caller.heartbeat({
      id: SESSION_ID,
      issueId: ISSUE_ID,
    })

    expect(result).toEqual({ alive: true })
    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.values).toMatchObject({
      id: SESSION_ID,
      issueId: ISSUE_ID,
      status: `in_review`,
    })
  })

  it(`re-creates a swept batch row as running (no issue to derive from)`, async () => {
    selectResults.push([]) // session row gone (swept)

    const result = await caller.heartbeat({
      id: SESSION_ID,
      teamId: TEAM_ID,
    })

    expect(result).toEqual({ alive: true })
    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.values).toMatchObject({
      id: SESSION_ID,
      teamId: TEAM_ID,
      status: `running`,
    })
    // A plain batch scope carries no action fields.
    expect(inserts[0]!.values.actionId).toBeNull()
    expect(inserts[0]!.values.actionName).toBeNull()
  })

  it(`re-creates a swept action row from the client snapshot (EXP-253)`, async () => {
    selectResults.push([]) // session row gone (swept)
    selectResults.push([{ id: ACTION_ID, teamId: TEAM_ID }]) // action exists, same team

    const result = await caller.heartbeat({
      id: SESSION_ID,
      teamId: TEAM_ID,
      actionId: ACTION_ID,
      actionName: `Code review`,
    })

    expect(result).toEqual({ alive: true })
    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.values).toMatchObject({
      id: SESSION_ID,
      teamId: TEAM_ID,
      actionId: ACTION_ID,
      actionName: `Code review`,
      status: `running`,
    })
  })

  it(`degrades a swept action row to batch-shaped when the action is gone`, async () => {
    selectResults.push([]) // session row gone (swept)
    selectResults.push([]) // the action was deleted meanwhile

    const result = await caller.heartbeat({
      id: SESSION_ID,
      teamId: TEAM_ID,
      actionId: ACTION_ID,
      actionName: `Code review`,
    })

    expect(result).toEqual({ alive: true })
    expect(inserts).toHaveLength(1)
    // action_id NULL, actionName kept — the same shape FK SET NULL leaves
    // on live rows when their action is deleted.
    expect(inserts[0]!.values).toMatchObject({
      id: SESSION_ID,
      teamId: TEAM_ID,
      actionId: null,
      actionName: `Code review`,
      status: `running`,
    })
  })

  it(`degrades a cross-team actionId to batch-shaped (never a cross-tenant FK)`, async () => {
    selectResults.push([]) // session row gone (swept)
    // The action exists but belongs to ANOTHER team than the claimed scope —
    // the resurrect must strip it exactly like a deleted action.
    selectResults.push([{ id: ACTION_ID, teamId: `99999999-9999-4999-8999-999999999999` }])

    const result = await caller.heartbeat({
      id: SESSION_ID,
      teamId: TEAM_ID,
      actionId: ACTION_ID,
      actionName: `Code review`,
    })

    expect(result).toEqual({ alive: true })
    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.values).toMatchObject({
      teamId: TEAM_ID,
      actionId: null,
      actionName: `Code review`,
    })
  })

  it(`rejects an action scope without its teamId as input validation`, async () => {
    const error = await rejectionOf(
      caller.heartbeat({ id: SESSION_ID, actionId: ACTION_ID })
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect((error as TRPCError).message).toContain(
      `actionId requires teamId and excludes issueId`
    )
    expect(inserts).toHaveLength(0)
  })
})

describe(`codingSessions.setNeedsInput — attention flag (EXP-214)`, () => {
  it(`writes exactly needs_input on a live owned row`, async () => {
    selectResults.push([{ userId: `actor`, status: `running` }])

    const result = await caller.setNeedsInput({
      id: SESSION_ID,
      needsInput: true,
    })

    expect(result).toEqual({ updated: true })
    expect(updates).toHaveLength(1)
    expect(updates[0]!.values).toEqual({ needsInput: true })
  })

  it(`reports a swept row without writing`, async () => {
    selectResults.push([]) // row gone

    const result = await caller.setNeedsInput({
      id: SESSION_ID,
      needsInput: true,
    })

    expect(result).toEqual({ updated: false })
    expect(updates).toHaveLength(0)
  })

  it(`refuses a non-owner`, async () => {
    selectResults.push([{ userId: `someone-else`, status: `running` }])

    const error = await rejectionOf(
      caller.setNeedsInput({ id: SESSION_ID, needsInput: false })
    )

    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`FORBIDDEN`)
    expect(updates).toHaveLength(0)
  })
})
