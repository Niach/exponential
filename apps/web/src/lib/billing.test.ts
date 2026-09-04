import { describe, expect, it, vi, beforeEach } from "vitest"
import { TRPCError } from "@trpc/server"

// Per-seat billing (EXP-286 tier model: free | team | unlimited). The
// db-backed helpers query drizzle chains and read isCloudInstance(); both are
// mocked so the resolution/gating logic can be exercised without Postgres.
// `db.select()` shifts the next pre-seeded result array off a FIFO queue —
// within any single billing helper the select order is deterministic, so the
// queue order matches call order.
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
  ACTIVE_STATUSES,
  countTeamWidgetSubmissionsLastHour,
  getPlanLimits,
  planFromSubscription,
  parseCompTier,
  resolveEffectiveTier,
  assertSeatAvailable,
  assertWidgetCreatable,
  getTeamPlan,
  getUserPlan,
  getTeamUsage,
  assertCanInviteMember,
  getInviteCapacity,
  resolveInviteCapacity,
  assertCanCreateWidget,
  assertCanUseHelpdesk,
  assertHelpdeskUsable,
  assertWithinStorageLimit,
  type PlanTier,
} from "./billing"
import { PLAN_LIMIT_MESSAGE_PREFIX } from "./plan-limit-error"

const TEAM_ID = `prod_team_monthly`
const TEAM_YEARLY_ID = `prod_team_yearly`
const WS = `11111111-1111-1111-1111-111111111111`
const USER = `user-1`

beforeEach(() => {
  selectResults.length = 0
  cloud.value = true
  process.env.CREEM_TEAM_PRODUCT_ID = TEAM_ID
  process.env.CREEM_TEAM_YEARLY_PRODUCT_ID = TEAM_YEARLY_ID
})

describe(`getPlanLimits — the tier table`, () => {
  it(`free = 3 seats / 250 MB / 1 widget / 60 submissions per hour`, () => {
    expect(getPlanLimits(`free`)).toEqual({
      seats: 3,
      storageMb: 250,
      widgetConfigs: 1,
      widgetSubmissionsPerHour: 60,
    })
  })
  it(`team = 10 GB / unlimited widgets + submissions`, () => {
    const team = getPlanLimits(`team`)
    expect(team.storageMb).toBe(10240)
    expect(team.widgetConfigs).toBe(Infinity)
    expect(team.widgetSubmissionsPerHour).toBe(Infinity)
  })
  it(`unlimited = everything Infinity`, () => {
    expect(getPlanLimits(`unlimited`)).toEqual({
      seats: Infinity,
      storageMb: Infinity,
      widgetConfigs: Infinity,
      widgetSubmissionsPerHour: Infinity,
    })
  })
})

