import { describe, expect, it, vi, beforeEach } from "vitest"

// The team-delete billing gate (REV2-55). `assertTeamDeletableBilling` reads
// the team's active subscriptions, so the DB helper and the cloud check are
// mocked; the predicate itself is pure.
const mocks = vi.hoisted(() => ({
  cloud: { value: true },
  subscriptions: {
    value: [] as Array<{ cancelAtPeriodEnd: boolean; status: string }>,
  },
  getCalls: { count: 0 },
}))

vi.mock(`@/lib/bootstrap-cloud`, () => ({
  isCloudInstance: () => mocks.cloud.value,
}))

vi.mock(`@/lib/billing/creem-subscriptions`, () => ({
  listActiveTeamSubscriptions: async () => {
    mocks.getCalls.count += 1
    return mocks.subscriptions.value
  },
}))

import {
  assertTeamDeletableBilling,
  assertTeamSubscriptionCancelled,
  isSubscriptionPendingCancel,
  SCHEDULED_CANCEL_STATUS,
  subscriptionBlocksTeamDelete,
  TEAM_DELETE_ACTIVE_SUBSCRIPTION_MESSAGE,
} from "./billing-handover"

/** A live subscription row (the only shape that blocks a team delete). */
const LIVE = { cancelAtPeriodEnd: false, status: `active` }

beforeEach(() => {
  mocks.cloud.value = true
  mocks.subscriptions.value = []
  mocks.getCalls.count = 0
})

describe(`isSubscriptionPendingCancel`, () => {
  it(`is false for a live subscription and for no subscription`, () => {
    expect(isSubscriptionPendingCancel(LIVE)).toBe(false)
    expect(isSubscriptionPendingCancel(null)).toBe(false)
    expect(isSubscriptionPendingCancel(undefined)).toBe(false)
  })

  it(`is true from our own optimistic cancelAtPeriodEnd flag`, () => {
    expect(
      isSubscriptionPendingCancel({ ...LIVE, cancelAtPeriodEnd: true })
    ).toBe(true)
  })

  // Creem's plugin persists `event.object.status` verbatim, so a cancellation
  // scheduled outside our UI (Creem dashboard, support) — or one whose
  // optimistic write was lost — shows up ONLY as this status.
  it(`is true from Creem's scheduled_cancel status alone`, () => {
    expect(SCHEDULED_CANCEL_STATUS).toBe(`scheduled_cancel`)
    expect(
      isSubscriptionPendingCancel({
        cancelAtPeriodEnd: false,
        status: SCHEDULED_CANCEL_STATUS,
      })
    ).toBe(true)
  })
})

describe(`subscriptionBlocksTeamDelete`, () => {
  it(`blocks while a live subscription exists`, () => {
    expect(subscriptionBlocksTeamDelete(LIVE)).toBe(true)
  })

  it(`allows once cancellation is scheduled (the paid period just runs out)`, () => {
    expect(
      subscriptionBlocksTeamDelete({ ...LIVE, cancelAtPeriodEnd: true })
    ).toBe(false)
  })

  // The row is still ACTIVE (scheduled_cancel is an entitled status), so the
  // gate must recognise the pending cancellation from the status alone.
  it(`allows a Creem-side scheduled_cancel with no local flag`, () => {
    expect(
      subscriptionBlocksTeamDelete({
        cancelAtPeriodEnd: false,
        status: SCHEDULED_CANCEL_STATUS,
      })
    ).toBe(false)
  })

  it(`allows a team with no active subscription`, () => {
    expect(subscriptionBlocksTeamDelete(null)).toBe(false)
  })
})

