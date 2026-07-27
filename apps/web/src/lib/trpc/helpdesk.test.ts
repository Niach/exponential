import { beforeEach, describe, expect, it, vi } from "vitest"
import { PgDialect } from "drizzle-orm/pg-core"

// REV2-58: replying to a RESOLVED ticket must reopen it — closing revoked the
// magic link, so the reply email otherwise invited the reporter to a page
// that refuses their answer.
// REV2-40: listThreads is paged (the Resolved tab grows forever and four
// clients poll it every 30s).
// REV2-10: getThread carries each message's email delivery status so the
// inbox can mark a reply the reporter never received.
//
// The router runs against ctx.db, so a fake db is enough: `select()` shifts
// pre-seeded rows off a FIFO queue and records the where/limit it was given,
// `insert().returning()` shifts its own queue, `update()` records the table +
// values, `execute()` fakes generateTxId's probe, and `transaction()` hands
// the callback the same fake db.
const h = vi.hoisted(() => {
  const selectQueue: unknown[][] = []
  const insertQueue: unknown[][] = []
  const updates: Array<{ table: unknown; values: Record<string, unknown> }> = []
  const state: { where: unknown; limit: unknown } = {
    where: undefined,
    limit: undefined,
  }

  function selectChain() {
    const p = Promise.resolve(selectQueue.shift() ?? []) as Promise<unknown[]> &
      Record<string, (arg?: unknown) => unknown>
    p.from = () => p
    p.leftJoin = () => p
    p.innerJoin = () => p
    p.orderBy = () => p
    p.where = (cond?: unknown) => {
      state.where = cond
      return p
    }
    p.limit = (n?: unknown) => {
      state.limit = n
      return p
    }
    return p
  }

  const db: Record<string, unknown> = {
    select: () => selectChain(),
    insert: () => ({
      values: () => {
        const p = Promise.resolve() as Promise<void> & {
          returning: () => Promise<unknown[]>
        }
        p.returning = () => Promise.resolve(insertQueue.shift() ?? [])
        return p
      },
    }),
    update: (table: unknown) => ({
      set: (values: Record<string, unknown>) => ({
        where: () => {
          updates.push({ table, values })
          return Promise.resolve()
        },
      }),
    }),
    execute: vi.fn(async () => ({ rows: [{ txid: `7` }] })),
  }
  db.transaction = async (fn: (tx: unknown) => Promise<unknown>) => fn(db)

  return { selectQueue, insertQueue, updates, state, db }
})

vi.mock(`@/db/connection`, () => ({ db: h.db }))

const assertTeamMember = vi.fn(async () => undefined)
vi.mock(`@/lib/team-membership`, () => ({
  assertTeamMember: (...args: unknown[]) => assertTeamMember(...(args as [])),
  getBoardTeamId: vi.fn(async () => ({ teamId: `team-1` })),
  getSoleHumanMemberId: vi.fn(async () => null),
}))

vi.mock(`@/lib/integrations/subscriptions`, () => ({
  ensureSubscribed: vi.fn(async () => undefined),
}))

const sendSupportReplyEmail = vi.fn(async () => ({
  delivered: true,
  provider: `ses`,
  messageId: `m-1`,
}))
vi.mock(`@/lib/email`, () => ({
  sendSupportReplyEmail: (...args: unknown[]) =>
    sendSupportReplyEmail(...(args as [])),
  deliveryStatus: () => `sent`,
}))

vi.mock(`@/lib/helpdesk/token`, () => ({
  mintSupportToken: () => `tok`,
}))

const reopenThreadInTx = vi.fn(async () => undefined)
vi.mock(`@/lib/helpdesk/service`, () => ({
  MAX_SUPPORT_MESSAGE_CHARS: 10_000,
  closeThreadInTx: vi.fn(async () => undefined),
  latestMessagesByThread: vi.fn(async () => new Map()),
  reopenThreadInTx: (...args: unknown[]) => reopenThreadInTx(...(args as [])),
  supportThreadUrl: (token: string) => `https://app.test/support/${token}`,
}))

import { helpdeskRouter } from "@/lib/trpc/helpdesk"
import { supportThreads } from "@/db/schema"

