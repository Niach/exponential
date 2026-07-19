import { beforeEach, describe, expect, it, vi } from "vitest"

// workspaces.delete must never delete the user's LAST personal workspace
// (EXP-82) — the EXP-43 ensureDefault self-heal would recreate it with a
// fresh id/slug on some clients and not others, which reads as data
// corruption. The router runs against ctx.db, so a fake db object is enough —
// `select()` shifts pre-seeded rows off a FIFO queue, `delete()` records the
// drizzle table object, `execute()` fakes generateTxId's
// `SELECT pg_current_xact_id()` probe, and `transaction()` hands the callback
// the same fake db. `findOtherPersonalMembership` is deliberately NOT mocked
// so the guard's real query runs against the queue.
const selectQueue: unknown[][] = []

function selectChain(): Promise<unknown[]> & Record<string, () => unknown> {
  const p = Promise.resolve(
    selectQueue.shift() ?? []
  ) as Promise<unknown[]> & Record<string, () => unknown>
  for (const m of [`from`, `where`, `innerJoin`, `limit`]) {
    p[m] = () => p
  }
  return p
}

const deletes: { table: unknown }[] = []

type FakeDb = {
  select: () => ReturnType<typeof selectChain>
  delete: (table: unknown) => { where: () => Promise<void> }
  execute: ReturnType<typeof vi.fn>
  transaction: <T>(fn: (tx: FakeDb) => Promise<T>) => Promise<T>
}

const fakeDb: FakeDb = {
  select: () => selectChain(),
  delete: (table: unknown) => ({
    where: () => {
      deletes.push({ table })
      return Promise.resolve()
    },
  }),
  execute: vi.fn(async () => ({ rows: [{ txid: `42` }] })),
  transaction: async (fn) => fn(fakeDb),
}

// `@/lib/trpc` (imported by the router) pulls in the real connection module;
// keep Postgres out of the test.
vi.mock(`@/db/connection`, () => ({ db: {} }))

vi.mock(`@/lib/admin`, () => ({
  isUserAdmin: vi.fn(async () => false),
  assertAdmin: vi.fn(async () => {}),
}))

vi.mock(`@/lib/workspace-membership`, () => ({
  assertWorkspaceOwner: vi.fn(async () => ({ role: `owner` })),
  getWorkspaceMember: vi.fn(async () => null),
}))

vi.mock(`@/lib/billing`, () => ({
  assertCanUseHelpdesk: vi.fn(async () => {}),
}))

const FEEDBACK_WS = `99999999-9999-4999-8999-999999999999`
vi.mock(`@/lib/bootstrap-cloud`, () => ({
  getFeedbackWorkspaceId: vi.fn(async () => FEEDBACK_WS),
}))

const cancelCreemSubscriptionsBestEffort = vi.fn(async () => {})
vi.mock(`@/lib/billing/creem-subscriptions`, () => ({
  findActiveSubscriptionsForWorkspaces: vi.fn(async () => []),
  cancelCreemSubscriptionsBestEffort: (...args: unknown[]) =>
    cancelCreemSubscriptionsBestEffort(...(args as [])),
}))

const deleteStorageObjects = vi.fn(async () => {})
vi.mock(`@/lib/storage/issue-attachment-cleanup`, () => ({
  deleteStorageObjects: (...args: unknown[]) =>
    deleteStorageObjects(...(args as [])),
}))

import { workspacesRouter } from "@/lib/trpc/workspaces"
import { workspaces } from "@/db/schema"

const WS = `11111111-1111-4111-8111-111111111111`
const OTHER_WS = `22222222-2222-4222-8222-222222222222`

function caller() {
  return workspacesRouter.createCaller({
    session: { user: { id: `user-a`, name: `User A` } },
    db: fakeDb,
  } as never)
}

beforeEach(() => {
  selectQueue.length = 0
  deletes.length = 0
  fakeDb.execute.mockClear()
  cancelCreemSubscriptionsBestEffort.mockClear()
  deleteStorageObjects.mockClear()
})

describe(`workspaces.delete — last-personal-workspace guard (EXP-82)`, () => {
  it(`refuses to delete the user's only personal workspace`, async () => {
    // findOtherPersonalMembership: no membership besides the doomed workspace.
    selectQueue.push([])

    await expect(caller().delete({ workspaceId: WS })).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
    })
    expect(deletes).toHaveLength(0)
    expect(cancelCreemSubscriptionsBestEffort).not.toHaveBeenCalled()
    expect(deleteStorageObjects).not.toHaveBeenCalled()
  })

  it(`deletes when another personal membership survives, reclaiming storage`, async () => {
    // findOtherPersonalMembership → a surviving membership.
    selectQueue.push([{ workspaceId: OTHER_WS }])
    // attachments storage-key collection inside the tx.
    selectQueue.push([{ storageKey: `attachments/a.png` }])

    const result = await caller().delete({ workspaceId: WS })

    expect(result).toEqual({ ok: true, txId: 42 })
    expect(deletes).toHaveLength(1)
    expect(deletes[0]!.table).toBe(workspaces)
    expect(deleteStorageObjects).toHaveBeenCalledWith([`attachments/a.png`])
  })

  it(`still refuses to delete the bootstrap feedback workspace`, async () => {
    await expect(
      caller().delete({ workspaceId: FEEDBACK_WS })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    expect(deletes).toHaveLength(0)
  })
})
