import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// EXP-56 dual-subject coding sessions: `start` takes EXACTLY ONE of
// issueId/releaseId (zod refine). The issue path denormalizes
// workspaceId/projectId from the issue's context; the release path loads the
// release, asserts membership against ITS workspace, and inserts with
// releaseId + workspaceId only — issueId/projectId must stay absent so the
// row never leaks through the anonymous project-scoped shape clause. The
// router runs against ctx.db (no transaction/generateTxId), so a fake db with
// a FIFO select queue + an insert recorder is enough.

const h = vi.hoisted(() => ({
  assertWorkspaceMember: vi.fn(
    async (..._args: unknown[]) => ({ role: `member` }) as unknown
  ),
  getIssueWorkspaceContext: vi.fn(async () => ({
    issueId: `issue-1`,
    projectId: `proj-1`,
    workspaceId: `ws-issue`,
  })),
}))

// lib/trpc.ts + lib/admin.ts import db/auth at module scope; runtime here only
// needs the exports to exist.
vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))

vi.mock(`@/lib/workspace-membership`, () => ({
  assertWorkspaceMember: h.assertWorkspaceMember,
  getIssueWorkspaceContext: h.getIssueWorkspaceContext,
}))

import { codingSessionsRouter } from "@/lib/trpc/coding-sessions"
import { codingSessions } from "@/db/schema"

const ISSUE_ID = `11111111-1111-4111-8111-111111111111`
const RELEASE_ID = `22222222-2222-4222-8222-222222222222`
const SESSION_ID = `33333333-3333-4333-8333-333333333333`

// FIFO select queue: each ctx.db.select() call resolves the next seeded rows.
const selectQueue: unknown[][] = []

function selectChain(): Promise<unknown[]> & Record<string, () => unknown> {
  const p = Promise.resolve(
    selectQueue.shift() ?? []
  ) as Promise<unknown[]> & Record<string, () => unknown>
  for (const m of [`from`, `where`, `limit`]) {
    p[m] = () => p
  }
  return p
}

const inserts: { table: unknown; values: Record<string, unknown> }[] = []

const fakeDb = {
  select: () => selectChain(),
  insert: (table: unknown) => ({
    values: (values: Record<string, unknown>) => {
      inserts.push({ table, values })
      return {
        returning: async () => [{ id: SESSION_ID, ...values }],
      }
    },
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
  selectQueue.length = 0
  inserts.length = 0
  h.assertWorkspaceMember.mockClear()
  h.assertWorkspaceMember.mockResolvedValue({ role: `member` })
  h.getIssueWorkspaceContext.mockClear()
  h.getIssueWorkspaceContext.mockResolvedValue({
    issueId: `issue-1`,
    projectId: `proj-1`,
    workspaceId: `ws-issue`,
  })
})

describe(`codingSessions.start — exactly-one-subject refine`, () => {
  it(`rejects BOTH issueId and releaseId as input validation`, async () => {
    const error = await rejectionOf(
      caller.start({ issueId: ISSUE_ID, releaseId: RELEASE_ID })
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect((error as TRPCError).message).toContain(
      `Exactly one of issueId/releaseId is required`
    )
    expect(inserts).toHaveLength(0)
    expect(h.getIssueWorkspaceContext).not.toHaveBeenCalled()
    expect(h.assertWorkspaceMember).not.toHaveBeenCalled()
  })

  it(`rejects NEITHER id as input validation`, async () => {
    const error = await rejectionOf(caller.start({}))
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect((error as TRPCError).message).toContain(
      `Exactly one of issueId/releaseId is required`
    )
    expect(inserts).toHaveLength(0)
    expect(h.assertWorkspaceMember).not.toHaveBeenCalled()
  })
})

describe(`codingSessions.start — issue path`, () => {
  it(`inserts with issueId + denormalized workspaceId/projectId after asserting membership`, async () => {
    const result = await caller.start({
      issueId: ISSUE_ID,
      deviceLabel: `MacBook`,
    })

    expect(h.getIssueWorkspaceContext).toHaveBeenCalledWith(ISSUE_ID)
    expect(h.assertWorkspaceMember).toHaveBeenCalledWith(`actor`, `ws-issue`)
    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.table).toBe(codingSessions)
    expect(inserts[0]!.values).toEqual({
      issueId: ISSUE_ID,
      workspaceId: `ws-issue`,
      projectId: `proj-1`,
      userId: `actor`,
      deviceLabel: `MacBook`,
      status: `running`,
    })
    // Issue-scoped rows never claim a release.
    expect(`releaseId` in inserts[0]!.values).toBe(false)
    expect(result.session).toMatchObject({ id: SESSION_ID, issueId: ISSUE_ID })
  })

  it(`refuses a non-member of the issue's workspace before inserting`, async () => {
    h.assertWorkspaceMember.mockRejectedValueOnce(
      new TRPCError({ code: `FORBIDDEN` })
    )

    const error = await rejectionOf(caller.start({ issueId: ISSUE_ID }))
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`FORBIDDEN`)
    expect(inserts).toHaveLength(0)
  })
})

describe(`codingSessions.start — release path`, () => {
  it(`inserts with releaseId + the release's workspaceId and NO issueId/projectId`, async () => {
    selectQueue.push([{ id: RELEASE_ID, workspaceId: `ws-release` }])

    const result = await caller.start({
      releaseId: RELEASE_ID,
      deviceLabel: `MacBook`,
    })

    // Membership is asserted against the RELEASE's workspace.
    expect(h.assertWorkspaceMember).toHaveBeenCalledWith(`actor`, `ws-release`)
    expect(h.getIssueWorkspaceContext).not.toHaveBeenCalled()
    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.table).toBe(codingSessions)
    expect(inserts[0]!.values).toEqual({
      releaseId: RELEASE_ID,
      workspaceId: `ws-release`,
      userId: `actor`,
      deviceLabel: `MacBook`,
      status: `running`,
    })
    // A release run spans projects: issue_id/project_id must be ABSENT so the
    // populate triggers no-op and the anonymous project-scoped shape clause
    // can never match the row.
    expect(`issueId` in inserts[0]!.values).toBe(false)
    expect(`projectId` in inserts[0]!.values).toBe(false)
    expect(result.session).toMatchObject({
      id: SESSION_ID,
      releaseId: RELEASE_ID,
    })
  })

  it(`throws NOT_FOUND for an unknown release`, async () => {
    // Empty select queue → no release row.
    const error = await rejectionOf(caller.start({ releaseId: RELEASE_ID }))
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`NOT_FOUND`)
    expect((error as TRPCError).message).toBe(`Release not found`)
    expect(h.assertWorkspaceMember).not.toHaveBeenCalled()
    expect(inserts).toHaveLength(0)
  })

  it(`refuses a non-member of the release's workspace before inserting`, async () => {
    selectQueue.push([{ id: RELEASE_ID, workspaceId: `ws-release` }])
    h.assertWorkspaceMember.mockRejectedValueOnce(
      new TRPCError({ code: `FORBIDDEN` })
    )

    const error = await rejectionOf(caller.start({ releaseId: RELEASE_ID }))
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`FORBIDDEN`)
    expect(inserts).toHaveLength(0)
  })

  it(`nulls an omitted deviceLabel`, async () => {
    selectQueue.push([{ id: RELEASE_ID, workspaceId: `ws-release` }])

    await caller.start({ releaseId: RELEASE_ID })

    expect(inserts[0]!.values.deviceLabel).toBeNull()
  })
})
