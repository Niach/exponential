import { describe, expect, it, vi, beforeEach } from "vitest"

// Record every db.update(...).set(...).where(...) chain so the default commit
// path can be asserted without a real Postgres connection. `vi.hoisted` lets
// the mock factory (hoisted above imports) share this recorder with the tests.
const { updateCalls } = vi.hoisted(() => ({
  updateCalls: [] as Array<{ table: unknown; values: unknown; where: unknown }>,
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
  },
}))

import {
  extractTeamBinding,
  bindSubscriptionToTeam,
  bindingInputFromCheckout,
  bindingInputFromSubscription,
} from "./creem-binding"

const WS = `11111111-1111-1111-1111-111111111111`
const SUB = `sub_abc123`

beforeEach(() => {
  updateCalls.length = 0
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

describe(`bindSubscriptionToTeam`, () => {
  it(`commits the binding and returns it`, async () => {
    const commit = vi.fn(async () => {})
    const result = await bindSubscriptionToTeam(
      {
        creemSubscriptionId: SUB,
        metadata: { teamId: WS, seats: 6 },
      },
      commit
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
      commit
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
    const result = await bindSubscriptionToTeam(event)

    expect(result).toEqual({ creemSubscriptionId: SUB, teamId: WS, seats: 8 })
    expect(updateCalls).toHaveLength(1)
    expect(updateCalls[0].values).toEqual({ teamId: WS, seats: 8 })
    // A where-clause (keyed on creemSubscriptionId) is always applied — never a
    // table-wide update.
    expect(updateCalls[0].where).toBeDefined()
  })
})
