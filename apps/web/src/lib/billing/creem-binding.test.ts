import { describe, expect, it, vi, beforeEach, afterEach } from "vitest"

// Record every db.update(...).set(...).where(...).returning() and
// db.insert(...).values(...).onConflictDoUpdate(...) chain so the default
// commit path can be asserted without a real Postgres connection, and serve
// rows to the default duplicate-guard read. `updateReturning` is what the
// update's RETURNING reports — an empty array means "no row keyed by this
// creemSubscriptionId yet", which sends the commit down its insert path
// (REV-12). `vi.hoisted` lets the mock factory (hoisted above imports) share
// these recorders with the tests.
const { updateCalls, insertCalls, selectRows, updateReturning } = vi.hoisted(
  () => ({
    updateCalls: [] as Array<{
      table: unknown
      values: unknown
      where: unknown
    }>,
    insertCalls: [] as Array<{
      table: unknown
      values: Record<string, unknown>
      conflict: { target: unknown; set: unknown }
    }>,
    selectRows: [] as unknown[],
    updateReturning: [] as unknown[],
  })
)

vi.mock(`@/db/connection`, () => ({
  db: {
    update: (table: unknown) => ({
      set: (values: unknown) => ({
        where: (where: unknown) => {
          updateCalls.push({ table, values, where })
          return { returning: async () => [...updateReturning] }
        },
      }),
    }),
    insert: (table: unknown) => ({
      values: (values: Record<string, unknown>) => ({
        onConflictDoUpdate: async (conflict: {
          target: unknown
          set: unknown
        }) => {
          insertCalls.push({ table, values, conflict })
        },
      }),
    }),
    select: () => ({
      from: () => ({
        where: async () => selectRows,
      }),
    }),
  },
}))

vi.mock(`@/lib/bootstrap-cloud`, () => ({
  isCloudInstance: () => true,
}))

import {
  extractTeamBinding,
  bindSubscriptionToTeam,
  bindingInputFromCheckout,
  bindingInputFromSubscription,
  pickDuplicateSubscriptionsToCancel,
  statusClearsPendingCancel,
  type TeamBoundSubscription,
} from "./creem-binding"

const WS = `11111111-1111-1111-1111-111111111111`
const SUB = `sub_abc123`
const OTHER_SUB = `sub_older999`

function row(overrides: Partial<TeamBoundSubscription>): TeamBoundSubscription {
  return {
    id: `row_${overrides.creemSubscriptionId ?? SUB}`,
    creemSubscriptionId: SUB,
    status: `active`,
    seats: 1,
    cancelAtPeriodEnd: false,
    createdAt: new Date(`2026-01-01T00:00:00.000Z`),
    ...overrides,
  }
}

// The seed an input with nothing beyond metadata/units produces — the shape
// most injected-commit assertions expect.
const EMPTY_SEED = {
  referenceId: null,
  creemCustomerId: null,
  productId: null,
  status: null,
}