const TEAM = `11111111-1111-4111-8111-111111111111`
const THREAD = `22222222-2222-4222-8222-222222222222`

function caller() {
  return helpdeskRouter.createCaller({
    session: { user: { id: `user-a` } },
    db: h.db,
  } as never)
}

const thread = (over: Record<string, unknown> = {}) => ({
  id: THREAD,
  teamId: TEAM,
  status: `open`,
  title: `Broken login`,
  reporterEmail: `reporter@example.com`,
  reporterName: null,
  linkedIssueId: null,
  tokenRevokedAt: null,
  // Engagement gate: the reporter has opened their link, and isn't watching
  // right now — so a reply email is actually attempted.
  lastReporterSeenAt: new Date(`2020-01-01T00:00:00.000Z`),
  ...over,
})

beforeEach(() => {
  h.selectQueue.length = 0
  h.insertQueue.length = 0
  h.updates.length = 0
  h.state.where = undefined
  h.state.limit = undefined
  reopenThreadInTx.mockClear()
  sendSupportReplyEmail.mockClear()
  assertTeamMember.mockClear()
})

describe(`helpdesk.reply auto-reopen (REV2-58)`, () => {
  it(`reopens a resolved thread so the emailed link works again`, async () => {
    h.selectQueue.push([thread({ status: `resolved` })]) // loadThreadForMember
    h.insertQueue.push([{ id: `msg-1` }]) // the outbound message
    h.selectQueue.push([{ name: `Acme` }]) // team name for the email
    h.insertQueue.push([{ id: `delivery-1` }]) // the ledger row

    const result = await caller().reply({ threadId: THREAD, body: `On it!` })

    expect(result.reopened).toBe(true)
    expect(reopenThreadInTx).toHaveBeenCalledTimes(1)
    // No bare updatedAt bump — reopen owns the write (and un-revokes the
    // token, which is the whole point).
    expect(
      h.updates.filter((u) => u.table === supportThreads)
    ).toHaveLength(0)
    expect(sendSupportReplyEmail).toHaveBeenCalledTimes(1)
  })

  it(`leaves an open thread open and just bumps its activity`, async () => {
    h.selectQueue.push([thread()])
    h.insertQueue.push([{ id: `msg-1` }])
    h.selectQueue.push([{ name: `Acme` }])
    h.insertQueue.push([{ id: `delivery-1` }])

    const result = await caller().reply({ threadId: THREAD, body: `On it!` })

    expect(result.reopened).toBe(false)
    expect(reopenThreadInTx).not.toHaveBeenCalled()
    const bump = h.updates.find((u) => u.table === supportThreads)
    expect(bump?.values.updatedAt).toBeInstanceOf(Date)
  })
})

describe(`helpdesk.listThreads paging (REV2-40)`, () => {
  it(`bounds the query with the default page size`, async () => {
    h.selectQueue.push([])
    await caller().listThreads({ teamId: TEAM })
    expect(h.state.limit).toBe(50)
  })

  it(`applies the cursor as a strict updatedAt bound`, async () => {
    h.selectQueue.push([])
    await caller().listThreads({
      teamId: TEAM,
      limit: 10,
      cursor: `2026-07-20T12:00:00.000Z` as never,
    })
    expect(h.state.limit).toBe(10)
    const { sql } = new PgDialect().sqlToQuery(h.state.where as never)
    expect(sql).toContain(`"updated_at" <`)
  })

  it(`gates on membership before reading anything`, async () => {
    h.selectQueue.push([])
    await caller().listThreads({ teamId: TEAM })
    expect(assertTeamMember).toHaveBeenCalledWith(`user-a`, TEAM)
  })
})

describe(`helpdesk.getThread delivery status (REV2-10)`, () => {
  it(`carries each message's email delivery status`, async () => {
    h.selectQueue.push([thread()]) // loadThreadForMember
    h.selectQueue.push([
      {
        message: { id: `msg-1`, direction: `outbound`, body: `hi` },
        emailDeliveryStatus: `failed`,
      },
    ])

    const result = await caller().getThread({ threadId: THREAD })

    expect(result.messages).toEqual([
      { id: `msg-1`, direction: `outbound`, body: `hi`, emailDeliveryStatus: `failed` },
    ])
  })
})
