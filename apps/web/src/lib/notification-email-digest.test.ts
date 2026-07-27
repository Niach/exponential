import { beforeEach, describe, expect, it, vi } from "vitest"

// The DB+transport shell of the digest sweep. The pure planning core is
// covered in notification-email-policy.test.ts; what is exercised here is
// what the shell DOES with a plan:
//   REV2-39 — bounded send concurrency, and a throttle blip must not buy the
//             user a 22h backoff (which, for rows already ≥2h old, meant the
//             digest was never sent at all).
//   REV2-52 — an unverified address defers its rows instead of claiming them.
//   REV2-51 — issue-less support_reply items link to the team Support inbox.
//
// The fake db is chainable and thenable: every builder method returns the
// chain, awaiting it resolves the next queued result, and each terminal
// operation is recorded.

type Op = {
  kind: `select` | `update` | `insert` | `delete`
  table?: unknown
  values?: Record<string, unknown>
}

const h = vi.hoisted(() => {
  const ops: Array<{
    kind: string
    table?: unknown
    values?: Record<string, unknown>
  }> = []
  const selectResults: unknown[][] = []
  const updateReturning: unknown[][] = []
  const insertReturning: unknown[][] = []

  function chain(result: () => Promise<unknown[]>) {
    const target: Record<string, unknown> = {}
    for (const method of [
      `from`,
      `innerJoin`,
      `leftJoin`,
      `where`,
      `orderBy`,
      `groupBy`,
      `limit`,
    ]) {
      target[method] = () => target
    }
    target.returning = () => result()
    ;(target as { then: unknown }).then = (
      resolve: (v: unknown) => unknown,
      reject: (e: unknown) => unknown
    ) => result().then(resolve, reject)
    return target
  }

  const db = {
    select: () => {
      ops.push({ kind: `select` })
      return chain(async () => selectResults.shift() ?? [])
    },
    update: (table: unknown) => ({
      set: (values: Record<string, unknown>) => {
        ops.push({ kind: `update`, table, values })
        return chain(async () => updateReturning.shift() ?? [])
      },
    }),
    insert: (table: unknown) => ({
      values: (values: Record<string, unknown>) => {
        ops.push({ kind: `insert`, table, values })
        return chain(async () => insertReturning.shift() ?? [])
      },
    }),
    delete: (table: unknown) => {
      ops.push({ kind: `delete`, table })
      return chain(async () => [])
    },
  }

  return { ops, selectResults, updateReturning, insertReturning, db }
})

const ops = h.ops as Op[]
const { selectResults, updateReturning, insertReturning } = h

vi.mock(`@/db/connection`, () => ({ db: h.db }))
vi.mock(`@/lib/email-enabled`, () => ({ emailEnabled: true }))

const sendNotificationDigestEmail = vi.fn(async (_args: unknown) => ({
  delivered: true,
  provider: `ses`,
  messageId: `m-1`,
  suppressed: false,
}))
vi.mock(`@/lib/email`, () => ({
  sendNotificationDigestEmail: (args: unknown) =>
    sendNotificationDigestEmail(args),
  deliveryStatus: (result: { delivered: boolean; suppressed?: boolean }) =>
    result.delivered ? `sent` : result.suppressed ? `suppressed` : `failed`,
}))

const getEmailPrefsMap = vi.fn(async (userIds: string[]) => {
  const map = new Map<string, unknown>()
  for (const id of userIds) {
    map.set(id, {
      emailEnabled: true,
      typePrefs: {},
      digest: `daily`,
      unsubscribeToken: `tok-${id}`,
    })
  }
  return map
})
vi.mock(`@/lib/notification-prefs`, () => ({
  getEmailPrefsMap: (userIds: string[]) => getEmailPrefsMap(userIds),
}))

import { runEmailDigestSweep } from "@/lib/notification-email-digest"
import { emailDeliveries, notifications } from "@/db/schema"

const NOW = new Date(`2026-07-20T12:00:00.000Z`)
const TWO_HOURS_AGO = new Date(NOW.getTime() - 2 * 60 * 60 * 1000)

const row = (over: Partial<Record<string, unknown>> = {}) => ({
  notificationId: `n-1`,
  userId: `user-1`,
  type: `issue_comment`,
  title: `A comment`,
  body: `hello`,
  createdAt: TWO_HOURS_AGO,
  readAt: null,
  email: `user-1@example.com`,
  emailVerified: true,
  issueIdentifier: `EXP-1`,
  teamSlug: `acme`,
  boardSlug: `board`,
  notificationTeamSlug: null,
  isMember: true,
  ...over,
})

// scan rows, then the lastSent/lastFailed aggregate, then the claim's
// `.returning()`.
function seed(rows: ReturnType<typeof row>[]) {
  selectResults.push(rows, [])
  updateReturning.push(rows.map((r) => ({ id: r.notificationId })))
  insertReturning.push(
    ...rows.map((_, index) => [{ id: `ledger-${index}` }])
  )
}

