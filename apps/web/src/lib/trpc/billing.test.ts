import { beforeEach, describe, expect, it, vi } from "vitest"

// billing.cancelSubscription / resumeSubscription (REV2-55): the ONE
// user-facing cancel path, and the prerequisite for deleting a paying team.
// Everything the router touches (DB, Creem, membership) is faked.
const mocks = vi.hoisted(() => ({
  subscription: null as {
    id: string
    productId: string
    creemSubscriptionId: string | null
    seats: number
    periodEnd: Date | null
    cancelAtPeriodEnd: boolean
    status: string
  } | null,
  updates: [] as Array<Record<string, unknown>>,
  schedule: vi.fn(async () => {}),
  resume: vi.fn(async () => {}),
  access: vi.fn(async () => ({ role: `owner` })),
}))

vi.mock(`@/db/connection`, () => ({
  db: {
    update: () => ({
      set: (values: Record<string, unknown>) => ({
        where: async () => {
          mocks.updates.push(values)
        },
      }),
    }),
  },
}))

vi.mock(`@/lib/bootstrap-cloud`, () => ({
  isCloudInstance: () => true,
}))

vi.mock(`@/lib/team-membership`, () => ({
  resolveTeamAccess: (...args: unknown[]) => mocks.access(...(args as [])),
}))

vi.mock(`@/lib/billing`, () => ({
  countOwnedTeams: vi.fn(async () => 0),
  getUserPlan: vi.fn(async () => ({ plan: `free` })),
  getTeamPlan: vi.fn(async () => ({ plan: `free`, limits: {} })),
  getTeamUsage: vi.fn(async () => ({})),
  FREE_OWNED_TEAMS_CAP: 10,
}))

vi.mock(`@creem_io/better-auth/server`, () => ({
  createCheckout: vi.fn(async () => ({ url: `https://creem.test/checkout` })),
}))

vi.mock(`@/lib/billing/creem-subscriptions`, () => ({
  assertSubscriptionMutable: vi.fn(),
  getActiveTeamSubscription: async () => mocks.subscription,
  resumeCreemSubscription: (...args: unknown[]) =>
    mocks.resume(...(args as [])),
  scheduleCreemSubscriptionCancellation: (...args: unknown[]) =>
    mocks.schedule(...(args as [])),
  updateCreemSubscriptionSeats: vi.fn(async () => 1),
  upgradeCreemSubscriptionProduct: vi.fn(async () => {}),
}))

import { billingRouter } from "@/lib/trpc/billing"

const WS = `11111111-1111-4111-8111-111111111111`
const PERIOD_END = new Date(`2026-12-01T00:00:00.000Z`)
const ORIGINAL_KEY = process.env.CREEM_API_KEY

function caller() {
  return billingRouter.createCaller({
    session: { user: { id: `user-a`, email: `a@example.com` } },
    db: {},
  } as never)
}

beforeEach(() => {
  process.env.CREEM_API_KEY = ORIGINAL_KEY ?? `creem_test_key`
  mocks.subscription = {
    id: `row-1`,
    productId: `prod_pro`,
    creemSubscriptionId: `sub_1`,
    seats: 3,
    periodEnd: PERIOD_END,
    cancelAtPeriodEnd: false,
    status: `active`,
  }
  mocks.updates.length = 0
  mocks.schedule.mockClear()
  mocks.resume.mockClear()
  mocks.access.mockClear()
})

