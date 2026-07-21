import { describe, expect, it, vi, beforeEach, afterEach } from "vitest"
import { TRPCError } from "@trpc/server"

// The module's DB/Creem entry points import "@/db/connection", the Creem SDK,
// and isCloudInstance(); all three are mocked so the helpers can be exercised
// without Postgres or the Creem API. `vi.hoisted` lets the mock factories
// (hoisted above imports) share these recorders with the tests.
const mocks = vi.hoisted(() => ({
  cloud: { value: true },
  selectRows: [] as unknown[],
  selectCalls: { count: 0 },
  cancel: vi.fn(),
  updateCalls: [] as Array<{ values: unknown }>,
}))

vi.mock(`@/db/connection`, () => ({
  db: {
    select: () => ({
      from: () => ({
        where: async () => {
          mocks.selectCalls.count += 1
          return mocks.selectRows
        },
      }),
    }),
    update: () => ({
      set: (values: unknown) => ({
        where: async () => {
          mocks.updateCalls.push({ values })
          return []
        },
      }),
    }),
  },
}))

vi.mock(`@/lib/bootstrap-cloud`, () => ({
  isCloudInstance: () => mocks.cloud.value,
}))

vi.mock(`@creem_io/better-auth/server`, () => ({
  createCreemClient: () => ({ subscriptions: { cancel: mocks.cancel } }),
}))

