import { describe, expect, it, vi, beforeEach } from "vitest"
import { TRPCError } from "@trpc/server"

// Per-seat billing (masterplan v5 §3). The db-backed helpers query drizzle
// chains and read isCloudInstance(); both are mocked so the resolution/gating
// logic can be exercised without Postgres. `db.select()` shifts the next
// pre-seeded result array off a FIFO queue — within any single billing helper
// the select order is deterministic, so the queue order matches call order.
const { selectResults, cloud } = vi.hoisted(() => ({
  selectResults: [] as unknown[][],
  cloud: { value: true },
}))

function chain(): Promise<unknown[]> & Record<string, () => unknown> {
  const p = Promise.resolve(
    selectResults.shift() ?? []
  ) as Promise<unknown[]> & Record<string, () => unknown>
  for (const m of [
    `from`,
    `where`,
    `innerJoin`,
    `leftJoin`,
    `orderBy`,
    `groupBy`,
    `limit`,
  ]) {
    p[m] = () => p
  }
  return p
}

vi.mock(`@/db/connection`, () => ({
  db: { select: () => chain() },
}))

vi.mock(`@/lib/bootstrap-cloud`, () => ({
  isCloudInstance: () => cloud.value,
}))

import {
  getPlanLimits,
  planFromSubscription,
  parseCompTier,
  resolveEffectiveTier,
  assertSeatAvailable,
  assertWidgetCreatable,
  getWorkspacePlan,
  getUserPlan,
  getWorkspaceUsage,
  assertCanInviteMember,
  assertCanCreateWidget,
  assertCanUseHelpdesk,
  assertHelpdeskUsable,
  assertWithinStorageLimit,
  type PlanTier,
} from "./billing"
import { PLAN_LIMIT_MESSAGE_PREFIX } from "./plan-limit-error"

const PRO_ID = `prod_pro_yearly`
const BUSINESS_ID = `prod_business_monthly`
const BUSINESS_YEARLY_ID = `prod_business_yearly`
const WS = `11111111-1111-1111-1111-111111111111`
const USER = `user-1`

beforeEach(() => {
  selectResults.length = 0
  cloud.value = true
  process.env.CREEM_PRO_PRODUCT_ID = PRO_ID
  process.env.CREEM_BUSINESS_PRODUCT_ID = BUSINESS_ID
  process.env.CREEM_BUSINESS_YEARLY_PRODUCT_ID = BUSINESS_YEARLY_ID
})

describe(`getPlanLimits — the §3.2 target table`, () => {
  it(`free = 1 seat / 250 MB / 0 widgets`, () => {
    expect(getPlanLimits(`free`)).toEqual({
      seats: 1,
      storageMb: 250,
      widgetConfigs: 0,
    })
  })
  it(`pro = 5 GB / 1 widget`, () => {
    const pro = getPlanLimits(`pro`)
    expect(pro.storageMb).toBe(5120)
    expect(pro.widgetConfigs).toBe(1)
  })
  it(`business = 50 GB / unlimited widgets`, () => {
    const biz = getPlanLimits(`business`)
    expect(biz.storageMb).toBe(51200)
    expect(biz.widgetConfigs).toBe(Infinity)
  })
  it(`unlimited = everything Infinity`, () => {
    expect(getPlanLimits(`unlimited`)).toEqual({
      seats: Infinity,
      storageMb: Infinity,
      widgetConfigs: Infinity,
    })
  })
})