describe(`planFromSubscription — team-bound resolution`, () => {
  it(`null subscription → free defaults`, () => {
    expect(planFromSubscription(null)).toEqual({
      plan: `free`,
      limits: {
        seats: 3,
        storageMb: 250,
        widgetConfigs: 1,
        widgetSubmissionsPerHour: 60,
      },
    })
  })

  it(`team subscription: purchased seats override the placeholder`, () => {
    const { plan, limits } = planFromSubscription({
      productId: TEAM_ID,
      seats: 7,
    })
    expect(plan).toBe(`team`)
    expect(limits.seats).toBe(7)
    expect(limits.storageMb).toBe(10240)
    expect(limits.widgetConfigs).toBe(Infinity)
  })

  it(`team (monthly + yearly product ids) both resolve to team`, () => {
    expect(planFromSubscription({ productId: TEAM_ID, seats: 3 }).plan).toBe(
      `team`
    )
    expect(
      planFromSubscription({ productId: TEAM_YEARLY_ID, seats: 3 }).plan
    ).toBe(`team`)
  })

  it(`invalid/zero seats fall back to 1 (never leaves a paid ws at 0 seats)`, () => {
    expect(
      planFromSubscription({ productId: TEAM_ID, seats: 0 }).limits.seats
    ).toBe(1)
    expect(
      planFromSubscription({ productId: TEAM_ID, seats: -5 }).limits.seats
    ).toBe(1)
  })

  it(`unknown productId fails closed to free (rotated/unset CREEM_* env)`, () => {
    const { plan, limits } = planFromSubscription({
      productId: `prod_unknown`,
      seats: 5,
    })
    expect(plan).toBe(`free`)
    expect(limits.storageMb).toBe(250)
    expect(limits.widgetConfigs).toBe(1)
  })

  it(`a configured id no longer matching after env rotation resolves free, not team`, () => {
    delete process.env.CREEM_TEAM_PRODUCT_ID
    expect(planFromSubscription({ productId: TEAM_ID, seats: 3 }).plan).toBe(
      `free`
    )
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

describe(`assertWidgetCreatable — per-tier count cap`, () => {
  it(`free allows its first config, blocks the second (1-widget cap)`, () => {
    const free = getPlanLimits(`free`)
    expect(() => assertWidgetCreatable(`free`, free, 0)).not.toThrow()
    expect(() => assertWidgetCreatable(`free`, free, 1)).toThrow(TRPCError)
  })
  it(`team allows many (unlimited configs)`, () => {
    const team = getPlanLimits(`team`)
    expect(() => assertWidgetCreatable(`team`, team, 99)).not.toThrow()
  })
})

describe(`parseCompTier — defensive column parse`, () => {
  it(`accepts the two grantable tiers`, () => {
    expect(parseCompTier(`team`)).toBe(`team`)
    expect(parseCompTier(`unlimited`)).toBe(`unlimited`)
  })
  it(`rejects null, undefined, free, legacy tiers, and garbage strings`, () => {
    expect(parseCompTier(null)).toBeNull()
    expect(parseCompTier(undefined)).toBeNull()
    expect(parseCompTier(`free`)).toBeNull()
    // The pre-EXP-286 tiers are a clean cut — migration 0054 rewrites any
    // stored `pro`/`business` comp to `team`; the parser must not resurrect
    // them.
    expect(parseCompTier(`pro`)).toBeNull()
    expect(parseCompTier(`business`)).toBeNull()
    expect(parseCompTier(`gold`)).toBeNull()
    expect(parseCompTier(``)).toBeNull()
  })
})

describe(`resolveEffectiveTier — comp floor (effective = max by rank)`, () => {
  it(`comp lifts a lower creem tier`, () => {
    expect(resolveEffectiveTier(`free`, `team`)).toBe(`team`)
    expect(resolveEffectiveTier(`free`, `unlimited`)).toBe(`unlimited`)
    expect(resolveEffectiveTier(`team`, `unlimited`)).toBe(`unlimited`)
  })
  it(`never lowers: a comp below the creem tier is a no-op`, () => {
    expect(resolveEffectiveTier(`unlimited`, `team`)).toBe(`unlimited`)
  })
  it(`equal tiers keep the creem tier (purchased seats stay authoritative)`, () => {
    expect(resolveEffectiveTier(`team`, `team`)).toBe(`team`)
  })
  it(`null / unknown comp values are ignored`, () => {
    expect(resolveEffectiveTier(`free`, null)).toBe(`free`)
    expect(resolveEffectiveTier(`team`, undefined)).toBe(`team`)
    expect(resolveEffectiveTier(`free`, `gold`)).toBe(`free`)
    expect(resolveEffectiveTier(`free`, `free`)).toBe(`free`)
  })
})

describe(`ACTIVE_STATUSES — which statuses still grant entitlements`, () => {
  // REV2-103: billing.cancelSubscription schedules the cancellation, and
  // Creem then reports `scheduled_cancel` until the paid period actually
  // ends. That time is bought and paid for, so the team stays on its plan —
  // omitting the status here would drop it to Free the second it clicked
  // Cancel (and, via getActiveTeamSubscription, hide the pending-cancel
  // banner + Resume button).
  it(`keeps a scheduled cancellation entitled until the period ends`, () => {
    expect(ACTIVE_STATUSES).toContain(`scheduled_cancel`)
  })

  it(`drops a subscription that actually ended`, () => {
    expect(ACTIVE_STATUSES).not.toContain(`canceled`)
    expect(ACTIVE_STATUSES).not.toContain(`unpaid`)
  })
})

describe(`getTeamPlan — team-bound lookup (no owner fan-out)`, () => {
  // Promise.all order inside getTeamPlan: subscription select first,
  // then the teams.comp_tier select.
  function seedPlan(sub: unknown[], compTier: string | null) {
    selectResults.push(sub)
    selectResults.push([{ compTier }])
  }

  it(`returns free when the team has no active subscription`, async () => {
    seedPlan([], null) // sub lookup: none, no comp
    expect(await getTeamPlan(WS)).toEqual({
      plan: `free`,
      limits: getPlanLimits(`free`),
    })
  })

  it(`resolves the plan + seats from the bound subscription row`, async () => {
    seedPlan([{ productId: TEAM_ID, seats: 12 }], null)
    const { plan, limits } = await getTeamPlan(WS)
    expect(plan).toBe(`team`)
    expect(limits.seats).toBe(12)
  })

  it(`comp tier lifts a free team — limits follow the comped tier`, async () => {
    seedPlan([], `team`)
    const { plan, limits } = await getTeamPlan(WS)
    expect(plan).toBe(`team`)
    expect(limits.storageMb).toBe(10240)
    expect(limits.widgetConfigs).toBe(Infinity)
    // No purchased quantity behind a comp → seats are uncapped, never 1.
    expect(limits.seats).toBe(Infinity)
  })

  it(`comp equal to the subscription tier keeps purchased seat gating`, async () => {
    seedPlan([{ productId: TEAM_ID, seats: 4 }], `team`)
    const { plan, limits } = await getTeamPlan(WS)
    expect(plan).toBe(`team`)
    expect(limits.seats).toBe(4)
  })

  it(`comp above the subscription tier wins and lifts limits`, async () => {
    seedPlan([{ productId: TEAM_ID, seats: 4 }], `unlimited`)
    const { plan, limits } = await getTeamPlan(WS)
    expect(plan).toBe(`unlimited`)
    expect(limits).toEqual(getPlanLimits(`unlimited`))
  })

  it(`garbage comp_tier values are ignored`, async () => {
    seedPlan([], `platinum`)
    expect((await getTeamPlan(WS)).plan).toBe(`free`)
  })

  it(`legacy pro/business comp values no longer resolve (migration 0054 rewrites them)`, async () => {
    seedPlan([], `business`)
    expect((await getTeamPlan(WS)).plan).toBe(`free`)
  })

  it(`missing team row degrades to the creem tier alone`, async () => {
    selectResults.push([]) // sub: none
    selectResults.push([]) // team row: gone
    expect((await getTeamPlan(WS)).plan).toBe(`free`)
  })

  it(`self-hosted short-circuits to unlimited without touching the db`, async () => {
    cloud.value = false
    // no selectResults pushed — a db hit would resolve to [] and misbehave,
    // proving the short-circuit fires first.
    expect(await getTeamPlan(WS)).toEqual({
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
  it(`any team subscription resolves to team`, async () => {
    selectResults.push([
      { productId: `prod_unknown` },
      { productId: TEAM_YEARLY_ID },
    ])
    expect((await getUserPlan(USER)).plan).toBe(`team`)
  })
  it(`self-hosted → unlimited`, async () => {
    cloud.value = false
    expect((await getUserPlan(USER)).plan).toBe(`unlimited`)
  })
})

describe(`getTeamUsage`, () => {
  it(`counts members, storage MB, and widget configs`, async () => {
    // Order matches getTeamUsage's Promise.all: members, issue-attachment
    // storage, session-attachment storage (EXP-702), widgets.
    selectResults.push([{ count: 1 }]) // members
    selectResults.push([{ totalBytes: `${3 * 1024 * 1024}` }]) // 3 MB issues
    selectResults.push([{ totalBytes: `${2 * 1024 * 1024}` }]) // 2 MB sessions
    selectResults.push([{ count: 2 }]) // widget configs
    const usage = await getTeamUsage(WS)
    expect(usage).toEqual({ members: 1, storageMb: 5, widgetConfigs: 2 })
  })
})

describe(`countTeamWidgetSubmissionsLastHour`, () => {
  it(`returns the joined trailing-hour count`, async () => {
    selectResults.push([{ count: 4 }])
    await expect(countTeamWidgetSubmissionsLastHour(WS)).resolves.toBe(4)
  })

  it(`empty result → 0`, async () => {
    selectResults.push([])
    await expect(countTeamWidgetSubmissionsLastHour(WS)).resolves.toBe(0)
  })
})

describe(`assertCanInviteMember — seat gate wired to team usage`, () => {
  async function seed(
    sub: unknown[],
    members: number,
    compTier: string | null = null
  ) {
    // Promise.all([getTeamPlan, getTeamUsage]) → sub select first,
    // then the comp-tier select, then usage's four selects (members,
    // issue-attachment storage, session-attachment storage, widgets).
    selectResults.push(sub) // getTeamPlan sub lookup
    selectResults.push([{ compTier }]) // getTeamPlan comp-tier lookup
    selectResults.push([{ count: members }]) // usage members
    selectResults.push([{ totalBytes: `0` }]) // usage storage (issues)
    selectResults.push([{ totalBytes: `0` }]) // usage storage (sessions)
    selectResults.push([{ count: 0 }]) // usage widgets
  }

  it(`allows the first invites on free (3 seats)`, async () => {
    await seed([], 1)
    await expect(assertCanInviteMember(WS)).resolves.toBeUndefined()
  })

  it(`blocks the invite that would exceed free's 3 seats`, async () => {
    await seed([], 3)
    await expect(assertCanInviteMember(WS)).rejects.toThrow(TRPCError)
  })

  it(`a comped team is never seat-gated (no purchased quantity)`, async () => {
    await seed([], 25, `team`)
    await expect(assertCanInviteMember(WS)).resolves.toBeUndefined()
  })

  it(`allows an invite when purchased seats exceed members`, async () => {
    await seed([{ productId: TEAM_ID, seats: 5 }], 2)
    await expect(assertCanInviteMember(WS)).resolves.toBeUndefined()
  })

  it(`blocks once members reach the purchased seat count (downgrade → invites only)`, async () => {
    await seed([{ productId: TEAM_ID, seats: 3 }], 3)
    await expect(assertCanInviteMember(WS)).rejects.toThrow(TRPCError)
  })

  it(`self-hosted never gates invites`, async () => {
    cloud.value = false
    await expect(assertCanInviteMember(WS)).resolves.toBeUndefined()
  })
})

describe(`resolveInviteCapacity — pure seat arithmetic (EXP-725)`, () => {
  it(`is null for non-finite seats (comp floor / unlimited)`, () => {
    expect(resolveInviteCapacity(Infinity, 40, 3)).toBeNull()
  })

  it(`subtracts members AND pending invites`, () => {
    expect(resolveInviteCapacity(3, 1, 0)).toBe(2)
    expect(resolveInviteCapacity(3, 1, 2)).toBe(0)
    expect(resolveInviteCapacity(5, 2, 1)).toBe(2)
  })

  it(`clamps an over-seat team at zero, never negative`, () => {
    expect(resolveInviteCapacity(3, 3, 1)).toBe(0)
    expect(resolveInviteCapacity(1, 4, 0)).toBe(0)
  })
})

describe(`getInviteCapacity — read-only capacity for the invite control`, () => {
  function seed(
    sub: unknown[],
    members: number,
    pending: number,
    compTier: string | null = null
  ) {
    // Promise.all([getTeamPlan, countTeamMembers, countPendingInvites]) issues
    // its selects in argument order: sub, comp tier, members, pending.
    selectResults.push(sub)
    selectResults.push([{ compTier }])
    selectResults.push([{ count: members }])
    selectResults.push([{ count: pending }])
  }

  it(`self-hosted is unlimited and never touches the db`, async () => {
    cloud.value = false
    await expect(getInviteCapacity(WS)).resolves.toEqual({ remaining: null })
    expect(selectResults).toHaveLength(0)
  })

  it(`free: 3 seats minus the owner leaves 2`, async () => {
    seed([], 1, 0)
    await expect(getInviteCapacity(WS)).resolves.toEqual({ remaining: 2 })
  })

  it(`pending invites hold seats before anyone accepts`, async () => {
    seed([], 1, 2)
    await expect(getInviteCapacity(WS)).resolves.toEqual({ remaining: 0 })
  })

  it(`an over-seat team reads as zero, not negative`, async () => {
    seed([], 3, 1)
    await expect(getInviteCapacity(WS)).resolves.toEqual({ remaining: 0 })
  })

  it(`a comped team is unlimited`, async () => {
    seed([], 25, 4, `team`)
    await expect(getInviteCapacity(WS)).resolves.toEqual({ remaining: null })
  })

  it(`purchased seats: 5 seats, 2 members, 1 pending → 2`, async () => {
    seed([{ productId: TEAM_ID, seats: 5 }], 2, 1)
    await expect(getInviteCapacity(WS)).resolves.toEqual({ remaining: 2 })
  })
})

describe(`assertCanCreateWidget — server-side widget-count gate`, () => {
  async function seed(
    sub: unknown[],
    widgets: number,
    compTier: string | null = null
  ) {
    selectResults.push(sub) // getTeamPlan sub lookup
    selectResults.push([{ compTier }]) // getTeamPlan comp-tier lookup
    selectResults.push([{ count: 1 }]) // usage members
    selectResults.push([{ totalBytes: `0` }]) // usage storage (issues)
    selectResults.push([{ totalBytes: `0` }]) // usage storage (sessions)
    selectResults.push([{ count: widgets }]) // usage widgets
  }

  it(`free team can create its first widget (EXP-180)`, async () => {
    await seed([], 0)
    await expect(assertCanCreateWidget(WS)).resolves.toBeUndefined()
  })

  it(`free team blocked at its 1-config cap`, async () => {
    await seed([], 1)
    await expect(assertCanCreateWidget(WS)).rejects.toThrow(TRPCError)
  })

  it(`team plan is never capped (unlimited configs)`, async () => {
    await seed([{ productId: TEAM_ID, seats: 3 }], 99)
    await expect(assertCanCreateWidget(WS)).resolves.toBeUndefined()
  })

  it(`self-hosted skips the gate`, async () => {
    cloud.value = false
    await expect(assertCanCreateWidget(WS)).resolves.toBeUndefined()
  })
})

describe(`assertCanUseHelpdesk — server-side paid gate`, () => {
  function seedPlan(sub: unknown[], compTier: string | null = null) {
    selectResults.push(sub) // getTeamPlan sub lookup
    selectResults.push([{ compTier }]) // getTeamPlan comp-tier lookup
  }

  it(`pure gate: free throws the plan-limit error, paid tiers pass`, () => {
    expect(() => assertHelpdeskUsable(`free`)).toThrow(/Team plan/)
    expect(() => assertHelpdeskUsable(`free`)).toThrow(
      new RegExp(PLAN_LIMIT_MESSAGE_PREFIX)
    )
    expect(() => assertHelpdeskUsable(`team`)).not.toThrow()
    expect(() => assertHelpdeskUsable(`unlimited`)).not.toThrow()
  })

  it(`free team cannot use the helpdesk`, async () => {
    seedPlan([])
    await expect(assertCanUseHelpdesk(WS)).rejects.toThrow(/Team plan/)
  })

  it(`team-plan team can`, async () => {
    seedPlan([{ productId: TEAM_ID, seats: 3 }])
    await expect(assertCanUseHelpdesk(WS)).resolves.toBeUndefined()
  })

  it(`a team comp unlocks it`, async () => {
    seedPlan([], `team`)
    await expect(assertCanUseHelpdesk(WS)).resolves.toBeUndefined()
  })

  it(`self-hosted skips the gate`, async () => {
    cloud.value = false
    await expect(assertCanUseHelpdesk(WS)).resolves.toBeUndefined()
  })
})

describe(`assertWithinStorageLimit — per-team storage budget`, () => {
  async function seed(
    sub: unknown[],
    usedMb: number,
    compTier: string | null = null
  ) {
    selectResults.push(sub) // getTeamPlan sub lookup
    selectResults.push([{ compTier }]) // getTeamPlan comp-tier lookup
    selectResults.push([{ count: 1 }]) // usage members
    selectResults.push([{ totalBytes: `${usedMb * 1024 * 1024}` }]) // issues
    selectResults.push([{ totalBytes: `0` }]) // sessions
    selectResults.push([{ count: 0 }]) // widgets
  }

  it(`free team blocked once an upload would exceed 250 MB`, async () => {
    await seed([], 250)
    await expect(assertWithinStorageLimit(WS, 1)).rejects.toThrow(TRPCError)
  })

  it(`a team comp lifts the storage budget past the free cap`, async () => {
    await seed([], 250, `team`)
    await expect(assertWithinStorageLimit(WS, 1024)).resolves.toBeUndefined()
  })

  it(`free team allows an upload that fits`, async () => {
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
  it(`accepts the three tiers`, () => {
    const tiers: PlanTier[] = [`free`, `team`, `unlimited`]
    expect(tiers).toHaveLength(3)
  })
})
