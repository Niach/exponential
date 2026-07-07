import { describe, expect, it } from "vitest"
import { TRPCError } from "@trpc/server"

// The module's DB/Creem entry points import "@/db/connection"; stub it so the
// pure helpers can be imported without a real Postgres connection.
import { vi } from "vitest"
vi.mock(`@/db/connection`, () => ({ db: {} }))

import {
  assertSubscriptionMutable,
  buildSeatUpdateItems,
  SUBSCRIPTION_UPDATE_BEHAVIOR,
} from "./creem-subscriptions"

const ITEM = {
  id: `sitem_1`,
  productId: `prod_1`,
  priceId: `pprice_1`,
  units: 1,
}

describe(`buildSeatUpdateItems`, () => {
  it(`maps the single item to an update payload with the new units`, () => {
    expect(buildSeatUpdateItems([ITEM], 5)).toEqual([
      { id: `sitem_1`, productId: `prod_1`, priceId: `pprice_1`, units: 5 },
    ])
  })

  // Creem rejects an items entry that carries only the item id ("Could not
  // find product or price"), and an entry WITHOUT an id creates a brand-new
  // line item — so every field must be present or we refuse.
  it.each([
    [`missing id`, { ...ITEM, id: null }],
    [`missing productId`, { ...ITEM, productId: undefined }],
    [`missing priceId`, { ...ITEM, priceId: `` }],
  ])(`throws when the item has %s`, (_label, item) => {
    expect(() => buildSeatUpdateItems([item], 2)).toThrow(TRPCError)
  })

  it(`throws on zero or multiple items (never guess which line to resize)`, () => {
    expect(() => buildSeatUpdateItems([], 2)).toThrow(TRPCError)
    expect(() => buildSeatUpdateItems(null, 2)).toThrow(TRPCError)
    expect(() => buildSeatUpdateItems([ITEM, { ...ITEM, id: `sitem_2` }], 2)).toThrow(
      TRPCError
    )
  })
})

describe(`assertSubscriptionMutable`, () => {
  it(`passes for an active bound subscription`, () => {
    expect(() =>
      assertSubscriptionMutable({
        creemSubscriptionId: `sub_1`,
        cancelAtPeriodEnd: false,
      })
    ).not.toThrow()
  })

  it(`throws when the workspace has no subscription`, () => {
    expect(() => assertSubscriptionMutable(null)).toThrow(
      /no active subscription/
    )
  })

  it(`throws when the row has no Creem subscription id (legacy row)`, () => {
    expect(() =>
      assertSubscriptionMutable({
        creemSubscriptionId: null,
        cancelAtPeriodEnd: false,
      })
    ).toThrow(/contact support/)
  })

  it(`throws when the subscription is scheduled to cancel`, () => {
    expect(() =>
      assertSubscriptionMutable({
        creemSubscriptionId: `sub_1`,
        cancelAtPeriodEnd: true,
      })
    ).toThrow(/scheduled to cancel/)
  })
})

describe(`SUBSCRIPTION_UPDATE_BEHAVIOR`, () => {
  // Creem's proration math is broken as of 2026-07-07 (increases charge
  // new+old instead of the delta — see the constant's comment). proration-none
  // is the only behavior that never overcharges; flip only after Creem
  // confirms a fix AND a test-mode seat increase charges exactly the prorated
  // delta.
  it(`stays proration-none until Creem fixes increase proration`, () => {
    expect(SUBSCRIPTION_UPDATE_BEHAVIOR).toBe(`proration-none`)
  })
})
