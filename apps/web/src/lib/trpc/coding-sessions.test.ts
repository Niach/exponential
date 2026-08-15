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
// Every select's where clause, in call order — the only way to assert the
// scoping a fake db can't execute (EXP-432: `get` matches owner OR host, and
// the shared-device lookup is scoped to the CALLER's own device rows).
const selectWheres: unknown[] = []

// Flatten a drizzle condition into its column names + bound values, in order:
// eq(a.b, `x`) ⇒ [`col:b`, `x`].
function whereShape(cond: unknown, out: unknown[] = []): unknown[] {
  if (!cond || typeof cond !== `object`) return out
  if (Array.isArray(cond)) {
    for (const child of cond) whereShape(child, out)
    return out
  }
  const rec = cond as Record<string, unknown>
  if (Array.isArray(rec.queryChunks)) return whereShape(rec.queryChunks, out)
  if (`value` in rec && `encoder` in rec) {
    out.push(rec.value)
    return out
  }
  if (typeof rec.name === `string` && rec.table) {
    out.push(`col:${rec.name}`)
    return out
  }
  return out
}

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
      where: (cond: unknown) => {
        selectWheres.push(cond)
        return { limit: async () => selectResults.shift() ?? [] }
      },
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
  selectWheres.length = 0
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
      // EXP-432: an unattributed start is host-less — the row is the
      // caller's own.
      hostUserId: null,
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
      hostUserId: null,
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
      hostUserId: null,
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

  it(`advances updated_at for a legacy merged row without touching status`, async () => {
    selectResults.push([{ userId: `actor`, status: `merged` }])

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

  it(`re-creates a swept issue-scoped row as ended when the issue's PR merged meanwhile (EXP-498)`, async () => {
    selectResults.push([]) // session row gone (swept)
    // The issue moved on while the laptop slept: PR merged, status done.
    // Merge always closes — the resurrected row comes back `ended` so the
    // owner's kill_watch tears the resumed terminal down.
    selectResults.push([{ status: `done`, prState: `merged` }])

    const result = await caller.heartbeat({
      id: SESSION_ID,
      issueId: ISSUE_ID,
    })

    expect(result).toEqual({ alive: true })
    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.values).toMatchObject({
      id: SESSION_ID,
      issueId: ISSUE_ID,
      status: `ended`,
    })
    expect(inserts[0]!.values.endedAt).toBeInstanceOf(Date)
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

describe(`codingSessions — builtin create-action (EXP-257)`, () => {
  const BUILTIN_ID = `builtin:create-action`

  it(`start requires teamId alongside the builtin literal`, async () => {
    const error = await rejectionOf(caller.start({ actionId: BUILTIN_ID }))
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect(inserts).toHaveLength(0)
  })

  it(`start inserts batch-shaped with actionId NULL + the constant name (no DB action load)`, async () => {
    const result = await caller.start({
      actionId: BUILTIN_ID,
      teamId: TEAM_ID,
      deviceLabel: `MacBook`,
    })
    expect(h.assertTeamMember).toHaveBeenCalledWith(`actor`, TEAM_ID)
    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.table).toBe(codingSessions)
    expect(inserts[0]!.values).toMatchObject({
      teamId: TEAM_ID,
      actionId: null,
      actionName: `Create action`,
      userId: `actor`,
      deviceLabel: `MacBook`,
      status: `running`,
    })
    expect(`issueId` in inserts[0]!.values).toBe(false)
    expect(result.session).toMatchObject({ actionName: `Create action` })
  })

  it(`heartbeat resurrects a builtin row actionId-NULL with the server-constant name`, async () => {
    // Row gone (swept) — only the session select runs: the builtin literal
    // must NEVER be compared against the uuid actions PK (22P02).
    selectResults.push([])
    const result = await caller.heartbeat({
      id: SESSION_ID,
      teamId: TEAM_ID,
      actionId: BUILTIN_ID,
      actionName: `client-sent junk`,
    })
    expect(result).toEqual({ alive: true })
    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.values).toMatchObject({
      id: SESSION_ID,
      teamId: TEAM_ID,
      actionId: null,
      actionName: `Create action`,
      status: `running`,
    })
    // No action-row pre-check select was consumed beyond the session read.
    expect(selectResults).toHaveLength(0)
  })
})

describe(`codingSessions — builtin fix-conflicts (EXP-259)`, () => {
  const FIX_CONFLICTS_ID = `builtin:fix-conflicts`

  it(`start requires teamId alongside the literal`, async () => {
    const error = await rejectionOf(
      caller.start({ actionId: FIX_CONFLICTS_ID })
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect(inserts).toHaveLength(0)
  })

  it(`start inserts batch-shaped with actionId NULL + its own constant name`, async () => {
    const result = await caller.start({
      actionId: FIX_CONFLICTS_ID,
      teamId: TEAM_ID,
      deviceLabel: `MacBook`,
    })
    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.values).toMatchObject({
      teamId: TEAM_ID,
      actionId: null,
      actionName: `Fix merge conflicts`,
      status: `running`,
    })
    expect(result.session).toMatchObject({ actionName: `Fix merge conflicts` })
  })

  it(`heartbeat resurrects with the fix-conflicts constant name`, async () => {
    selectResults.push([])
    const result = await caller.heartbeat({
      id: SESSION_ID,
      teamId: TEAM_ID,
      actionId: FIX_CONFLICTS_ID,
      actionName: `client-sent junk`,
    })
    expect(result).toEqual({ alive: true })
    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.values).toMatchObject({
      id: SESSION_ID,
      teamId: TEAM_ID,
      actionId: null,
      actionName: `Fix merge conflicts`,
      status: `running`,
    })
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

  it(`writes needs_input on a legacy merged row (still live)`, async () => {
    selectResults.push([{ userId: `actor`, status: `merged` }])

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

// ── Shared-device attribution (EXP-432) ──────────────────────────────────────
// The daemon owner (the caller) may hand the row's ownership to the teammate
// who requested the run — but only when their OWN device row says they shared
// that server device with the session's team and the teammate is still a
// member. The row then reads userId = requester, hostUserId = caller.

const DEVICE_ID = `srv-1`
const REQUESTER = `requester`

// What the caller's own devices row looks like for a valid share.
function sharedDevice(overrides: Record<string, unknown> = {}) {
  return { kind: `server`, sharedTeamId: `ws-issue`, ...overrides }
}

describe(`codingSessions.start — shared-device attribution (EXP-432)`, () => {
  it(`attributes an issue-scoped start to the requester with the caller as host`, async () => {
    selectResults.push([sharedDevice()])

    await caller.start({
      issueId: ISSUE_ID,
      deviceId: DEVICE_ID,
      startedById: REQUESTER,
      deviceLabel: `build-box`,
    })

    // Both principals are checked against the session's team: the caller
    // (may start here at all) and the requester (still a member).
    expect(h.assertTeamMember).toHaveBeenCalledWith(`actor`, `ws-issue`)
    expect(h.assertTeamMember).toHaveBeenCalledWith(REQUESTER, `ws-issue`)
    // The device row is looked up among the CALLER's own devices.
    expect(whereShape(selectWheres[0])).toEqual([
      `col:user_id`,
      `actor`,
      `col:device_id`,
      DEVICE_ID,
    ])
    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.values).toMatchObject({
      issueId: ISSUE_ID,
      teamId: `ws-issue`,
      userId: REQUESTER,
      hostUserId: `actor`,
    })
  })

  it(`attributes a batch start the same way, against the batch's team`, async () => {
    selectResults.push([sharedDevice({ sharedTeamId: TEAM_ID })])

    await caller.start({
      teamId: TEAM_ID,
      deviceId: DEVICE_ID,
      startedById: REQUESTER,
    })

    expect(h.assertTeamMember).toHaveBeenCalledWith(REQUESTER, TEAM_ID)
    expect(inserts[0]!.values).toMatchObject({
      teamId: TEAM_ID,
      userId: REQUESTER,
      hostUserId: `actor`,
    })
  })

  it(`attributes an action start, resolving the team from the action row`, async () => {
    selectResults.push([{ id: ACTION_ID, teamId: TEAM_ID, name: `Code review` }])
    selectResults.push([sharedDevice({ sharedTeamId: TEAM_ID })])

    await caller.start({
      actionId: ACTION_ID,
      deviceId: DEVICE_ID,
      startedById: REQUESTER,
    })

    expect(inserts[0]!.values).toMatchObject({
      actionId: ACTION_ID,
      userId: REQUESTER,
      hostUserId: `actor`,
    })
  })

  it(`attributes a builtin start (no DB action row)`, async () => {
    selectResults.push([sharedDevice({ sharedTeamId: TEAM_ID })])

    await caller.start({
      actionId: `builtin:create-action`,
      teamId: TEAM_ID,
      deviceId: DEVICE_ID,
      startedById: REQUESTER,
    })

    expect(inserts[0]!.values).toMatchObject({
      actionName: `Create action`,
      userId: REQUESTER,
      hostUserId: `actor`,
    })
  })

  it(`requires the sharing device's deviceId`, async () => {
    const error = await rejectionOf(
      caller.start({ issueId: ISSUE_ID, startedById: REQUESTER })
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect((error as TRPCError).message).toContain(`deviceId`)
    expect(inserts).toHaveLength(0)
  })

  it(`refuses a device shared with a DIFFERENT team`, async () => {
    selectResults.push([sharedDevice({ sharedTeamId: `ws-other` })])

    const error = await rejectionOf(
      caller.start({
        issueId: ISSUE_ID,
        deviceId: DEVICE_ID,
        startedById: REQUESTER,
      })
    )
    expect((error as TRPCError).code).toBe(`FORBIDDEN`)
    expect((error as TRPCError).message).toContain(`not shared`)
    expect(inserts).toHaveLength(0)
  })

  it(`refuses a non-server (desktop) device`, async () => {
    selectResults.push([sharedDevice({ kind: `desktop` })])

    const error = await rejectionOf(
      caller.start({
        issueId: ISSUE_ID,
        deviceId: DEVICE_ID,
        startedById: REQUESTER,
      })
    )
    expect((error as TRPCError).code).toBe(`FORBIDDEN`)
    expect(inserts).toHaveLength(0)
  })

  it(`refuses a deviceId the caller doesn't own`, async () => {
    selectResults.push([]) // no row among the caller's devices

    const error = await rejectionOf(
      caller.start({
        issueId: ISSUE_ID,
        deviceId: DEVICE_ID,
        startedById: REQUESTER,
      })
    )
    expect((error as TRPCError).code).toBe(`FORBIDDEN`)
    expect(inserts).toHaveLength(0)
  })

  it(`refuses attributing to a NON-member of the session's team`, async () => {
    selectResults.push([sharedDevice()])
    h.assertTeamMember.mockImplementation(async (userId: unknown) => {
      if (userId === REQUESTER) throw new TRPCError({ code: `FORBIDDEN` })
      return { role: `member` }
    })

    const error = await rejectionOf(
      caller.start({
        issueId: ISSUE_ID,
        deviceId: DEVICE_ID,
        startedById: REQUESTER,
      })
    )
    expect((error as TRPCError).code).toBe(`FORBIDDEN`)
    expect(inserts).toHaveLength(0)
  })

  it(`self-attribution stays the plain host-less path (no device lookup)`, async () => {
    await caller.start({
      issueId: ISSUE_ID,
      deviceId: DEVICE_ID,
      startedById: `actor`,
    })

    expect(selectWheres).toHaveLength(0)
    expect(inserts[0]!.values).toMatchObject({
      userId: `actor`,
      hostUserId: null,
    })
  })
})

describe(`codingSessions — host allowances (EXP-432)`, () => {
  it(`heartbeat: the host advances a requester-owned row`, async () => {
    selectResults.push([
      { userId: REQUESTER, hostUserId: `actor`, status: `running` },
    ])

    const result = await caller.heartbeat({ id: SESSION_ID })

    expect(result).toEqual({ alive: true })
    expect(updates).toHaveLength(1)
  })

  it(`heartbeat: a third user is still FORBIDDEN`, async () => {
    selectResults.push([
      { userId: REQUESTER, hostUserId: `host-x`, status: `running` },
    ])

    const error = await rejectionOf(caller.heartbeat({ id: SESSION_ID }))
    expect((error as TRPCError).code).toBe(`FORBIDDEN`)
    expect(updates).toHaveLength(0)
  })

  it(`heartbeat: a swept shared-device row resurrects requester-owned`, async () => {
    selectResults.push([]) // session row gone (swept)
    selectResults.push([sharedDevice()]) // the caller's shared device
    selectResults.push([{ status: `in_progress` }]) // the issue

    const result = await caller.heartbeat({
      id: SESSION_ID,
      issueId: ISSUE_ID,
      deviceId: DEVICE_ID,
      startedById: REQUESTER,
    })

    expect(result).toEqual({ alive: true })
    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.values).toMatchObject({
      id: SESSION_ID,
      issueId: ISSUE_ID,
      userId: REQUESTER,
      hostUserId: `actor`,
      status: `running`,
    })
  })

  it(`heartbeat: a failed attribution degrades to { alive: false } instead of throwing`, async () => {
    selectResults.push([]) // session row gone
    selectResults.push([]) // …and the share is gone too

    const result = await caller.heartbeat({
      id: SESSION_ID,
      issueId: ISSUE_ID,
      deviceId: DEVICE_ID,
      startedById: REQUESTER,
    })

    expect(result).toEqual({ alive: false })
    expect(inserts).toHaveLength(0)
  })

  it(`setNeedsInput: the host may flag a requester-owned row`, async () => {
    selectResults.push([
      { userId: REQUESTER, hostUserId: `actor`, status: `running` },
    ])

    const result = await caller.setNeedsInput({
      id: SESSION_ID,
      needsInput: true,
    })

    expect(result).toEqual({ updated: true })
    expect(updates[0]!.values).toEqual({ needsInput: true })
  })

  it(`setNeedsInput: a third user is still FORBIDDEN`, async () => {
    selectResults.push([
      { userId: REQUESTER, hostUserId: `host-x`, status: `running` },
    ])

    const error = await rejectionOf(
      caller.setNeedsInput({ id: SESSION_ID, needsInput: true })
    )
    expect((error as TRPCError).code).toBe(`FORBIDDEN`)
    expect(updates).toHaveLength(0)
  })

  it(`end: the host may end the run on its own machine`, async () => {
    selectResults.push([
      {
        id: SESSION_ID,
        userId: REQUESTER,
        hostUserId: `actor`,
        status: `running`,
      },
    ])

    const result = await caller.end({ id: SESSION_ID })

    expect(result.session).toMatchObject({ id: SESSION_ID })
    expect(updates).toHaveLength(1)
    expect(updates[0]!.values).toMatchObject({ status: `ended` })
  })

  it(`end: a third user is still FORBIDDEN`, async () => {
    selectResults.push([
      {
        id: SESSION_ID,
        userId: REQUESTER,
        hostUserId: `host-x`,
        status: `running`,
      },
    ])

    const error = await rejectionOf(caller.end({ id: SESSION_ID }))
    expect((error as TRPCError).code).toBe(`FORBIDDEN`)
    expect(updates).toHaveLength(0)
  })

  it(`get: scopes the row to owner OR host`, async () => {
    selectResults.push([
      { id: SESSION_ID, userId: REQUESTER, hostUserId: `actor` },
    ])

    const result = await caller.get({ id: SESSION_ID })

    expect(result.session).toMatchObject({ id: SESSION_ID })
    // The fake db can't execute a where clause, so assert its shape: the id
    // AND an or() over both principals.
    expect(whereShape(selectWheres[0])).toEqual([
      `col:id`,
      SESSION_ID,
      `col:user_id`,
      `actor`,
      `col:host_user_id`,
      `actor`,
    ])
  })

  it(`get: hands the whole row back, host_user_id included`, async () => {
    // The CLI daemon polls this for the →ended kill edge on rows it only
    // HOSTS (EXP-403/445). A narrowed projection that dropped host_user_id
    // would hide the ownership split from the only client that has no sync.
    selectResults.push([
      {
        id: SESSION_ID,
        userId: REQUESTER,
        hostUserId: `actor`,
        status: `ended`,
      },
    ])

    const result = await caller.get({ id: SESSION_ID })

    expect(result.session).toMatchObject({
      userId: REQUESTER,
      hostUserId: `actor`,
      status: `ended`,
    })
  })
})