describe(`planFromSubscription — workspace-bound resolution`, () => {
  it(`null subscription → free defaults`, () => {
    expect(planFromSubscription(null)).toEqual({
      plan: `free`,
      limits: { seats: 1, storageMb: 250, widgetConfigs: 0 },
    })
  })

  it(`pro subscription: purchased seats override the placeholder`, () => {
    const { plan, limits } = planFromSubscription({
      productId: PRO_ID,
      seats: 7,
    })
    expect(plan).toBe(`pro`)
    expect(limits.seats).toBe(7)
    expect(limits.storageMb).toBe(5120)
    expect(limits.widgetConfigs).toBe(1)
  })

  it(`business (monthly + yearly product ids) both resolve to business`, () => {
    expect(planFromSubscription({ productId: BUSINESS_ID, seats: 3 }).plan).toBe(
      `business`
    )
    expect(
      planFromSubscription({ productId: BUSINESS_YEARLY_ID, seats: 3 }).plan
    ).toBe(`business`)
  })

  it(`invalid/zero seats fall back to 1 (never leaves a paid ws at 0 seats)`, () => {
    expect(planFromSubscription({ productId: PRO_ID, seats: 0 }).limits.seats).toBe(
      1
    )
    expect(
      planFromSubscription({ productId: PRO_ID, seats: -5 }).limits.seats
    ).toBe(1)
  })

  it(`unknown productId fails closed to free (rotated/unset CREEM_* env)`, () => {
    const { plan, limits } = planFromSubscription({
      productId: `prod_unknown`,
      seats: 5,
    })
    expect(plan).toBe(`free`)
    expect(limits.storageMb).toBe(250)
    expect(limits.widgetConfigs).toBe(0)
  })

  it(`a configured id no longer matching after env rotation resolves free, not pro`, () => {
    delete process.env.CREEM_BUSINESS_PRODUCT_ID
    expect(
      planFromSubscription({ productId: BUSINESS_ID, seats: 3 }).plan
    ).toBe(`free`)
  })
})

describe(`assertSeatAvailable — the invite-time seat gate`, () => {
  it(`allows an invite while under the seat count`, () => {
    expect(() => assertSeatAvailable(1, 3)).not.toThrow()
  })
  it(`blocks when seats are full`, () => {
    expect(() => assertSeatAvailable(3, 3)).toThrow(TRPCError)
  })
  it(`error names seats and carries the plan-limit prefix`, () => {
    try {
      assertSeatAvailable(1, 1)
      throw new Error(`should have thrown`)
    } catch (e) {
      const err = e as TRPCError
      expect(err.code).toBe(`PRECONDITION_FAILED`)
      expect(err.message).toContain(PLAN_LIMIT_MESSAGE_PREFIX)
      expect(err.message).toContain(`1 seat`)
    }
  })
})

describe(`assertWidgetCreatable — Pro+ widget gate`, () => {
  it(`free is blocked entirely (widget is a Pro feature)`, () => {
    expect(() =>
      assertWidgetCreatable(`free`, getPlanLimits(`free`), 0)
    ).toThrow(/Pro and Business/)
  })
  it(`pro allows the first config, blocks the second`, () => {
    const pro = getPlanLimits(`pro`)
    expect(() => assertWidgetCreatable(`pro`, pro, 0)).not.toThrow()
    expect(() => assertWidgetCreatable(`pro`, pro, 1)).toThrow(TRPCError)
  })
  it(`business allows many (unlimited configs)`, () => {
    const biz = getPlanLimits(`business`)
    expect(() => assertWidgetCreatable(`business`, biz, 99)).not.toThrow()
  })
})

describe(`parseCompTier — defensive column parse`, () => {
  it(`accepts the three grantable tiers`, () => {
    expect(parseCompTier(`pro`)).toBe(`pro`)
    expect(parseCompTier(`business`)).toBe(`business`)
    expect(parseCompTier(`unlimited`)).toBe(`unlimited`)
  })
  it(`rejects null, undefined, free, and garbage strings`, () => {
    expect(parseCompTier(null)).toBeNull()
    expect(parseCompTier(undefined)).toBeNull()
    expect(parseCompTier(`free`)).toBeNull()
    expect(parseCompTier(`gold`)).toBeNull()
    expect(parseCompTier(``)).toBeNull()
  })
})

