import { describe, expect, it, vi } from "vitest"

vi.mock(`@/db/connection`, () => ({ db: {} }))

import {
  lifecycleEventName,
  recordSubscriptionLifecycleEvent,
} from "@/lib/conversion/subscription-events"

const row = {
  userId: `user_1`,
  teamId: `team_1`,
  seats: 5,
  productId: `prod_1`,
}

type RecordedArgs = {
  name: string
  userId?: string | null
  properties?: Record<string, unknown> | null
}

function deps(overrides: Partial<{ row: typeof row | null }> = {}) {
  const record = vi.fn(async (_dbx: unknown, _args: RecordedArgs) => {})
  const loadSubscription = vi.fn(async (_id: string) =>
    overrides.row === undefined ? row : overrides.row
  )
  return { record, loadSubscription }
}

function recordedArgs(d: ReturnType<typeof deps>, call = 0): RecordedArgs {
  const args = d.record.mock.calls[call]?.[1]
  expect(args).toBeDefined()
  return args!
}

describe(`lifecycleEventName`, () => {
  it(`maps statuses to event names`, () => {
    expect(lifecycleEventName({ status: `active` })).toBe(
      `subscription_first_active`
    )
    expect(lifecycleEventName({ status: `paid` })).toBe(
      `subscription_first_active`
    )
    expect(lifecycleEventName({ status: `canceled`, terminal: true })).toBe(
      `subscription_canceled`
    )
    // Non-conversion statuses record nothing — including `trialing`
    // (no trial products exist; free + Team only).
    expect(lifecycleEventName({ status: `trialing` })).toBeNull()
    expect(lifecycleEventName({ status: `scheduled_cancel` })).toBeNull()
    expect(lifecycleEventName({ status: `unpaid` })).toBeNull()
    expect(lifecycleEventName({ status: null })).toBeNull()
  })
})

describe(`recordSubscriptionLifecycleEvent`, () => {
  it(`records first-active with the bound row's user/team/seats`, async () => {
    const d = deps()
    await recordSubscriptionLifecycleEvent(
      { creemSubscriptionId: `sub_1`, status: `active` },
      d
    )
    expect(d.record).toHaveBeenCalledTimes(1)
    const args = recordedArgs(d)
    expect(args).toMatchObject({
      name: `subscription_first_active`,
      userId: `user_1`,
      properties: {
        creemSubscriptionId: `sub_1`,
        teamId: `team_1`,
        seats: 5,
        productId: `prod_1`,
        status: `active`,
      },
    })
  })

  it(`records subscription_canceled for terminal events`, async () => {
    const d = deps()
    await recordSubscriptionLifecycleEvent(
      { creemSubscriptionId: `sub_1`, status: `expired`, terminal: true },
      d
    )
    expect(recordedArgs(d).name).toBe(`subscription_canceled`)
  })

  it(`records nothing without a subscription id or mappable status`, async () => {
    const d = deps()
    await recordSubscriptionLifecycleEvent(
      { creemSubscriptionId: null, status: `active` },
      d
    )
    await recordSubscriptionLifecycleEvent(
      { creemSubscriptionId: `  `, status: `active` },
      d
    )
    await recordSubscriptionLifecycleEvent(
      { creemSubscriptionId: `sub_1`, status: `paused` },
      d
    )
    expect(d.record).not.toHaveBeenCalled()
  })

  it(`falls back to metadata.referenceId when the row is missing`, async () => {
    const d = deps({ row: null })
    await recordSubscriptionLifecycleEvent(
      {
        creemSubscriptionId: `sub_1`,
        status: `active`,
        metadata: { referenceId: `user_meta` },
      },
      d
    )
    expect(recordedArgs(d)).toMatchObject({
      userId: `user_meta`,
      properties: { creemSubscriptionId: `sub_1` },
    })
  })

  it(`never throws when a dep fails`, async () => {
    const record = vi.fn(async () => {})
    const loadSubscription = vi.fn(async () => {
      throw new Error(`db down`)
    })
    await expect(
      recordSubscriptionLifecycleEvent(
        { creemSubscriptionId: `sub_1`, status: `active` },
        { record, loadSubscription }
      )
    ).resolves.toBeUndefined()
  })
})
