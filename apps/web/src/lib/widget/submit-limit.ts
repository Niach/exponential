import { isCloudInstance } from "@/lib/bootstrap-cloud"
import { getPlanLimits, getTeamPlan } from "@/lib/billing"
import { TtlPromiseCache } from "@/lib/ttl-promise-cache"
import {
  getWidgetRateLimiters,
  TokenBucketLimiter,
  type RateLimitResult,
} from "@/lib/widget/rate-limit"

// Plan-aware widget submit gate (EXP-459). Self-hosted keeps the classic
// env-tunable per-key bucket untouched (no billing code runs — EXP-364: plan
// limits exist only on cloud). Cloud replaces it with a per-TEAM bucket sized
// by the team's plan: the team is the billing boundary, and paid tiers are
// simply unlimited on this axis. The per-IP abuse bucket in the submit route
// stays global and plan-independent either way.
//
// Lives in its own module so rate-limit.ts stays pure (billing.ts pulls in
// the db connection at import time).

type TeamPlan = Awaited<ReturnType<typeof getTeamPlan>>

// A rejected lookup is evicted on settle by TtlPromiseCache, so a transient
// DB error never poisons the cache; the route surfaces it as its usual 500.
let teamPlanCache: TtlPromiseCache<TeamPlan> | null = null

// One shared limiter works because free is the only tier with a finite
// ceiling — revisit if PLAN_LIMITS ever grows a second finite rate.
let freeTeamLimiter: TokenBucketLimiter | null = null

function getTeamPlanCached(teamId: string): Promise<TeamPlan> {
  teamPlanCache ??= new TtlPromiseCache<TeamPlan>({
    ttlMs: 30_000,
    maxEntries: 5_000,
  })
  return teamPlanCache.get(teamId, () => getTeamPlan(teamId))
}

export async function takeWidgetSubmitToken(
  config: { publicKey: string; teamId: string },
  nowMs = Date.now()
): Promise<RateLimitResult> {
  if (!isCloudInstance()) {
    return getWidgetRateLimiters().perKeyLimiter.tryTake(
      `key:${config.publicKey}`,
      nowMs
    )
  }

  const { limits } = await getTeamPlanCached(config.teamId)
  if (limits.widgetSubmissionsPerHour === Infinity) return { ok: true }

  freeTeamLimiter ??= new TokenBucketLimiter({
    capacity: 10,
    refillPerHour: getPlanLimits(`free`).widgetSubmissionsPerHour,
  })
  return freeTeamLimiter.tryTake(`team:${config.teamId}`, nowMs)
}

export function resetWidgetSubmitLimitStateForTest(): void {
  teamPlanCache = null
  freeTeamLimiter = null
}