describe(`resolveEffectiveTier — comp floor (effective = max by rank)`, () => {
  it(`comp lifts a lower creem tier`, () => {
    expect(resolveEffectiveTier(`free`, `pro`)).toBe(`pro`)
    expect(resolveEffectiveTier(`free`, `business`)).toBe(`business`)
    expect(resolveEffectiveTier(`pro`, `business`)).toBe(`business`)
    expect(resolveEffectiveTier(`free`, `unlimited`)).toBe(`unlimited`)
  })
  it(`never lowers: a comp below the creem tier is a no-op`, () => {
    expect(resolveEffectiveTier(`business`, `pro`)).toBe(`business`)
    expect(resolveEffectiveTier(`unlimited`, `business`)).toBe(`unlimited`)
  })
  it(`equal tiers keep the creem tier (purchased seats stay authoritative)`, () => {
    expect(resolveEffectiveTier(`business`, `business`)).toBe(`business`)
  })
  it(`null / unknown comp values are ignored`, () => {
    expect(resolveEffectiveTier(`free`, null)).toBe(`free`)
    expect(resolveEffectiveTier(`pro`, undefined)).toBe(`pro`)
    expect(resolveEffectiveTier(`free`, `gold`)).toBe(`free`)
    expect(resolveEffectiveTier(`free`, `free`)).toBe(`free`)
  })
})

describe(`getWorkspacePlan — workspace-bound lookup (no owner fan-out)`, () => {
  // Promise.all order inside getWorkspacePlan: subscription select first,
  // then the workspaces.comp_tier select.
  function seedPlan(sub: unknown[], compTier: string | null) {
    selectResults.push(sub)
    selectResults.push([{ compTier }])
  }

  it(`returns free when the workspace has no active subscription`, async () => {
    seedPlan([], null) // sub lookup: none, no comp
    expect(await getWorkspacePlan(WS)).toEqual({
      plan: `free`,
      limits: getPlanLimits(`free`),
    })
  })

  it(`resolves the plan + seats from the bound subscription row`, async () => {
    seedPlan([{ productId: PRO_ID, seats: 12 }], null)
    const { plan, limits } = await getWorkspacePlan(WS)
    expect(plan).toBe(`pro`)
    expect(limits.seats).toBe(12)
  })

  it(`comp tier lifts a free workspace — limits follow the comped tier`, async () => {
    seedPlan([], `business`)
    const { plan, limits } = await getWorkspacePlan(WS)
    expect(plan).toBe(`business`)
    expect(limits.storageMb).toBe(51200)
    expect(limits.widgetConfigs).toBe(Infinity)
    // No purchased quantity behind a comp → seats are uncapped, never 1.
    expect(limits.seats).toBe(Infinity)
  })

  it(`comp below the subscription tier changes nothing`, async () => {
    seedPlan([{ productId: BUSINESS_ID, seats: 8 }], `pro`)
    const { plan, limits } = await getWorkspacePlan(WS)
    expect(plan).toBe(`business`)
    expect(limits.seats).toBe(8)
  })

  it(`comp equal to the subscription tier keeps purchased seat gating`, async () => {
    seedPlan([{ productId: PRO_ID, seats: 4 }], `pro`)
    const { plan, limits } = await getWorkspacePlan(WS)
    expect(plan).toBe(`pro`)
    expect(limits.seats).toBe(4)
  })

  it(`comp above the subscription tier wins and lifts limits`, async () => {
    seedPlan([{ productId: PRO_ID, seats: 4 }], `unlimited`)
    const { plan, limits } = await getWorkspacePlan(WS)
    expect(plan).toBe(`unlimited`)
    expect(limits).toEqual(getPlanLimits(`unlimited`))
  })

  it(`garbage comp_tier values are ignored`, async () => {
    seedPlan([], `platinum`)
    expect((await getWorkspacePlan(WS)).plan).toBe(`free`)
  })

  it(`missing workspace row degrades to the creem tier alone`, async () => {
    selectResults.push([]) // sub: none
    selectResults.push([]) // workspace row: gone
    expect((await getWorkspacePlan(WS)).plan).toBe(`free`)
  })

  it(`self-hosted short-circuits to unlimited without touching the db`, async () => {
    cloud.value = false
    // no selectResults pushed — a db hit would resolve to [] and misbehave,
    // proving the short-circuit fires first.
    expect(await getWorkspacePlan(WS)).toEqual({
      plan: `unlimited`,
      limits: getPlanLimits(`unlimited`),
    })
  })
})

