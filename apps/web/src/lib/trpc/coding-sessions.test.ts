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

// EXP-700: `end` tells a live parent about a vanished agent-started child;
// the helper is internally best-effort, so the router just calls it.
vi.mock(`@/lib/steer-child-messages`, () => ({
  notifyParentOfChildEnd: vi.fn(async () => ({ delivered: false })),
}))

import { codingSessionsRouter } from "@/lib/trpc/coding-sessions"
import { codingSessions } from "@/db/schema"
import { notifyParentOfChildEnd } from "@/lib/steer-child-messages"

const ISSUE_ID = `11111111-1111-4111-8111-111111111111`
const TEAM_ID = `22222222-2222-4222-8222-222222222222`
const SESSION_ID = `33333333-3333-4333-8333-333333333333`
const ACTION_ID = `44444444-4444-4444-8444-444444444444`

const inserts: { table: unknown; values: Record<string, unknown> }[] = []
const updates: { table: unknown; values: Record<string, unknown> }[] = []
// Every update's where clause, in call order — the fake db can't execute the
// status fence, so tests assert its SHAPE instead (EXP-531: a needs_input
// `true` write is fenced to `running` rows only).
const updateWheres: unknown[] = []
// Queued rows for successive update(...).returning() calls; empty = the
// default single row. Lets a test model a conditional update that matched
// NOTHING (EXP-700: `end` racing the agent's own close-out).
const updateResults: unknown[][] = []
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
      where: (cond: unknown) => ({
        returning: async () => {
          updates.push({ table, values })
          updateWheres.push(cond)
          return updateResults.shift() ?? [{ id: SESSION_ID }]
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
  updateWheres.length = 0
  updateResults.length = 0
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
  vi.mocked(notifyParentOfChildEnd).mockClear()
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
    const result = await caller.start({ issueId: ISSUE_ID })

    expect(h.getIssueTeamContext).toHaveBeenCalledWith(ISSUE_ID)
    expect(h.assertTeamMember).toHaveBeenCalledWith(`actor`, `ws-issue`)
    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.table).toBe(codingSessions)
    expect(inserts[0]!.values).toEqual({
      issueId: ISSUE_ID,
      teamId: `ws-issue`,
      boardId: `proj-1`,
      // EXP-679: an issue start is a person's unless an agent asked for it.
      startedReason: null,
      userId: `actor`,
      // EXP-432: an unattributed start is host-less — the row is the
      // caller's own.
      hostUserId: null,
      deviceId: null,
      deviceLabel: null,
      // EXP-484: no agent named by this start.
      agent: null,
      // EXP-637: issue rows carry no run branch (the issue owns
      // `exp/<IDENTIFIER>`) and this start resumes nothing.
      resumedFromId: null,
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
    const result = await caller.start({ teamId: TEAM_ID })

    // Membership is asserted against the given team.
    expect(h.assertTeamMember).toHaveBeenCalledWith(`actor`, TEAM_ID)
    expect(h.getIssueTeamContext).not.toHaveBeenCalled()
    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.table).toBe(codingSessions)
    expect(inserts[0]!.values).toEqual({
      teamId: TEAM_ID,
      // EXP-679: a batch start is a person's unless an agent asked for it.
      startedReason: null,
      userId: `actor`,
      hostUserId: null,
      deviceId: null,
      deviceLabel: null,
      agent: null,
      branch: null,
      resumedFromId: null,
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

  it(`nulls the device stamp when no deviceId rides along`, async () => {
    await caller.start({ teamId: TEAM_ID })

    expect(inserts[0]!.values.deviceId).toBeNull()
    expect(inserts[0]!.values.deviceLabel).toBeNull()
  })
})

// EXP-484: the run records which agent CLI drives it.
describe(`codingSessions.start — agent (EXP-484)`, () => {
  it(`stores the named agent on an issue run`, async () => {
    await caller.start({ issueId: ISSUE_ID, agent: `codex` })
    expect(inserts[0]!.values.agent).toBe(`codex`)
  })

  it(`stores it on batch runs too, and rejects an off-contract agent`, async () => {
    await caller.start({ teamId: TEAM_ID, agent: `pi` })
    expect(inserts[0]!.values.agent).toBe(`pi`)

    const error = await rejectionOf(
      caller.start({ teamId: TEAM_ID, agent: `aider` } as never)
    )
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
  })
})

describe(`codingSessions.start — action path (EXP-253)`, () => {
  it(`inserts batch-shaped plus actionId + the server-resolved name snapshot`, async () => {
    selectResults.push([
      { id: ACTION_ID, teamId: TEAM_ID, name: `Code review` },
    ])

    const result = await caller.start({ actionId: ACTION_ID })

    // Membership is asserted against the ACTION's team, not client input.
    expect(h.assertTeamMember).toHaveBeenCalledWith(`actor`, TEAM_ID)
    expect(h.getIssueTeamContext).not.toHaveBeenCalled()
    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.values).toEqual({
      teamId: TEAM_ID,
      actionId: ACTION_ID,
      actionName: `Code review`,
      startedReason: null,
      automationId: null,
      userId: `actor`,
      hostUserId: null,
      deviceId: null,
      deviceLabel: null,
      agent: null,
      branch: null,
      resumedFromId: null,
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

  it(`stamps startedReason on automated action starts (EXP-530)`, async () => {
    selectResults.push([
      { id: ACTION_ID, teamId: TEAM_ID, name: `Code review` },
    ])

    await caller.start({ actionId: ACTION_ID, startedReason: `schedule` })

    expect(inserts[0]!.values.startedReason).toBe(`schedule`)
  })

  it(`links the firing automation when it targets the action (EXP-583)`, async () => {
    const AUTOMATION_ID = `44444444-4444-4444-8444-444444444444`
    selectResults.push([{ id: ACTION_ID, teamId: TEAM_ID, name: `Code review` }])
    selectResults.push([{ id: AUTOMATION_ID }]) // automations probe hit
    await caller.start({
      actionId: ACTION_ID,
      startedReason: `event`,
      automationId: AUTOMATION_ID,
    })
    expect(inserts[0]!.values.automationId).toBe(AUTOMATION_ID)

    // A stale/foreign automation id degrades to NULL instead of refusing.
    selectResults.push([{ id: ACTION_ID, teamId: TEAM_ID, name: `Code review` }])
    selectResults.push([])
    await caller.start({
      actionId: ACTION_ID,
      startedReason: `event`,
      automationId: AUTOMATION_ID,
    })
    expect(inserts[1]!.values.automationId).toBeNull()

    // automationId without a reason is a zod-level reject.
    await expect(
      caller.start({ actionId: ACTION_ID, automationId: AUTOMATION_ID })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`rejects startedReason without a real actionId (EXP-530)`, async () => {
    // No action at all, and the builtin literal — neither can automate.
    await expect(
      caller.start({ teamId: TEAM_ID, startedReason: `event` })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(
      caller.start({
        actionId: `builtin:create-action`,
        teamId: TEAM_ID,
        startedReason: `event`,
      })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    // `schedule` is the same story — only automations automate starts.
    await expect(
      caller.start({ issueId: ISSUE_ID, startedReason: `schedule` })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    expect(inserts).toHaveLength(0)
  })

  // EXP-679: a run started BY another coding session is unattended like an
  // automation, but it has no automation row and no action row behind it —
  // `agent` therefore rides EVERY subject.
  it(`accepts startedReason 'agent' on any subject (EXP-679)`, async () => {
    await caller.start({ issueId: ISSUE_ID, startedReason: `agent` })
    expect(inserts[0]!.values.startedReason).toBe(`agent`)

    await caller.start({ teamId: TEAM_ID, startedReason: `agent` })
    expect(inserts[1]!.values.startedReason).toBe(`agent`)

    await caller.start({
      actionId: `builtin:create-action`,
      teamId: TEAM_ID,
      startedReason: `agent`,
    })
    expect(inserts[2]!.values.startedReason).toBe(`agent`)

    selectResults.push([{ id: ACTION_ID, teamId: TEAM_ID, name: `Code review` }])
    await caller.start({ actionId: ACTION_ID, startedReason: `agent` })
    expect(inserts[3]!.values.startedReason).toBe(`agent`)
    expect(inserts[3]!.values.automationId).toBeNull()
  })

  it(`rejects automationId riding an agent start (EXP-679)`, async () => {
    await expect(
      caller.start({
        actionId: ACTION_ID,
        startedReason: `agent`,
        automationId: `44444444-4444-4444-8444-444444444444`,
      })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    expect(inserts).toHaveLength(0)
  })

  it(`heartbeat resurrects an agent-started run on any subject (EXP-679)`, async () => {
    selectResults.push([]) // row swept
    await caller.heartbeat({
      id: SESSION_ID,
      teamId: TEAM_ID,
      startedReason: `agent`,
    })
    expect(inserts[0]!.values.startedReason).toBe(`agent`)

    // schedule/event still need a real action row to echo.
    selectResults.push([])
    await caller.heartbeat({
      id: SESSION_ID,
      teamId: TEAM_ID,
      startedReason: `schedule`,
    })
    expect(inserts[1]!.values.startedReason).toBeNull()
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
// `running`, but the ping only ever advances updated_at (and coalesces the
// EXP-701 acked_at pickup stamp) — it can never downgrade the status — and
// an `ended` row stays final.
describe(`codingSessions.heartbeat — in_review liveness`, () => {
  it(`advances updated_at for an in_review row without touching status`, async () => {
    selectResults.push([{ userId: `actor`, status: `in_review` }])

    const result = await caller.heartbeat({ id: SESSION_ID })

    expect(result).toEqual({ alive: true })
    expect(updates).toHaveLength(1)
    expect(Object.keys(updates[0]!.values)).toEqual([`updatedAt`, `ackedAt`])
  })

  it(`advances updated_at for an in_review row without touching status`, async () => {
    selectResults.push([{ userId: `actor`, status: `in_review` }])

    const result = await caller.heartbeat({ id: SESSION_ID })

    expect(result).toEqual({ alive: true })
    expect(updates).toHaveLength(1)
    expect(Object.keys(updates[0]!.values)).toEqual([`updatedAt`, `ackedAt`])
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
    })
    expect(h.assertTeamMember).toHaveBeenCalledWith(`actor`, TEAM_ID)
    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.table).toBe(codingSessions)
    expect(inserts[0]!.values).toMatchObject({
      teamId: TEAM_ID,
      actionId: null,
      actionName: `Create action`,
      userId: `actor`,
      deviceLabel: null,
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

describe(`codingSessions — builtin chat (EXP-615)`, () => {
  const CHAT_ID = `builtin:chat`

  it(`start requires teamId alongside the literal`, async () => {
    const error = await rejectionOf(caller.start({ actionId: CHAT_ID }))
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect(inserts).toHaveLength(0)
  })

  it(`start inserts batch-shaped with actionId NULL + its own constant name`, async () => {
    const result = await caller.start({
      actionId: CHAT_ID,
      teamId: TEAM_ID,
    })
    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.values).toMatchObject({
      teamId: TEAM_ID,
      actionId: null,
      actionName: `Chat`,
      status: `running`,
    })
    expect(result.session).toMatchObject({ actionName: `Chat` })
  })

  it(`start still refuses startedReason on the builtin literal`, async () => {
    const error = await rejectionOf(
      caller.start({
        actionId: CHAT_ID,
        teamId: TEAM_ID,
        startedReason: `schedule`,
      })
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect(inserts).toHaveLength(0)
  })

  it(`heartbeat resurrects with the chat constant name`, async () => {
    selectResults.push([])
    const result = await caller.heartbeat({
      id: SESSION_ID,
      teamId: TEAM_ID,
      actionId: CHAT_ID,
      actionName: `client-sent junk`,
    })
    expect(result).toEqual({ alive: true })
    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.values).toMatchObject({
      id: SESSION_ID,
      teamId: TEAM_ID,
      actionId: null,
      actionName: `Chat`,
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

  it(`accepts a true write on in_review too (EXP-679)`, async () => {
    // The fake db can't evaluate the where clause — assert its shape. EXP-531
    // fenced `true` to `running`, but since EXP-673 a person-started run stays
    // live after its PR opens and the post-turn idle nudge there means exactly
    // "your turn now"; the refusal pinned "Working…" forever. The list-badge
    // masking lives in sessionDisplayState now.
    selectResults.push([{ userId: `actor`, status: `in_review` }])

    const result = await caller.setNeedsInput({
      id: SESSION_ID,
      needsInput: true,
    })

    expect(result).toEqual({ updated: true })
    expect(updates[0]!.values).toEqual({ needsInput: true })
    const shape = whereShape(updateWheres[0]).flat()
    expect(shape).toContain(`running`)
    expect(shape).toContain(`in_review`)
    // `ended` stays final on both values.
    expect(shape).not.toContain(`ended`)
  })

  it(`clears needs_input on every live status (EXP-531)`, async () => {
    // false still lands anywhere live, so a stale flag can always be
    // retired.
    selectResults.push([{ userId: `actor`, status: `in_review` }])

    const result = await caller.setNeedsInput({
      id: SESSION_ID,
      needsInput: false,
    })

    expect(result).toEqual({ updated: true })
    expect(updates).toHaveLength(1)
    expect(updates[0]!.values).toEqual({ needsInput: false })
    const shape = whereShape(updateWheres[0]).flat()
    expect(shape).toContain(`running`)
    expect(shape).toContain(`in_review`)
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

  it(`self-attribution stays the plain host-less path (no share lookup)`, async () => {
    await caller.start({
      issueId: ISSUE_ID,
      deviceId: DEVICE_ID,
      startedById: `actor`,
    })

    // The only select is EXP-549's device-label stamp (below) — never the
    // share verification.
    expect(selectWheres).toHaveLength(1)
    expect(inserts[0]!.values).toMatchObject({
      userId: `actor`,
      hostUserId: null,
      deviceId: DEVICE_ID,
    })
  })
})

// EXP-549: the session row carries the host machine's steer deviceId, and
// its `device_label` snapshot comes from the registry row's label (the user's
// RENAME) — resolved among the CALLER's own device rows. The sent label is
// the fallback for a start that outran the fire-and-forget `devices.register`;
// no deviceId still means no stamp at all.
describe(`codingSessions — device stamp (EXP-549)`, () => {
  it(`start stamps deviceId and prefers the registry label over the sent hostname`, async () => {
    selectResults.push([{ label: `macbook` }])

    await caller.start({
      issueId: ISSUE_ID,
      deviceId: DEVICE_ID,
      deviceLabel: `MacBook-Pro-von-Danny.local`,
    })

    expect(whereShape(selectWheres[0])).toEqual([
      `col:user_id`,
      `actor`,
      `col:device_id`,
      DEVICE_ID,
    ])
    expect(inserts[0]!.values).toMatchObject({
      deviceId: DEVICE_ID,
      deviceLabel: `macbook`,
    })
  })

  // `devices.register` is fire-and-forget on the desktop and the CLI daemon,
  // so the very first start after a launch can beat it — the sent hostname is
  // all there is, and without it the row's snapshot stays NULL forever.
  it(`start falls back to the sent label when the caller has no registry row`, async () => {
    selectResults.push([])

    await caller.start({
      teamId: TEAM_ID,
      deviceId: DEVICE_ID,
      deviceLabel: `fresh-box`,
    })

    expect(inserts[0]!.values).toMatchObject({
      deviceId: DEVICE_ID,
      deviceLabel: `fresh-box`,
    })
  })

  it(`start stamps a NULL label with no registry row and no sent label`, async () => {
    selectResults.push([])

    await caller.start({ teamId: TEAM_ID, deviceId: DEVICE_ID })

    expect(inserts[0]!.values).toMatchObject({
      deviceId: DEVICE_ID,
      deviceLabel: null,
    })
  })

  it(`start without a deviceId stamps NULL and never probes the registry`, async () => {
    await caller.start({ issueId: ISSUE_ID, deviceLabel: `old-host` })

    expect(selectWheres).toHaveLength(0)
    expect(inserts[0]!.values).toMatchObject({
      deviceId: null,
      deviceLabel: null,
    })
  })

  it(`heartbeat refreshes the device stamp on an existing row when the deviceId rides along`, async () => {
    selectResults.push([{ userId: `actor`, hostUserId: null, status: `running` }])
    selectResults.push([{ label: `macbook` }])

    const result = await caller.heartbeat({
      id: SESSION_ID,
      deviceId: DEVICE_ID,
    })

    expect(result).toEqual({ alive: true })
    expect(updates).toHaveLength(1)
    expect(updates[0]!.values).toMatchObject({
      deviceId: DEVICE_ID,
      deviceLabel: `macbook`,
    })
  })

  it(`heartbeat without a deviceId only advances updated_at + the ack`, async () => {
    selectResults.push([{ userId: `actor`, hostUserId: null, status: `running` }])

    await caller.heartbeat({ id: SESSION_ID })

    expect(Object.keys(updates[0]!.values)).toEqual([`updatedAt`, `ackedAt`])
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

// EXP-637: repo-backed action and chat runs get their own worktree + branch,
// so the row records it the same way a batch row does once its PR opens.
describe(`codingSessions — run branch + resume (EXP-637)`, () => {
  const RESUMED_FROM = `55555555-5555-4555-8555-555555555555`

  it(`stamps the branch on an action start`, async () => {
    // 1 = the resume-link lookup (the predecessor still exists), 2 = the
    // action row.
    selectResults.push([{ id: RESUMED_FROM }])
    selectResults.push([{ id: ACTION_ID, teamId: TEAM_ID, name: `Refresh` }])

    await caller.start({
      actionId: ACTION_ID,
      branch: `exp/refresh-1a2b3c4d`,
      resumedFromId: RESUMED_FROM,
    })

    expect(inserts[0]!.values).toMatchObject({
      branch: `exp/refresh-1a2b3c4d`,
      resumedFromId: RESUMED_FROM,
    })
  })

  // EXP-639: `resumed_from_id` is a real FK and the 2h idle sweep DELETES
  // stale running rows, so the desktop's run-registry record routinely names
  // a session that is gone. Inserting it raw failed with a 23503 — a raw 500
  // on the user's Resume click.
  it(`stores NULL when the resumed-from run no longer exists`, async () => {
    selectResults.push([]) // swept predecessor
    selectResults.push([{ id: ACTION_ID, teamId: TEAM_ID, name: `Refresh` }])

    await caller.start({
      actionId: ACTION_ID,
      branch: `exp/refresh-1a2b3c4d`,
      resumedFromId: RESUMED_FROM,
    })

    expect(inserts[0]!.values).toMatchObject({ resumedFromId: null })
    // Scoped by id ONLY — the link is history, never authorization.
    expect(whereShape(selectWheres[0])).toEqual([`col:id`, RESUMED_FROM])
  })

  it(`skips the lookup entirely without a resumedFromId`, async () => {
    selectResults.push([{ id: ACTION_ID, teamId: TEAM_ID, name: `Refresh` }])

    await caller.start({ actionId: ACTION_ID })

    expect(inserts[0]!.values).toMatchObject({ resumedFromId: null })
    // Only the action lookup ran.
    expect(selectWheres).toHaveLength(1)
  })

  it(`stamps the branch on a batch and a builtin start`, async () => {
    await caller.start({ teamId: TEAM_ID, branch: `exp/chat-1a2b3c4d` })
    expect(inserts[0]!.values).toMatchObject({ branch: `exp/chat-1a2b3c4d` })

    inserts.length = 0
    await caller.start({
      actionId: `builtin:chat`,
      teamId: TEAM_ID,
      branch: `exp/chat-9f8e7d6c`,
    })
    expect(inserts[0]!.values).toMatchObject({ branch: `exp/chat-9f8e7d6c` })
  })

  it(`refuses a branch on an issue start — the issue owns its branch`, async () => {
    const error = await rejectionOf(
      caller.start({ issueId: ISSUE_ID, branch: `exp/MET-12` })
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect(inserts).toHaveLength(0)
  })

  it(`carries the branch through a heartbeat re-create`, async () => {
    // No existing row → the re-create path.
    selectResults.push([])

    const result = await caller.heartbeat({
      id: SESSION_ID,
      teamId: TEAM_ID,
      actionId: ACTION_ID,
      actionName: `Refresh`,
      branch: `exp/refresh-1a2b3c4d`,
    })

    expect(result).toEqual({ alive: true })
    expect(inserts[0]!.values).toMatchObject({
      id: SESSION_ID,
      branch: `exp/refresh-1a2b3c4d`,
    })
  })

  it(`refuses a branch on an issue-scoped heartbeat`, async () => {
    const error = await rejectionOf(
      caller.heartbeat({
        id: SESSION_ID,
        issueId: ISSUE_ID,
        branch: `exp/MET-12`,
      })
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
  })
})

// EXP-637: `end` is the CLIENT end path (agent exit, tab close, quit) and now
// says so on the row. It never writes a summary — only the agent's own
// exponential_sessions_end does.
describe(`codingSessions.end — endedBy stamp (EXP-637)`, () => {
  it(`stamps endedBy client and clears needsInput`, async () => {
    selectResults.push([
      { id: SESSION_ID, userId: `actor`, hostUserId: null, status: `running` },
    ])

    await caller.end({ id: SESSION_ID })

    expect(updates).toHaveLength(1)
    expect(updates[0]!.values).toMatchObject({
      status: `ended`,
      endedBy: `client`,
      needsInput: false,
    })
    expect(`summary` in updates[0]!.values).toBe(false)
    // The write is fenced to a still-live row (see the race case below).
    expect(whereShape(updateWheres[0])).toEqual([
      `col:id`,
      SESSION_ID,
      `col:status`,
      `ended`,
    ])
    // EXP-700: a vanished agent-started child must not leave its parent
    // waiting — the (internally best-effort) notify runs on every real end.
    expect(notifyParentOfChildEnd).toHaveBeenCalledWith(
      expect.anything(),
      SESSION_ID,
      { summary: null, endedBy: `client` }
    )
  })

  // EXP-700: the agent's own `exponential_sessions_end` fires moments before
  // the process exits, so the read above can still see `running` while the
  // close-out commits. The fenced update then matches nothing: the row keeps
  // its summary and `endedBy: agent`, and the parent is told once (by the
  // close-out), never a second "ended without a report (client)".
  it(`leaves a close-out that won the race alone and does not re-notify`, async () => {
    selectResults.push([
      { id: SESSION_ID, userId: `actor`, hostUserId: null, status: `running` },
    ])
    updateResults.push([])
    selectResults.push([
      { id: SESSION_ID, status: `ended`, endedBy: `agent`, summary: `Done.` },
    ])

    const result = await caller.end({ id: SESSION_ID })

    expect(result.session).toMatchObject({ endedBy: `agent`, summary: `Done.` })
    expect(notifyParentOfChildEnd).not.toHaveBeenCalled()
  })

  it(`leaves an already-ended row alone`, async () => {
    selectResults.push([
      { id: SESSION_ID, userId: `actor`, hostUserId: null, status: `ended` },
    ])
    selectResults.push([{ id: SESSION_ID, status: `ended` }])

    await caller.end({ id: SESSION_ID })

    expect(updates).toHaveLength(0)
    // EXP-700: the idempotent no-op end never re-notifies.
    expect(notifyParentOfChildEnd).not.toHaveBeenCalled()
  })
})
