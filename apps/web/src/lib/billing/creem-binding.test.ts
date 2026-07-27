import { describe, expect, it, vi, beforeEach, afterEach } from "vitest"

// Record every db.update(...).set(...).where(...) chain so the default commit
// path can be asserted without a real Postgres connection, and serve rows to
// the default duplicate-guard read. `vi.hoisted` lets the mock factory (hoisted
// above imports) share these recorders with the tests.
const { updateCalls, selectRows } = vi.hoisted(() => ({
  updateCalls: [] as Array<{ table: unknown; values: unknown; where: unknown }>,
  selectRows: [] as unknown[],
}))

vi.mock(`@/db/connection`, () => ({
  db: {
    update: (table: unknown) => ({
      set: (values: unknown) => ({
        where: (where: unknown) => {
          updateCalls.push({ table, values, where })
          return Promise.resolve()
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

beforeEach(() => {
  updateCalls.length = 0
  selectRows.length = 0
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
    })
  })

  it(`maps a flattened subscription.* event (units on the first item)`, () => {
    const input = bindingInputFromSubscription({
      id: SUB,
      metadata: { teamId: WS, seats: 9 },
      items: [{ units: 9 }],
    })
    expect(input).toEqual({
      creemSubscriptionId: SUB,
      metadata: { teamId: WS, seats: 9 },
      units: 9,
    })
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
    expect(commit).toHaveBeenCalledExactlyOnceWith({
      creemSubscriptionId: SUB,
      teamId: WS,
      seats: 6,
    })
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
    const result = await bindSubscriptionToTeam(event)

    expect(result).toEqual({ creemSubscriptionId: SUB, teamId: WS, seats: 8 })
    expect(updateCalls).toHaveLength(1)
    expect(updateCalls[0].values).toEqual({ teamId: WS, seats: 8 })
    // A where-clause (keyed on creemSubscriptionId) is always applied — never a
    // table-wide update.
    expect(updateCalls[0].where).toBeDefined()
    expect(console.error).not.toHaveBeenCalled()
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