import {
  assertSubscriptionMutable,
  buildSeatUpdateItems,
  cancelCreemSubscriptionsBestEffort,
  findActiveSubscriptionsForUser,
  findActiveSubscriptionsForTeams,
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

  it(`throws when the team has no subscription`, () => {
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
  // Creem's proration was broken 2026-07-07..07-09 (increases overcharged,
  // decreases regressed to charging) so we pinned proration-none. Re-verified
  // fixed in test mode 2026-07-21 (increase charged the exact prorated delta,
  // decrease refunded it — see the constant's comment), so we charge the delta
  // immediately. Revert to proration-none if Creem regresses.
  it(`charges the prorated delta immediately`, () => {
    expect(SUBSCRIPTION_UPDATE_BEHAVIOR).toBe(`proration-charge-immediately`)
  })
})

// ── Cancel-on-delete (go-live audit) ─────────────────────────────────────────
// Deleting a team/account must not leave a paying ghost subscription in
// Creem. The capture helpers run BEFORE the local delete (the FKs make rows
// unfindable afterwards) and the cancel runs AFTER commit, best-effort.

const ORIGINAL_CREEM_API_KEY = process.env.CREEM_API_KEY

describe(`cancel-on-delete`, () => {
  let errorSpy: ReturnType<typeof vi.spyOn>
  let warnSpy: ReturnType<typeof vi.spyOn>

  beforeEach(() => {
    mocks.cloud.value = true
    mocks.selectRows = []
    mocks.selectCalls.count = 0
    mocks.updateCalls.length = 0
    mocks.cancel.mockReset()
    process.env.CREEM_API_KEY = `creem_test_key`
    errorSpy = vi.spyOn(console, `error`).mockImplementation(() => {})
    warnSpy = vi.spyOn(console, `warn`).mockImplementation(() => {})
  })

  afterEach(() => {
    errorSpy.mockRestore()
    warnSpy.mockRestore()
    if (ORIGINAL_CREEM_API_KEY === undefined) {
      delete process.env.CREEM_API_KEY
    } else {
      process.env.CREEM_API_KEY = ORIGINAL_CREEM_API_KEY
    }
  })

  describe(`findActiveSubscriptionsForTeams`, () => {
    it(`returns the team's active subscription rows`, async () => {
      mocks.selectRows = [{ id: `row-1`, creemSubscriptionId: `sub_1` }]
      await expect(
        findActiveSubscriptionsForTeams([`ws-1`])
      ).resolves.toEqual([{ id: `row-1`, creemSubscriptionId: `sub_1` }])
      expect(mocks.selectCalls.count).toBe(1)
    })

    it(`skips the query entirely for an empty team list`, async () => {
      await expect(findActiveSubscriptionsForTeams([])).resolves.toEqual(
        []
      )
      expect(mocks.selectCalls.count).toBe(0)
    })

    it(`no-ops on self-hosted instances (no billing)`, async () => {
      mocks.cloud.value = false
      await expect(
        findActiveSubscriptionsForTeams([`ws-1`])
      ).resolves.toEqual([])
      expect(mocks.selectCalls.count).toBe(0)
    })
  })

  describe(`findActiveSubscriptionsForUser`, () => {
    it(`returns the subscriptions the user purchased`, async () => {
      mocks.selectRows = [{ id: `row-1`, creemSubscriptionId: `sub_1` }]
      await expect(findActiveSubscriptionsForUser(`user-1`)).resolves.toEqual([
        { id: `row-1`, creemSubscriptionId: `sub_1` },
      ])
      expect(mocks.selectCalls.count).toBe(1)
    })

    it(`no-ops on self-hosted instances (no billing)`, async () => {
      mocks.cloud.value = false
      await expect(findActiveSubscriptionsForUser(`user-1`)).resolves.toEqual(
        []
      )
      expect(mocks.selectCalls.count).toBe(0)
    })
  })

  describe(`cancelCreemSubscriptionsBestEffort`, () => {
    it(`cancels each subscription immediately and marks the local rows canceled`, async () => {
      mocks.cancel.mockResolvedValue({})
      await cancelCreemSubscriptionsBestEffort([
        { id: `row-1`, creemSubscriptionId: `sub_1` },
        { id: `row-2`, creemSubscriptionId: `sub_2` },
      ])
      expect(mocks.cancel).toHaveBeenCalledTimes(2)
      expect(mocks.cancel).toHaveBeenNthCalledWith(1, `sub_1`, {
        mode: `immediate`,
      })
      expect(mocks.cancel).toHaveBeenNthCalledWith(2, `sub_2`, {
        mode: `immediate`,
      })
      expect(mocks.updateCalls).toHaveLength(2)
      expect(mocks.updateCalls[0].values).toMatchObject({ status: `canceled` })
    })

    it(`continues past a failed remote cancel and never throws`, async () => {
      mocks.cancel
        .mockRejectedValueOnce(new Error(`Creem is down`))
        .mockResolvedValueOnce({})
      await expect(
        cancelCreemSubscriptionsBestEffort([
          { id: `row-1`, creemSubscriptionId: `sub_1` },
          { id: `row-2`, creemSubscriptionId: `sub_2` },
        ])
      ).resolves.toBeUndefined()
      expect(mocks.cancel).toHaveBeenCalledTimes(2)
      // Only the successfully cancelled row is marked canceled locally.
      expect(mocks.updateCalls).toHaveLength(1)
      // The failure is logged loudly for manual dashboard cleanup.
      expect(errorSpy).toHaveBeenCalledWith(
        expect.stringContaining(`sub_1`),
        expect.any(Error)
      )
    })

    it(`skips rows without a Creem subscription id (legacy rows)`, async () => {
      await cancelCreemSubscriptionsBestEffort([
        { id: `row-legacy`, creemSubscriptionId: null },
      ])
      expect(mocks.cancel).not.toHaveBeenCalled()
      expect(mocks.updateCalls).toHaveLength(0)
      expect(errorSpy).toHaveBeenCalled()
    })

    it(`no-ops on self-hosted instances (no billing)`, async () => {
      mocks.cloud.value = false
      await cancelCreemSubscriptionsBestEffort([
        { id: `row-1`, creemSubscriptionId: `sub_1` },
      ])
      expect(mocks.cancel).not.toHaveBeenCalled()
    })

    it(`logs instead of throwing when CREEM_API_KEY is not configured`, async () => {
      delete process.env.CREEM_API_KEY
      await expect(
        cancelCreemSubscriptionsBestEffort([
          { id: `row-1`, creemSubscriptionId: `sub_1` },
        ])
      ).resolves.toBeUndefined()
      expect(mocks.cancel).not.toHaveBeenCalled()
      expect(errorSpy).toHaveBeenCalledWith(
        expect.stringContaining(`sub_1`)
      )
    })
  })
})
