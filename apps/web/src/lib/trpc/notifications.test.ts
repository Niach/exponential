import { beforeEach, describe, expect, it, vi } from "vitest"
import { PgDialect } from "drizzle-orm/pg-core"

// REV2-13: the Support entry's unread badge counts issue-less `support_reply`
// rows, and NOTHING in the Support surface ever cleared them —
// markReadByIssue can't (their issue_id is NULL by construction), so the
// badge stayed lit no matter how many tickets a member answered.
// REV2-52: emailPrefs must report whether the address is verified — the
// digest sweep silently refuses unverified ones.
//
// The router runs against ctx.db, so a fake db is enough: `update()` records
// the table + the set values and captures the where condition for rendering
// back to SQL, `execute()` fakes generateTxId's probe, and `transaction()`
// hands the callback the same fake db.
const updates: Array<{ table: unknown; values: Record<string, unknown> }> = []
let capturedWhere: unknown

type FakeDb = {
  update: (table: unknown) => {
    set: (values: Record<string, unknown>) => {
      where: (cond: unknown) => Promise<void>
    }
  }
  execute: ReturnType<typeof vi.fn>
  transaction: <T>(fn: (tx: FakeDb) => Promise<T>) => Promise<T>
}

const fakeDb: FakeDb = {
  update: (table: unknown) => ({
    set: (values: Record<string, unknown>) => ({
      where: (cond: unknown) => {
        updates.push({ table, values })
        capturedWhere = cond
        return Promise.resolve()
      },
    }),
  }),
  execute: vi.fn(async () => ({ rows: [{ txid: `42` }] })),
  transaction: async (fn) => fn(fakeDb),
}

vi.mock(`@/db/connection`, () => ({ db: {} }))

const getOrCreateEmailPrefs = vi.fn(async () => ({
  emailEnabled: true,
  typePrefs: {},
  digest: `daily`,
}))
vi.mock(`@/lib/notification-prefs`, () => ({
  getOrCreateEmailPrefs: (...args: unknown[]) =>
    getOrCreateEmailPrefs(...(args as [])),
  updateEmailPrefs: vi.fn(),
}))

vi.mock(`@/lib/email-enabled`, () => ({ emailEnabled: true }))

import { notificationsRouter } from "@/lib/trpc/notifications"
import { notifications } from "@/db/schema"

const TEAM = `11111111-1111-4111-8111-111111111111`

function caller(user: Record<string, unknown> = {}) {
  return notificationsRouter.createCaller({
    session: {
      user: {
        id: `user-a`,
        email: `a@example.com`,
        emailVerified: true,
        ...user,
      },
    },
    db: fakeDb,
  } as never)
}

beforeEach(() => {
  updates.length = 0
  capturedWhere = undefined
  fakeDb.execute.mockClear()
})

describe(`notifications.markReadSupport (REV2-13)`, () => {
  it(`clears the caller's unread issue-less support rows for one team`, async () => {
    const result = await caller().markReadSupport({ teamId: TEAM })

    expect(result).toEqual({ txId: 42 })
    expect(updates).toHaveLength(1)
    expect(updates[0]!.table).toBe(notifications)
    expect(updates[0]!.values.readAt).toBeInstanceOf(Date)

    const { sql, params } = new PgDialect().sqlToQuery(capturedWhere as never)
    // Self-scoped, team-scoped, support-only, issue-less, unread-only.
    expect(params).toContain(`user-a`)
    expect(params).toContain(`support_reply`)
    expect(params).toContain(TEAM)
    expect(sql).toContain(`"issue_id" is null`)
    expect(sql).toContain(`"read_at" is null`)
  })
})

describe(`notifications.emailPrefs (REV2-52)`, () => {
  it(`reports a verified address`, async () => {
    const prefs = await caller().emailPrefs()
    expect(prefs).toMatchObject({
      transportConfigured: true,
      emailVerified: true,
      email: `a@example.com`,
    })
  })

  it(`reports an unverified address so the panel can offer a resend`, async () => {
    const prefs = await caller({ emailVerified: false }).emailPrefs()
    expect(prefs.emailVerified).toBe(false)
  })
})