describe(`getUserPlan — best purchased tier for the abuse guard`, () => {
  it(`free when the user bought nothing`, async () => {
    selectResults.push([])
    expect((await getUserPlan(USER)).plan).toBe(`free`)
  })
  it(`business wins over pro across multiple subs`, async () => {
    selectResults.push([{ productId: PRO_ID }, { productId: BUSINESS_ID }])
    expect((await getUserPlan(USER)).plan).toBe(`business`)
  })
  it(`self-hosted → unlimited`, async () => {
    cloud.value = false
    expect((await getUserPlan(USER)).plan).toBe(`unlimited`)
  })
})

describe(`getWorkspaceUsage — agent-excluded member count`, () => {
  it(`counts members, storage MB, and widget configs`, async () => {
    // Order matches getWorkspaceUsage's Promise.all: members, storage, widgets.
    selectResults.push([{ count: 1 }]) // members (already agent-excluded by SQL)
    selectResults.push([{ totalBytes: `${5 * 1024 * 1024}` }]) // 5 MB
    selectResults.push([{ count: 2 }]) // widget configs
    const usage = await getWorkspaceUsage(WS)
    expect(usage).toEqual({ members: 1, storageMb: 5, widgetConfigs: 2 })
  })
})

describe(`assertCanInviteMember — seat gate wired to workspace usage`, () => {
  async function seed(
    sub: unknown[],
    members: number,
    compTier: string | null = null
  ) {
    // Promise.all([getWorkspacePlan, getWorkspaceUsage]) → sub select first,
    // then the comp-tier select, then usage's three selects (members,
    // storage, widgets).
    selectResults.push(sub) // getWorkspacePlan sub lookup
    selectResults.push([{ compTier }]) // getWorkspacePlan comp-tier lookup
    selectResults.push([{ count: members }]) // usage members
    selectResults.push([{ totalBytes: `0` }]) // usage storage
    selectResults.push([{ count: 0 }]) // usage widgets
  }

  it(`blocks the first invite on free (1 seat, owner already fills it)`, async () => {
    await seed([], 1)
    await expect(assertCanInviteMember(WS)).rejects.toThrow(TRPCError)
  })

  it(`a comped workspace is never seat-gated (no purchased quantity)`, async () => {
    await seed([], 25, `pro`)
    await expect(assertCanInviteMember(WS)).resolves.toBeUndefined()
  })

  it(`allows an invite when purchased seats exceed members`, async () => {
    await seed([{ productId: PRO_ID, seats: 5 }], 2)
    await expect(assertCanInviteMember(WS)).resolves.toBeUndefined()
  })

  it(`blocks once members reach the purchased seat count (downgrade → invites only)`, async () => {
    await seed([{ productId: PRO_ID, seats: 3 }], 3)
    await expect(assertCanInviteMember(WS)).rejects.toThrow(TRPCError)
  })

  it(`self-hosted never gates invites`, async () => {
    cloud.value = false
    await expect(assertCanInviteMember(WS)).resolves.toBeUndefined()
  })
})

describe(`assertCanCreateWidget — server-side Pro gate`, () => {
  async function seed(
    sub: unknown[],
    widgets: number,
    compTier: string | null = null
  ) {
    selectResults.push(sub) // getWorkspacePlan sub lookup
    selectResults.push([{ compTier }]) // getWorkspacePlan comp-tier lookup
    selectResults.push([{ count: 1 }]) // usage members
    selectResults.push([{ totalBytes: `0` }]) // usage storage
    selectResults.push([{ count: widgets }]) // usage widgets
  }

  it(`free workspace cannot create a widget`, async () => {
    await seed([], 0)
    await expect(assertCanCreateWidget(WS)).rejects.toThrow(/Pro and Business/)
  })

  it(`a pro comp unlocks the widget (the comp floor lifts limits too)`, async () => {
    await seed([], 0, `pro`)
    await expect(assertCanCreateWidget(WS)).resolves.toBeUndefined()
  })

  it(`pro workspace can create its first widget`, async () => {
    await seed([{ productId: PRO_ID, seats: 3 }], 0)
    await expect(assertCanCreateWidget(WS)).resolves.toBeUndefined()
  })

  it(`pro workspace blocked at its 1-config cap`, async () => {
    await seed([{ productId: PRO_ID, seats: 3 }], 1)
    await expect(assertCanCreateWidget(WS)).rejects.toThrow(TRPCError)
  })

  it(`self-hosted skips the gate`, async () => {
    cloud.value = false
    await expect(assertCanCreateWidget(WS)).resolves.toBeUndefined()
  })
})

