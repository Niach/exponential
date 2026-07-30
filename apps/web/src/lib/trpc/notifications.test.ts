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

// Rows the next `update(...).returning()` / `select(...)` resolves to.
let updateRows: unknown[] = []
let selectRows: unknown[] = []

type Returnable = Promise<unknown[]> & { returning: () => Promise<unknown[]> }

type FakeDb = {
  update: (table: unknown) => {
    set: (values: Record<string, unknown>) => {
      where: (cond: unknown) => Returnable
    }
  }
  select: (columns?: unknown) => {
    from: (table: unknown) => { where: (cond: unknown) => Promise<unknown[]> }
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
        return Object.assign(Promise.resolve(updateRows), {
          returning: async () => updateRows,
        })
      },
    }),
  }),
  select: () => ({
    from: () => ({
      where: (cond: unknown) => {
        capturedWhere = cond
        return Promise.resolve(selectRows)
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
  digestHour: 8,
}))
const updateEmailPrefs = vi.fn(
  async (_userId: string, patch: Record<string, unknown>) => ({
    emailEnabled: true,
    typePrefs: {},
    digest: `daily`,
    digestHour: 8,
    ...patch,
  })
)
vi.mock(`@/lib/notification-prefs`, () => ({
  getOrCreateEmailPrefs: (...args: unknown[]) =>
    getOrCreateEmailPrefs(...(args as [])),
  updateEmailPrefs: (...args: unknown[]) =>
    updateEmailPrefs(...(args as [string, Record<string, unknown>])),
}))

vi.mock(`@/lib/email-enabled`, () => ({ emailEnabled: true }))

import { notificationsRouter } from "@/lib/trpc/notifications"
import { usersRouter } from "@/lib/trpc/users"
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
  updateEmailPrefs.mockClear()
  updateRows = []
  selectRows = []
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

// EXP-369: the daily digest fires at a user-chosen LOCAL hour. Full hours only
// — the sweep runs every 10 minutes and resolves nothing finer.
describe(`notifications digestHour (EXP-369)`, () => {
  it(`reports the stored send hour`, async () => {
    const prefs = await caller().emailPrefs()
    expect(prefs.digestHour).toBe(8)
  })

  it(`persists a full hour and echoes it back`, async () => {
    const prefs = await caller().updateEmailPrefs({ digestHour: 21 })
    expect(updateEmailPrefs).toHaveBeenCalledWith(`user-a`, { digestHour: 21 })
    expect(prefs.digestHour).toBe(21)
    // The boundaries are valid too.
    await caller().updateEmailPrefs({ digestHour: 0 })
    await caller().updateEmailPrefs({ digestHour: 23 })
  })

  it(`rejects out-of-range and fractional hours`, async () => {
    for (const digestHour of [24, -1, 7.5]) {
      await expect(caller().updateEmailPrefs({ digestHour })).rejects.toThrow()
    }
    expect(updateEmailPrefs).not.toHaveBeenCalled()
  })
})

// EXP-369: the digest send hour is a LOCAL hour, so the account needs a zone.
// Clients claim it best-effort at login (onlyIfUnset); the account panel sets
// it explicitly. Lives here beside the digestHour cases it exists to serve.
describe(`users.setTimezone (EXP-369)`, () => {
  function usersCaller() {
    return usersRouter.createCaller({
      session: { user: { id: `user-a`, email: `a@example.com` } },
      db: fakeDb,
    } as never)
  }

  it(`reads back the stored zone (null when never captured)`, async () => {
    selectRows = [{ timezone: `Europe/Berlin` }]
    expect(await usersCaller().timezone()).toEqual({
      timezone: `Europe/Berlin`,
    })
    selectRows = []
    expect(await usersCaller().timezone()).toEqual({ timezone: null })
  })

  it(`an explicit pick overwrites whatever is stored`, async () => {
    updateRows = [{ id: `user-a` }]
    const result = await usersCaller().setTimezone({
      timezone: `America/New_York`,
    })

    expect(result).toEqual({ saved: true })
    expect(updates[0]!.values.timezone).toBe(`America/New_York`)
    const { sql } = new PgDialect().sqlToQuery(capturedWhere as never)
    expect(sql).not.toContain(`"timezone" is null`)
  })

  it(`onlyIfUnset never overwrites a zone the user already has`, async () => {
    updateRows = []
    const result = await usersCaller().setTimezone({
      timezone: `Europe/Berlin`,
      onlyIfUnset: true,
    })

    // No row matched → the claim was a no-op, which is the whole point.
    expect(result).toEqual({ saved: false })
    const { sql } = new PgDialect().sqlToQuery(capturedWhere as never)
    expect(sql).toContain(`"timezone" is null`)
  })

  it(`rejects a zone Intl does not know`, async () => {
    await expect(
      usersCaller().setTimezone({ timezone: `Mars/Olympus_Mons` })
    ).rejects.toThrow(/Unknown timezone/)
    // Nothing was written.
    expect(updates).toHaveLength(0)
  })
})