describe(`assertTeamSubscriptionCancelled`, () => {
  it(`throws PRECONDITION_FAILED with the actionable cancel-first message`, () => {
    expect(() => assertTeamSubscriptionCancelled(LIVE)).toThrow(
      TEAM_DELETE_ACTIVE_SUBSCRIPTION_MESSAGE
    )
    try {
      assertTeamSubscriptionCancelled(LIVE)
    } catch (err) {
      expect((err as { code: string }).code).toBe(`PRECONDITION_FAILED`)
    }
  })

  it(`names Billing so the owner knows where to go`, () => {
    expect(TEAM_DELETE_ACTIVE_SUBSCRIPTION_MESSAGE).toContain(`Billing`)
  })

  // Three native clients match this error by its LEADING clause to render
  // their own delete-blocked copy — apps/android .../data/api/TrpcClient.kt,
  // apps/ios ExpCore/Sources/API/TrpcErrorInfo.swift and apps/desktop
  // crates/ui/src/settings/mod.rs. Rewording the prefix on the web silently
  // breaks all three, so pin it here.
  it(`keeps the leading clause the native matchers key off`, () => {
    expect(
      TEAM_DELETE_ACTIVE_SUBSCRIPTION_MESSAGE.startsWith(
        `This team has an active subscription`
      )
    ).toBe(true)
  })

  it(`passes for a scheduled cancellation and for no subscription`, () => {
    expect(() =>
      assertTeamSubscriptionCancelled({ ...LIVE, cancelAtPeriodEnd: true })
    ).not.toThrow()
    expect(() =>
      assertTeamSubscriptionCancelled({
        cancelAtPeriodEnd: false,
        status: SCHEDULED_CANCEL_STATUS,
      })
    ).not.toThrow()
    expect(() => assertTeamSubscriptionCancelled(null)).not.toThrow()
  })
})

describe(`assertTeamDeletableBilling`, () => {
  it(`refuses a team with a live subscription`, async () => {
    mocks.subscriptions.value = [LIVE]
    await expect(assertTeamDeletableBilling(`ws-1`)).rejects.toThrow(
      TEAM_DELETE_ACTIVE_SUBSCRIPTION_MESSAGE
    )
  })

  it(`allows a team whose subscription is scheduled to cancel`, async () => {
    mocks.subscriptions.value = [{ ...LIVE, cancelAtPeriodEnd: true }]
    await expect(
      assertTeamDeletableBilling(`ws-1`)
    ).resolves.toBeUndefined()
  })

  // scheduled_cancel is an ACTIVE status, so the row is still returned here —
  // the gate must let the delete through on the status alone.
  it(`allows a team whose subscription is scheduled_cancel in Creem`, async () => {
    mocks.subscriptions.value = [
      { cancelAtPeriodEnd: false, status: SCHEDULED_CANCEL_STATUS },
    ]
    await expect(
      assertTeamDeletableBilling(`ws-1`)
    ).resolves.toBeUndefined()
  })

  it(`allows a team with no active subscription at all`, async () => {
    mocks.subscriptions.value = []
    await expect(
      assertTeamDeletableBilling(`ws-1`)
    ).resolves.toBeUndefined()
  })

  // The REV2-30 duplicate window can leave TWO active rows on one team. The
  // gate reads them all, so a live one blocks no matter where it sits — a
  // most-seats-wins single read could have returned the scheduled_cancel row
  // and waved the delete through while the other subscription kept charging.
  it(`refuses when ANY of several rows is still live`, async () => {
    mocks.subscriptions.value = [
      { ...LIVE, cancelAtPeriodEnd: true },
      { cancelAtPeriodEnd: false, status: SCHEDULED_CANCEL_STATUS },
      LIVE,
    ]
    await expect(assertTeamDeletableBilling(`ws-1`)).rejects.toThrow(
      TEAM_DELETE_ACTIVE_SUBSCRIPTION_MESSAGE
    )
  })

  it(`allows a team whose every active row is already ending`, async () => {
    mocks.subscriptions.value = [
      { ...LIVE, cancelAtPeriodEnd: true },
      { cancelAtPeriodEnd: false, status: SCHEDULED_CANCEL_STATUS },
    ]
    await expect(
      assertTeamDeletableBilling(`ws-1`)
    ).resolves.toBeUndefined()
  })

  it(`skips the lookup entirely on self-hosted instances`, async () => {
    mocks.cloud.value = false
    mocks.subscriptions.value = [LIVE]
    await expect(
      assertTeamDeletableBilling(`ws-1`)
    ).resolves.toBeUndefined()
    expect(mocks.getCalls.count).toBe(0)
  })
})