describe(`assertCanUseHelpdesk — server-side Pro gate`, () => {
  function seedPlan(sub: unknown[], compTier: string | null = null) {
    selectResults.push(sub) // getWorkspacePlan sub lookup
    selectResults.push([{ compTier }]) // getWorkspacePlan comp-tier lookup
  }

  it(`pure gate: free throws the plan-limit error, paid tiers pass`, () => {
    expect(() => assertHelpdeskUsable(`free`)).toThrow(/Pro and Business/)
    expect(() => assertHelpdeskUsable(`free`)).toThrow(
      new RegExp(PLAN_LIMIT_MESSAGE_PREFIX)
    )
    expect(() => assertHelpdeskUsable(`pro`)).not.toThrow()
    expect(() => assertHelpdeskUsable(`business`)).not.toThrow()
    expect(() => assertHelpdeskUsable(`unlimited`)).not.toThrow()
  })

  it(`free workspace cannot use the helpdesk`, async () => {
    seedPlan([])
    await expect(assertCanUseHelpdesk(WS)).rejects.toThrow(/Pro and Business/)
  })

  it(`pro workspace can`, async () => {
    seedPlan([{ productId: PRO_ID, seats: 3 }])
    await expect(assertCanUseHelpdesk(WS)).resolves.toBeUndefined()
  })

  it(`a pro comp unlocks it`, async () => {
    seedPlan([], `pro`)
    await expect(assertCanUseHelpdesk(WS)).resolves.toBeUndefined()
  })

  it(`self-hosted skips the gate`, async () => {
    cloud.value = false
    await expect(assertCanUseHelpdesk(WS)).resolves.toBeUndefined()
  })
})

describe(`assertWithinStorageLimit — per-workspace storage budget`, () => {
  async function seed(
    sub: unknown[],
    usedMb: number,
    compTier: string | null = null
  ) {
    selectResults.push(sub) // getWorkspacePlan sub lookup
    selectResults.push([{ compTier }]) // getWorkspacePlan comp-tier lookup
    selectResults.push([{ count: 1 }]) // usage members
    selectResults.push([{ totalBytes: `${usedMb * 1024 * 1024}` }]) // storage
    selectResults.push([{ count: 0 }]) // widgets
  }

  it(`free workspace blocked once an upload would exceed 250 MB`, async () => {
    await seed([], 250)
    await expect(assertWithinStorageLimit(WS, 1)).rejects.toThrow(TRPCError)
  })

  it(`a business comp lifts the storage budget past the free cap`, async () => {
    await seed([], 250, `business`)
    await expect(assertWithinStorageLimit(WS, 1024)).resolves.toBeUndefined()
  })

  it(`free workspace allows an upload that fits`, async () => {
    await seed([], 10)
    await expect(assertWithinStorageLimit(WS, 1024)).resolves.toBeUndefined()
  })

  it(`self-hosted (unlimited storage) never blocks`, async () => {
    cloud.value = false
    await expect(
      assertWithinStorageLimit(WS, 999 * 1024 * 1024)
    ).resolves.toBeUndefined()
  })
})

describe(`PlanTier type export is usable`, () => {
  it(`accepts the four tiers`, () => {
    const tiers: PlanTier[] = [`free`, `pro`, `business`, `unlimited`]
    expect(tiers).toHaveLength(4)
  })
})