describe(`billing.cancelSubscription`, () => {
  it(`schedules cancellation and records the pending state locally`, async () => {
    const result = await caller().cancelSubscription({ teamId: WS })

    expect(mocks.schedule).toHaveBeenCalledWith(`sub_1`)
    expect(mocks.updates).toEqual([
      expect.objectContaining({ cancelAtPeriodEnd: true }),
    ])
    expect(result).toEqual({
      cancelAtPeriodEnd: true,
      periodEnd: PERIOD_END.toISOString(),
    })
  })

  it(`is owner-only`, async () => {
    await caller().cancelSubscription({ teamId: WS })
    expect(mocks.access).toHaveBeenCalledWith(
      `user-a`,
      WS,
      `mutate_resources`,
      { roles: [`owner`] }
    )
  })

  it(`is idempotent once a cancellation is already scheduled`, async () => {
    mocks.subscription = { ...mocks.subscription!, cancelAtPeriodEnd: true }

    const result = await caller().cancelSubscription({ teamId: WS })

    expect(result.cancelAtPeriodEnd).toBe(true)
    expect(mocks.schedule).not.toHaveBeenCalled()
    expect(mocks.updates).toHaveLength(0)
  })

  // REV2-103: a cancellation scheduled outside this router (Creem dashboard,
  // support, or an optimistic write we lost) shows up ONLY as Creem's
  // `scheduled_cancel` status — re-scheduling it must stay a no-op.
  it(`is idempotent from Creem's scheduled_cancel status alone`, async () => {
    mocks.subscription = {
      ...mocks.subscription!,
      cancelAtPeriodEnd: false,
      status: `scheduled_cancel`,
    }

    const result = await caller().cancelSubscription({ teamId: WS })

    expect(result.cancelAtPeriodEnd).toBe(true)
    expect(mocks.schedule).not.toHaveBeenCalled()
    expect(mocks.updates).toHaveLength(0)
  })

  it(`refuses when the team has no active subscription`, async () => {
    mocks.subscription = null
    await expect(
      caller().cancelSubscription({ teamId: WS })
    ).rejects.toThrow(/no active subscription/)
  })

  it(`refuses a legacy row with no Creem subscription id`, async () => {
    mocks.subscription = { ...mocks.subscription!, creemSubscriptionId: null }
    await expect(
      caller().cancelSubscription({ teamId: WS })
    ).rejects.toThrow(/contact support/)
    expect(mocks.schedule).not.toHaveBeenCalled()
  })
})

describe(`billing.resumeSubscription`, () => {
  it(`resumes a scheduled cancellation and clears the local flag`, async () => {
    mocks.subscription = { ...mocks.subscription!, cancelAtPeriodEnd: true }

    const result = await caller().resumeSubscription({ teamId: WS })

    expect(mocks.resume).toHaveBeenCalledWith(`sub_1`)
    expect(mocks.updates).toEqual([
      expect.objectContaining({ cancelAtPeriodEnd: false }),
    ])
    expect(result).toEqual({ cancelAtPeriodEnd: false })
  })

  // The status is the only signal for a dashboard-side cancellation, and the
  // row is still ACTIVE (scheduled_cancel is an entitled status), so Resume
  // must work off it — and clear it, or the banner would stick around until
  // the confirming webhook landed.
  it(`resumes from Creem's scheduled_cancel status and clears it`, async () => {
    mocks.subscription = {
      ...mocks.subscription!,
      cancelAtPeriodEnd: false,
      status: `scheduled_cancel`,
    }

    const result = await caller().resumeSubscription({ teamId: WS })

    expect(mocks.resume).toHaveBeenCalledWith(`sub_1`)
    expect(mocks.updates).toEqual([
      expect.objectContaining({ cancelAtPeriodEnd: false, status: `active` }),
    ])
    expect(result).toEqual({ cancelAtPeriodEnd: false })
  })

  it(`leaves a non-scheduled status untouched when resuming`, async () => {
    mocks.subscription = {
      ...mocks.subscription!,
      cancelAtPeriodEnd: true,
      status: `trialing`,
    }

    await caller().resumeSubscription({ teamId: WS })

    expect(mocks.updates).toEqual([
      expect.objectContaining({ status: `trialing` }),
    ])
  })

  it(`refuses when nothing is scheduled to cancel`, async () => {
    await expect(
      caller().resumeSubscription({ teamId: WS })
    ).rejects.toThrow(/not scheduled to cancel/)
    expect(mocks.resume).not.toHaveBeenCalled()
  })
})

describe(`billing.teamPlan — pending-cancel banner state`, () => {
  it(`derives cancelAtPeriodEnd from Creem's scheduled_cancel status`, async () => {
    mocks.subscription = {
      ...mocks.subscription!,
      cancelAtPeriodEnd: false,
      status: `scheduled_cancel`,
    }

    const result = await caller().teamPlan({ teamId: WS })

    expect(result.subscription).toMatchObject({
      cancelAtPeriodEnd: true,
      periodEnd: PERIOD_END.toISOString(),
    })
  })

  it(`reports a live subscription as not pending cancel`, async () => {
    const result = await caller().teamPlan({ teamId: WS })
    expect(result.subscription).toMatchObject({ cancelAtPeriodEnd: false })
  })
})