beforeEach(() => {
  updateCalls.length = 0
  insertCalls.length = 0
  selectRows.length = 0
  updateReturning.length = 0
  vi.spyOn(console, `error`).mockImplementation(() => {})
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe(`extractTeamBinding`, () => {
  it(`binds teamId + seats from metadata`, () => {
    expect(
      extractTeamBinding({
        creemSubscriptionId: SUB,
        metadata: { teamId: WS, seats: 4, referenceId: `user_1` },
      })
    ).toEqual({ creemSubscriptionId: SUB, teamId: WS, seats: 4 })
  })

  it(`coerces string seat metadata (Creem round-trips metadata as strings)`, () => {
    expect(
      extractTeamBinding({
        creemSubscriptionId: SUB,
        metadata: { teamId: WS, seats: `7` },
      })?.seats
    ).toBe(7)
  })

  it(`prefers the paid entity units over forged metadata seats`, () => {
    // metadata is client-suppliable at checkout time; units is the quantity
    // Creem actually charged for. A forged metadata.seats must never win.
    expect(
      extractTeamBinding({
        creemSubscriptionId: SUB,
        metadata: { teamId: WS, seats: 1000 },
        units: 1,
      })?.seats
    ).toBe(1)
  })

  it(`falls back to the entity units when metadata has no seats`, () => {
    expect(
      extractTeamBinding({
        creemSubscriptionId: SUB,
        metadata: { teamId: WS },
        units: 3,
      })?.seats
    ).toBe(3)
  })

  it(`defaults seats to 1 when neither metadata nor units carry a count`, () => {
    expect(
      extractTeamBinding({
        creemSubscriptionId: SUB,
        metadata: { teamId: WS },
      })?.seats
    ).toBe(1)
  })

  it(`is not bindable without a subscription id`, () => {
    expect(
      extractTeamBinding({
        creemSubscriptionId: null,
        metadata: { teamId: WS, seats: 2 },
      })
    ).toBeNull()
  })

  it(`is not bindable without a teamId in metadata`, () => {
    expect(
      extractTeamBinding({
        creemSubscriptionId: SUB,
        metadata: { seats: 2, referenceId: `user_1` },
      })
    ).toBeNull()
  })

  it(`ignores non-positive / non-integer seat values`, () => {
    expect(
      extractTeamBinding({
        creemSubscriptionId: SUB,
        metadata: { teamId: WS, seats: 0 },
        units: -5,
      })?.seats
    ).toBe(1)
  })
})

describe(`payload mappers`, () => {
  it(`maps a flattened checkout.completed event`, () => {
    const input = bindingInputFromCheckout({
      units: 5,
      metadata: { teamId: WS, seats: 5, referenceId: `user_1` },
      subscription: { id: SUB },
    })
    expect(input).toEqual({
      creemSubscriptionId: SUB,
      metadata: { teamId: WS, seats: 5, referenceId: `user_1` },
      units: 5,
      creemCustomerId: null,
      productId: null,
    })
  })

  it(`maps a flattened subscription.* event (units on the first item)`, () => {
    const input = bindingInputFromSubscription({
      id: SUB,
      metadata: { teamId: WS, seats: 9 },
      items: [{ units: 9 }],
      status: `active`,
    })
    expect(input).toEqual({
      creemSubscriptionId: SUB,
      metadata: { teamId: WS, seats: 9 },
      units: 9,
      status: `active`,
      creemCustomerId: null,
      productId: null,
    })
  })

  // Seed ids for the commit's insert path (REV-12): subscription events carry
  // expanded customer/product entities; a checkout's nested subscription
  // references them as bare id strings (preferred over the checkout's own
  // expanded entities — they are the subscription's).
  it(`carries customer/product ids off a subscription event`, () => {
    const input = bindingInputFromSubscription({
      id: SUB,
      metadata: { teamId: WS },
      customer: { id: `cust_1` },
      product: { id: `prod_1` },
    })
    expect(input.creemCustomerId).toBe(`cust_1`)
    expect(input.productId).toBe(`prod_1`)
  })

  it(`prefers the nested subscription's bare-id references on a checkout`, () => {
    const input = bindingInputFromCheckout({
      metadata: { teamId: WS },
      subscription: { id: SUB, customer: `cust_1`, product: `prod_1` },
      customer: { id: `cust_other` },
      product: { id: `prod_other` },
    })
    expect(input.creemCustomerId).toBe(`cust_1`)
    expect(input.productId).toBe(`prod_1`)
  })

  it(`falls back to the checkout's expanded entities when the nested subscription omits them`, () => {
    const input = bindingInputFromCheckout({
      metadata: { teamId: WS },
      subscription: { id: SUB },
      customer: { id: `cust_1` },
      product: { id: `prod_1` },
    })
    expect(input.creemCustomerId).toBe(`cust_1`)
    expect(input.productId).toBe(`prod_1`)
  })

  // A checkout's `status` describes the CHECKOUT (`completed`), not the
  // subscription — mapping it would let a checkout clear a pending cancel.
  it(`never carries a status off a checkout event`, () => {
    const input = bindingInputFromCheckout({
      units: 5,
      metadata: { teamId: WS },
      subscription: { id: SUB },
    })
    expect(input).not.toHaveProperty(`status`)
  })
})

describe(`statusClearsPendingCancel`, () => {
  it(`clears on every live, non-scheduled_cancel status`, () => {
    for (const status of [`active`, `trialing`, `paid`]) {
      expect(statusClearsPendingCancel(status), status).toBe(true)
    }
  })

  it(`never clears while the cancellation is the pending state itself`, () => {
    expect(statusClearsPendingCancel(`scheduled_cancel`)).toBe(false)
  })

  // These say nothing about a SCHEDULED cancellation, so they must not touch
  // the flag either way.
  it(`never clears on a dead or suspended status`, () => {
    for (const status of [`canceled`, `expired`, `paused`, `past_due`, `unpaid`]) {
      expect(statusClearsPendingCancel(status), status).toBe(false)
    }
  })

  it(`never clears without a status (checkout events, legacy payloads)`, () => {
    expect(statusClearsPendingCancel(null)).toBe(false)
    expect(statusClearsPendingCancel(undefined)).toBe(false)
    expect(statusClearsPendingCancel(``)).toBe(false)
  })
})

describe(`pickDuplicateSubscriptionsToCancel`, () => {
  const older = row({
    creemSubscriptionId: OTHER_SUB,
    createdAt: new Date(`2026-01-01T00:00:00.000Z`),
  })
  const newer = row({
    creemSubscriptionId: SUB,
    createdAt: new Date(`2026-02-01T00:00:00.000Z`),
  })

  it(`cancels the newer subscription when a second one lands`, () => {
    expect(pickDuplicateSubscriptionsToCancel(newer, [older])).toEqual([newer])
  })

  it(`still cancels the newer one when the webhook is for the older sub`, () => {
    // A subscription.update on the incumbent must reach the same verdict as
    // the checkout webhook for the duplicate.
    expect(pickDuplicateSubscriptionsToCancel(older, [newer])).toEqual([newer])
  })

  it(`keeps the incumbent on a createdAt tie`, () => {
    const tied = row({ creemSubscriptionId: SUB, createdAt: older.createdAt })
    expect(pickDuplicateSubscriptionsToCancel(tied, [older])).toEqual([tied])
  })

  it(`keeps only the oldest when three subscriptions compete`, () => {
    const newest = row({
      creemSubscriptionId: `sub_newest`,
      createdAt: new Date(`2026-03-01T00:00:00.000Z`),
    })
    expect(
      pickDuplicateSubscriptionsToCancel(newest, [older, newer])
    ).toEqual([newer, newest])
  })

  it(`cancels nothing when the team has no other active subscription`, () => {
    expect(pickDuplicateSubscriptionsToCancel(newer, [])).toEqual([])
  })

  it(`cancels nothing when the incoming subscription is not active`, () => {
    const canceled = row({ creemSubscriptionId: SUB, status: `canceled` })
    expect(pickDuplicateSubscriptionsToCancel(canceled, [older])).toEqual([])
  })

  it(`cancels nothing when the incoming subscription is already ending`, () => {
    const ending = row({
      creemSubscriptionId: SUB,
      status: `scheduled_cancel`,
      createdAt: newer.createdAt,
    })
    expect(pickDuplicateSubscriptionsToCancel(ending, [older])).toEqual([])
  })

  it(`never cancels a replacement for an incumbent that is already ending`, () => {
    // The incumbent is scheduled to cancel, so the new subscription is a
    // legitimate replacement — cancelling it would leave the team on the
    // subscription that is about to end.
    expect(
      pickDuplicateSubscriptionsToCancel(newer, [
        row({ ...older, status: `scheduled_cancel` }),
      ])
    ).toEqual([])
    expect(
      pickDuplicateSubscriptionsToCancel(newer, [
        row({ ...older, cancelAtPeriodEnd: true }),
      ])
    ).toEqual([])
  })

  it(`ignores inactive incumbents entirely`, () => {
    expect(
      pickDuplicateSubscriptionsToCancel(newer, [
        row({ ...older, status: `canceled` }),
      ])
    ).toEqual([])
  })
})

describe(`bindSubscriptionToTeam`, () => {
  it(`commits the binding and returns it`, async () => {
    const commit = vi.fn(async () => {})
    const result = await bindSubscriptionToTeam(
      {
        creemSubscriptionId: SUB,
        metadata: { teamId: WS, seats: 6 },
      },
      { commit, loadTeamSubscriptions: async () => [] }
    )
    expect(result).toEqual({ creemSubscriptionId: SUB, teamId: WS, seats: 6 })
    expect(commit).toHaveBeenCalledExactlyOnceWith(
      { creemSubscriptionId: SUB, teamId: WS, seats: 6 },
      { clearPendingCancel: false, seed: EMPTY_SEED }
    )
  })

  it(`does not commit an unbindable payload`, async () => {
    const commit = vi.fn(async () => {})
    const result = await bindSubscriptionToTeam(
      { creemSubscriptionId: SUB, metadata: { seats: 2 } },
      { commit }
    )
    expect(result).toBeNull()
    expect(commit).not.toHaveBeenCalled()
  })

  // The packet's core acceptance: a webhook event carrying metadata.teamId
  // + units N ends with a creem_subscriptions row bound to that team with
  // seats=N. Exercised through the default DB commit path (db mocked above).
  it(`writes teamId + seats=N to creem_subscriptions on the default path`, async () => {
    const event = bindingInputFromCheckout({
      units: 8,
      metadata: { teamId: WS, referenceId: `user_1` },
      subscription: { id: SUB },
    })
    selectRows.push(row({ creemSubscriptionId: SUB }))
    updateReturning.push({ id: `row_1` })
    const result = await bindSubscriptionToTeam(event)

    expect(result).toEqual({ creemSubscriptionId: SUB, teamId: WS, seats: 8 })
    expect(updateCalls).toHaveLength(1)
    expect(updateCalls[0].values).toEqual({ teamId: WS, seats: 8 })
    // A where-clause (keyed on creemSubscriptionId) is always applied — never a
    // table-wide update.
    expect(updateCalls[0].where).toBeDefined()
    // The row existed, so the insert half of the upsert never runs.
    expect(insertCalls).toHaveLength(0)
    expect(console.error).not.toHaveBeenCalled()
  })

  // REV-12: a subscription.* event can outrun its checkout.completed, and the
  // plugin's misdirected customer-id fallback is blocked by the
  // reject_creem_subscription_rekey trigger — so no row exists for the id.
  // The default commit must CREATE it (seeded from the event) instead of
  // silently updating zero rows, or the paying subscription stays invisible
  // to getTeamPlan, the duplicate guard and the delete gates.
  it(`creates the row when the plugin has none for this subscription id`, async () => {
    const event = bindingInputFromSubscription({
      id: SUB,
      metadata: { teamId: WS, referenceId: `user_1` },
      items: [{ units: 4 }],
      status: `active`,
      customer: { id: `cust_1` },
      product: { id: `prod_1` },
    })
    const result = await bindSubscriptionToTeam(event, {
      loadTeamSubscriptions: async () => [],
    })

    expect(result).toEqual({ creemSubscriptionId: SUB, teamId: WS, seats: 4 })
    expect(insertCalls).toHaveLength(1)
    expect(insertCalls[0].values).toMatchObject({
      creemSubscriptionId: SUB,
      teamId: WS,
      seats: 4,
      referenceId: `user_1`,
      creemCustomerId: `cust_1`,
      productId: `prod_1`,
      status: `active`,
    })
    expect(insertCalls[0].values.id).toEqual(expect.any(String))
    // Conflict target + set: a concurrent delivery creating the row between
    // the update and the insert must degrade to the plain binding write.
    expect(insertCalls[0].conflict.target).toBeDefined()
    expect(insertCalls[0].conflict.set).toEqual({
      teamId: WS,
      seats: 4,
      // The live status also clears the pending-cancel mirror (see the
      // healing describe below) — same set as the update half.
      cancelAtPeriodEnd: false,
    })
    // Loud: a bind-path row creation means the plugin missed the event.
    expect(console.error).toHaveBeenCalledOnce()
  })

  it(`never seeds a status-less (checkout) payload with a fake status`, async () => {
    const event = bindingInputFromCheckout({
      units: 2,
      metadata: { teamId: WS, referenceId: `user_1` },
      subscription: { id: SUB, customer: `cust_1`, product: `prod_1` },
    })
    await bindSubscriptionToTeam(event, { loadTeamSubscriptions: async () => [] })

    expect(insertCalls).toHaveLength(1)
    // No status key at all — the schema's `pending` default applies.
    expect(insertCalls[0].values).not.toHaveProperty(`status`)
  })
})

describe(`bindSubscriptionToTeam pending-cancel healing`, () => {
  // A cancellation scheduled and then RESUMED in the Creem dashboard never
  // reaches billing.resumeSubscription, so our optimistic cancelAtPeriodEnd
  // mirror would stay true forever — pinning the pending-cancel banner and
  // making assertSubscriptionMutable refuse every seat/plan change. The
  // webhook is the only place that learns about it.
  it(`clears the stale flag when the event reports a live status`, async () => {
    const event = bindingInputFromSubscription({
      id: SUB,
      metadata: { teamId: WS },
      items: [{ units: 2 }],
      status: `active`,
    })
    selectRows.push(row({ creemSubscriptionId: SUB, cancelAtPeriodEnd: true }))
    updateReturning.push({ id: `row_1` })
    await bindSubscriptionToTeam(event)

    expect(updateCalls).toHaveLength(1)
    expect(updateCalls[0].values).toEqual({
      teamId: WS,
      seats: 2,
      cancelAtPeriodEnd: false,
    })
  })

  it(`leaves the flag alone while the cancellation is still scheduled`, async () => {
    const event = bindingInputFromSubscription({
      id: SUB,
      metadata: { teamId: WS },
      items: [{ units: 2 }],
      status: `scheduled_cancel`,
    })
    selectRows.push(
      row({ creemSubscriptionId: SUB, status: `scheduled_cancel` })
    )
    updateReturning.push({ id: `row_1` })
    await bindSubscriptionToTeam(event)

    expect(updateCalls).toHaveLength(1)
    expect(updateCalls[0].values).toEqual({ teamId: WS, seats: 2 })
  })

  it(`leaves the flag alone for a payload that carries no status`, async () => {
    // checkout.completed and any legacy/partial payload: nothing was learned
    // about the cancellation state, so nothing is written.
    selectRows.push(row({ creemSubscriptionId: SUB }))
    updateReturning.push({ id: `row_1` })
    await bindSubscriptionToTeam(
      bindingInputFromCheckout({
        units: 2,
        metadata: { teamId: WS },
        subscription: { id: SUB },
      })
    )

    expect(updateCalls).toHaveLength(1)
    expect(updateCalls[0].values).toEqual({ teamId: WS, seats: 2 })
  })

  it(`hands the verdict to an injected commit sink`, async () => {
    const commit = vi.fn(async () => {})
    await bindSubscriptionToTeam(
      {
        creemSubscriptionId: SUB,
        metadata: { teamId: WS },
        units: 1,
        status: `trialing`,
      },
      { commit, loadTeamSubscriptions: async () => [] }
    )
    expect(commit).toHaveBeenCalledExactlyOnceWith(
      { creemSubscriptionId: SUB, teamId: WS, seats: 1 },
      { clearPendingCancel: true, seed: { ...EMPTY_SEED, status: `trialing` } }
    )
  })
})

describe(`bindSubscriptionToTeam duplicate guard (REV2-30)`, () => {
  const incumbent = row({
    creemSubscriptionId: OTHER_SUB,
    seats: 3,
    createdAt: new Date(`2026-01-01T00:00:00.000Z`),
  })
  const incoming = row({
    creemSubscriptionId: SUB,
    seats: 5,
    createdAt: new Date(`2026-02-01T00:00:00.000Z`),
  })

  it(`cancels the newly bound subscription and still commits the binding`, async () => {
    const commit = vi.fn(async () => {})
    const cancelSubscriptions = vi.fn(async () => {})
    const result = await bindSubscriptionToTeam(
      { creemSubscriptionId: SUB, metadata: { teamId: WS }, units: 5 },
      {
        commit,
        cancelSubscriptions,
        loadTeamSubscriptions: async () => [incumbent, incoming],
      }
    )

    expect(cancelSubscriptions).toHaveBeenCalledExactlyOnceWith([incoming])
    // The row stays bound so the double-charge is discoverable (and the
    // best-effort cancel marks it canceled locally).
    expect(result).toEqual({ creemSubscriptionId: SUB, teamId: WS, seats: 5 })
    expect(commit).toHaveBeenCalledOnce()
    expect(console.error).toHaveBeenCalledOnce()
    expect(vi.mocked(console.error).mock.calls[0].join(` `)).toContain(SUB)
  })

  it(`logs loudly but cancels nothing when the verdict abstains`, async () => {
    const commit = vi.fn(async () => {})
    const cancelSubscriptions = vi.fn(async () => {})
    await bindSubscriptionToTeam(
      { creemSubscriptionId: SUB, metadata: { teamId: WS }, units: 5 },
      {
        commit,
        cancelSubscriptions,
        loadTeamSubscriptions: async () => [
          row({ ...incumbent, status: `scheduled_cancel` }),
          incoming,
        ],
      }
    )

    expect(cancelSubscriptions).not.toHaveBeenCalled()
    expect(console.error).toHaveBeenCalledOnce()
    expect(commit).toHaveBeenCalledOnce()
  })

  it(`stays silent once the duplicate itself is no longer active`, async () => {
    // The webhook our own cancellation triggers must not re-report a state we
    // already resolved.
    const cancelSubscriptions = vi.fn(async () => {})
    await bindSubscriptionToTeam(
      { creemSubscriptionId: SUB, metadata: { teamId: WS }, units: 5 },
      {
        commit: async () => {},
        cancelSubscriptions,
        loadTeamSubscriptions: async () => [
          incumbent,
          row({ ...incoming, status: `canceled` }),
        ],
      }
    )
    expect(cancelSubscriptions).not.toHaveBeenCalled()
    expect(console.error).not.toHaveBeenCalled()
  })

  it(`stays silent for the normal single-subscription re-bind`, async () => {
    const cancelSubscriptions = vi.fn(async () => {})
    await bindSubscriptionToTeam(
      { creemSubscriptionId: SUB, metadata: { teamId: WS }, units: 5 },
      {
        commit: async () => {},
        cancelSubscriptions,
        loadTeamSubscriptions: async () => [incoming],
      }
    )
    expect(cancelSubscriptions).not.toHaveBeenCalled()
    expect(console.error).not.toHaveBeenCalled()
  })

  // The full REV-12 out-of-order scenario: subscription.active for a NEW
  // subscription arrives first, no row exists for it, and the team already
  // has a live incumbent. The commit must create the row BEFORE the guard
  // runs so the duplicate is caught (and cancelled) within this same webhook
  // delivery instead of waiting for checkout.completed.
  it(`creates the missing row first, then cancels it as the newer duplicate`, async () => {
    const insertsSeenAtCancel: number[] = []
    const cancelSubscriptions = vi.fn(async () => {
      insertsSeenAtCancel.push(insertCalls.length)
    })
    await bindSubscriptionToTeam(
      bindingInputFromSubscription({
        id: SUB,
        metadata: { teamId: WS, referenceId: `user_1` },
        items: [{ units: 5 }],
        status: `active`,
        customer: { id: `cust_1` },
        product: { id: `prod_1` },
      }),
      {
        cancelSubscriptions,
        loadTeamSubscriptions: async () => [incumbent, incoming],
      }
    )

    expect(insertCalls).toHaveLength(1)
    expect(cancelSubscriptions).toHaveBeenCalledExactlyOnceWith([incoming])
    // The row existed by the time the guard's verdict was executed.
    expect(insertsSeenAtCancel).toEqual([1])
  })

  it(`binds anyway when the duplicate check itself fails`, async () => {
    const commit = vi.fn(async () => {})
    const result = await bindSubscriptionToTeam(
      { creemSubscriptionId: SUB, metadata: { teamId: WS }, units: 5 },
      {
        commit,
        loadTeamSubscriptions: async () => {
          throw new Error(`db down`)
        },
      }
    )
    expect(result).toEqual({ creemSubscriptionId: SUB, teamId: WS, seats: 5 })
    expect(commit).toHaveBeenCalledOnce()
    expect(console.error).toHaveBeenCalledOnce()
  })
})