beforeEach(() => {
  ops.length = 0
  selectResults.length = 0
  updateReturning.length = 0
  insertReturning.length = 0
  sendNotificationDigestEmail.mockClear()
  sendNotificationDigestEmail.mockResolvedValue({
    delivered: true,
    provider: `ses`,
    messageId: `m-1`,
    suppressed: false,
  })
})

describe(`digest deep links (REV2-51)`, () => {
  it(`links issue-less support_reply items to the team Support inbox`, async () => {
    seed([
      row({
        notificationId: `n-support`,
        type: `support_reply`,
        issueIdentifier: null,
        teamSlug: null,
        boardSlug: null,
        notificationTeamSlug: `acme`,
      }),
    ])

    await runEmailDigestSweep(NOW)

    const args = sendNotificationDigestEmail.mock.calls[0][0] as {
      items: Array<{ url: string | null }>
    }
    expect(args.items[0].url).toMatch(/\/t\/acme\/support$/)
  })

  it(`still prefers the issue deep link when the row has one`, async () => {
    seed([row({ notificationTeamSlug: `acme` })])
    await runEmailDigestSweep(NOW)
    const args = sendNotificationDigestEmail.mock.calls[0][0] as {
      items: Array<{ url: string | null }>
    }
    expect(args.items[0].url).toMatch(/\/boards\/board\/issues\/EXP-1$/)
  })
})

describe(`unverified recipients (REV2-52)`, () => {
  it(`defers their rows instead of claiming them away forever`, async () => {
    selectResults.push([row({ emailVerified: false })], [])
    updateReturning.push([])

    const result = await runEmailDigestSweep(NOW)

    expect(result).toEqual({ emailsSent: 0, notificationsClaimed: 0 })
    // Nothing was claimed and nothing was sent — verifying inside the 24h
    // backstop still digests these.
    expect(ops.some((op) => op.kind === `update`)).toBe(false)
    expect(sendNotificationDigestEmail).not.toHaveBeenCalled()
  })

  it(`still claims rows the recipient can never receive (no address / no membership)`, async () => {
    selectResults.push(
      [row({ notificationId: `n-gone`, isMember: false })],
      []
    )
    updateReturning.push([{ id: `n-gone` }])

    const result = await runEmailDigestSweep(NOW)

    expect(result.notificationsClaimed).toBe(1)
    const claim = ops.find(
      (op) => op.kind === `update` && op.table === notifications
    )
    expect(claim?.values?.emailedAt).toEqual(NOW)
    expect(sendNotificationDigestEmail).not.toHaveBeenCalled()
  })
})

describe(`send fan-out (REV2-39)`, () => {
  it(`never exceeds the bounded concurrency`, async () => {
    const rows = Array.from({ length: 12 }, (_, i) =>
      row({ notificationId: `n-${i}`, userId: `user-${i}`, email: `u${i}@x.io` })
    )
    seed(rows)

    let inFlight = 0
    let peak = 0
    sendNotificationDigestEmail.mockImplementation(async () => {
      inFlight += 1
      peak = Math.max(peak, inFlight)
      await new Promise((resolve) => setTimeout(resolve, 1))
      inFlight -= 1
      return {
        delivered: true,
        provider: `ses`,
        messageId: `m`,
        suppressed: false,
      }
    })

    const result = await runEmailDigestSweep(NOW)

    expect(sendNotificationDigestEmail).toHaveBeenCalledTimes(12)
    expect(result.emailsSent).toBe(12)
    expect(peak).toBeLessThanOrEqual(5)
    expect(peak).toBeGreaterThan(1)
  })

  it(`a THROTTLED send drops its ledger row so the next sweep retries`, async () => {
    seed([row()])
    sendNotificationDigestEmail.mockRejectedValue(
      Object.assign(new Error(`Maximum sending rate exceeded`), {
        name: `ThrottlingException`,
      })
    )

    await runEmailDigestSweep(NOW)

    // No `failed` ledger row survives to feed the 22h backoff…
    expect(
      ops.some(
        (op) => op.kind === `update` && op.table === emailDeliveries
      )
    ).toBe(false)
    expect(
      ops.some((op) => op.kind === `delete` && op.table === emailDeliveries)
    ).toBe(true)
    // …and the notifications are un-claimed for the next sweep.
    const unclaim = ops.filter(
      (op) =>
        op.kind === `update` &&
        op.table === notifications &&
        op.values?.emailedAt === null
    )
    expect(unclaim).toHaveLength(1)
  })

  it(`a PERSISTENT send failure keeps the failed ledger row (daily backoff)`, async () => {
    seed([row()])
    sendNotificationDigestEmail.mockRejectedValue(
      Object.assign(new Error(`Email address is not verified in SES`), {
        name: `MessageRejected`,
      })
    )

    await runEmailDigestSweep(NOW)

    const ledgerUpdate = ops.find(
      (op) => op.kind === `update` && op.table === emailDeliveries
    )
    expect(ledgerUpdate?.values?.status).toBe(`failed`)
    expect(
      ops.some((op) => op.kind === `delete` && op.table === emailDeliveries)
    ).toBe(false)
  })
})
