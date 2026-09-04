import { TRPCError } from "@trpc/server"
import { and, desc, eq, gt, gte, inArray, isNull, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  teamMembers,
  teams,
  attachments,
  sessionAttachments,
  creem_subscriptions,
  teamInvites,
  widgetConfigs,
  widgetSubmissions,
} from "@/db/schema"
import { isCloudInstance } from "@/lib/bootstrap-cloud"
import { PLAN_LIMIT_MESSAGE_PREFIX } from "@/lib/plan-limit-error"

export type PlanTier = `free` | `team` | `unlimited`

// Per-seat model (EXP-286 rebrand). The ONLY monetized axes are
// seats (team size), storage per team, and feedback-widget configs.
// Boards, repositories, and coding-session capacity are unlimited on every
// tier. Push + email notification delivery and remote steer are FREE on every
// tier and are never plan-gated — do NOT add booleans for them here.
type PlanLimits = {
  // Purchased seats a team may fill with members. Free = 3;
  // the paid tier overrides this placeholder with the subscription's purchased
  // quantity (see planFromSubscription).
  seats: number
  // Attachment storage budget per team, in megabytes.
  storageMb: number
  // Feedback-widget configs a team may create. Free = 1 (EXP-180),
  // Team = unlimited.
  widgetConfigs: number
  // Widget submissions per hour, aggregated per TEAM (the billing boundary —
  // paid teams have unlimited widget configs, so a per-key ceiling would be
  // trivially bypassed). Enforced only by the widget submit path on cloud
  // (lib/widget/submit-limit.ts); the per-IP abuse bucket stays global and
  // plan-independent.
  widgetSubmissionsPerHour: number
}

// NOTE: the `seats` value on the paid tier is only a placeholder — the real
// seat allowance is the subscription's purchased quantity, applied in
// planFromSubscription. Free stays at a hard 3 (enough to experience the
// realtime-collaboration core single-team, EXP-286).
const PLAN_LIMITS: Record<PlanTier, PlanLimits> = {
  free: {
    seats: 3,
    storageMb: 250,
    widgetConfigs: 1,
    widgetSubmissionsPerHour: 60,
  },
  team: {
    seats: 1,
    storageMb: 10240,
    widgetConfigs: Infinity,
    widgetSubmissionsPerHour: Infinity,
  },
  unlimited: {
    seats: Infinity,
    storageMb: Infinity,
    widgetConfigs: Infinity,
    widgetSubmissionsPerHour: Infinity,
  },
}

// Invisible abuse guard: a FREE user may own at most this many
// teams. Not shown in any pricing UI — it only exists to stop storage
// farming (N free teams × the per-team storage budget). Paid users
// have no cap.
export const FREE_OWNED_TEAMS_CAP = 10

// Every plan-limit throw below uses PRECONDITION_FAILED + a message starting
// with PLAN_LIMIT_MESSAGE_PREFIX so clients can detect it and render an
// upgrade nudge (see lib/plan-limit-error.ts).
function planLimitError(message: string): TRPCError {
  return new TRPCError({
    code: `PRECONDITION_FAILED`,
    message: `${PLAN_LIMIT_MESSAGE_PREFIX} ${message}`,
  })
}

export function getPlanLimits(plan: PlanTier): PlanLimits {
  return PLAN_LIMITS[plan]
}

// Deduped so a single stale subscription row can't flood the logs — the warn
// exists to make an env misconfiguration visible, once per product id.
const warnedUnknownProductIds = new Set<string>()

function productIdToTier(productId: string): PlanTier {
  if (productId === process.env.CREEM_TEAM_PRODUCT_ID) return `team`
  if (productId === process.env.CREEM_TEAM_YEARLY_PRODUCT_ID) return `team`
  // Fail closed: an unrecognized product id (rotated/decommissioned product,
  // unset CREEM_*_PRODUCT_ID env) must not grant paid entitlements — a
  // paying customer silently under-provisioned to Free would be noticed and
  // fixed, while a legacy product would be over-granted forever.
  if (!warnedUnknownProductIds.has(productId)) {
    warnedUnknownProductIds.add(productId)
    console.warn(
      `[billing] subscription productId "${productId}" matches no configured CREEM_*_PRODUCT_ID — treating as free (check the env configuration)`
    )
  }
  return `free`
}

// Statuses that still grant entitlements. Mirrors (and is asserted equal to)
// ACTIVE_SUBSCRIPTION_STATUSES in lib/billing/creem-subscriptions.ts, which
// documents why `scheduled_cancel` belongs here: a subscription scheduled to
// cancel is paid through `periodEnd`, so the team keeps its plan until Creem's
// `subscription.canceled` webhook lands. Exported for that parity test.
export const ACTIVE_STATUSES = [
  `active`,
  `trialing`,
  `paid`,
  `scheduled_cancel`,
]

// Rank order for the comp-tier floor (EXP-49): an admin-granted complimentary
// tier (teams.comp_tier) can only ever RAISE a team's effective
// tier, never lower it.
const TIER_RANK: Record<PlanTier, number> = {
  free: 0,
  team: 1,
  unlimited: 2,
}

// Defensive parse of the raw teams.comp_tier column value. `free` is not
// a valid comp value (a floor of free is a no-op), and an unknown string must
// be IGNORED rather than crash or distort plan resolution — the column is
// plain text, not a Postgres enum.
export function parseCompTier(
  value: string | null | undefined
): PlanTier | null {
  if (value === `team` || value === `unlimited`) {
    return value
  }
  return null
}

// Pure comp-floor resolution: effective tier = max(Creem-derived tier, comp
// tier) by rank. Exported so the floor logic can be unit-tested without a DB.
export function resolveEffectiveTier(
  creemTier: PlanTier,
  compTier: string | null | undefined
): PlanTier {
  const comp = parseCompTier(compTier)
  if (!comp) return creemTier
  return TIER_RANK[comp] > TIER_RANK[creemTier] ? comp : creemTier
}

export type ActiveSubscription = { productId: string; seats: number }

// Pure resolution: a team's plan + effective limits from its single active
// team-bound subscription (or `null` → free). The subscription's purchased
// seat quantity overrides the tier's placeholder seat count. Exported so the
// resolution can be unit-tested without a DB.
export function planFromSubscription(subscription: ActiveSubscription | null): {
  plan: PlanTier
  limits: PlanLimits
} {
  if (!subscription) {
    return { plan: `free`, limits: PLAN_LIMITS.free }
  }
  const plan = productIdToTier(subscription.productId)
  const seats =
    Number.isInteger(subscription.seats) && subscription.seats > 0
      ? subscription.seats
      : 1
  return { plan, limits: { ...PLAN_LIMITS[plan], seats } }
}

// Team-bound plan resolution (L19). A subscription belongs to ONE
// team (creem_subscriptions.teamId) — no owner fan-out. When a
// team somehow carries more than one active subscription we take the one
// with the most seats so a team is never accidentally under-provisioned.
// An admin-granted comp tier (teams.comp_tier) acts as a FLOOR over the
// Creem-derived tier — effective plan = max of the two by rank (EXP-49).
export async function getTeamPlan(
  teamId: string
): Promise<{ plan: PlanTier; limits: PlanLimits }> {
  if (!isCloudInstance()) {
    return { plan: `unlimited`, limits: PLAN_LIMITS.unlimited }
  }

  const [[sub], [ws]] = await Promise.all([
    db
      .select({
        productId: creem_subscriptions.productId,
        seats: creem_subscriptions.seats,
      })
      .from(creem_subscriptions)
      .where(
        and(
          eq(creem_subscriptions.teamId, teamId),
          inArray(creem_subscriptions.status, ACTIVE_STATUSES)
        )
      )
      .orderBy(desc(creem_subscriptions.seats))
      .limit(1),
    db
      .select({ compTier: teams.compTier })
      .from(teams)
      .where(eq(teams.id, teamId))
      .limit(1),
  ])

  const base = planFromSubscription(sub ?? null)
  const effective = resolveEffectiveTier(base.plan, ws?.compTier ?? null)
  if (effective === base.plan) return base
  // The comp floor won: limits follow the comped tier. There is no purchased
  // seat quantity behind a comp (and the paid tiers' placeholder of 1 would
  // strand a comped team unable to invite anyone), so comped seats are
  // uncapped — comping is an admin trust decision. A subscription can only
  // reclaim seat gating by outranking (or matching) the comp tier.
  return {
    plan: effective,
    limits: { ...PLAN_LIMITS[effective], seats: Infinity },
  }
}

// User-scoped entitlement: the best plan a user has personally purchased
// (creem_subscriptions.referenceId → the buyer), independent of any single
// team. Only used by the free-tier owned-team abuse guard and the
// userPlan pre-gate — seats are not meaningful cross-team, so this returns
// the tier's base limits.
export async function getUserPlan(
  userId: string
): Promise<{ plan: PlanTier; limits: PlanLimits }> {
  if (!isCloudInstance()) {
    return { plan: `unlimited`, limits: PLAN_LIMITS.unlimited }
  }

  const subs = await db
    .select({ productId: creem_subscriptions.productId })
    .from(creem_subscriptions)
    .where(
      and(
        eq(creem_subscriptions.referenceId, userId),
        inArray(creem_subscriptions.status, ACTIVE_STATUSES)
      )
    )

  let bestPlan: PlanTier = `free`
  for (const sub of subs) {
    const tier = productIdToTier(sub.productId)
    if (tier === `team`) {
      bestPlan = `team`
      break
    }
  }

  return { plan: bestPlan, limits: PLAN_LIMITS[bestPlan] }
}

// Number of teams the user OWNS.
export async function countOwnedTeams(userId: string): Promise<number> {
  const [row] = await db
    .select({ count: sql<number>`count(*)::int` })
    .from(teamMembers)
    .where(
      sql`${teamMembers.userId} = ${userId} AND ${teamMembers.role} = 'owner'`
    )
  return row?.count ?? 0
}

export type TeamUsage = {
  members: number
  storageMb: number
  widgetConfigs: number
}

export async function getTeamUsage(
  teamId: string
): Promise<TeamUsage> {
  const [memberCount, [storageSum], [sessionStorageSum], [widgetCount]] =
    await Promise.all([
      countTeamMembers(teamId),
      db
        .select({
          totalBytes: sql<string>`coalesce(sum(${attachments.sizeBytes}), 0)::bigint`,
        })
        .from(attachments)
        .where(eq(attachments.teamId, teamId)),
      // Steer images (EXP-702) — server-only rows, but their blobs occupy
      // the same per-team storage budget.
      db
        .select({
          totalBytes: sql<string>`coalesce(sum(${sessionAttachments.sizeBytes}), 0)::bigint`,
        })
        .from(sessionAttachments)
        .where(eq(sessionAttachments.teamId, teamId)),
      db
        .select({ count: sql<number>`count(*)::int` })
        .from(widgetConfigs)
        .where(eq(widgetConfigs.teamId, teamId)),
    ])

  const totalBytes =
    Number(storageSum.totalBytes) + Number(sessionStorageSum.totalBytes)

  return {
    members: memberCount,
    storageMb: Math.round((totalBytes / (1024 * 1024)) * 10) / 10,
    widgetConfigs: widgetCount.count,
  }
}

// Widget submissions filed for the team's widgets in the trailing hour —
// display-only fuel for the settings usage bar (enforcement is the in-process
// token bucket in lib/widget/submit-limit.ts, which the two deliberately
// approximate: the bucket also counts honeypot-dropped attempts the DB never
// sees). NOT part of getTeamUsage — that runs on hot enforcement paths
// (invites, widget create, attachment upload) that don't need this join.
export async function countTeamWidgetSubmissionsLastHour(
  teamId: string
): Promise<number> {
  const [row] = await db
    .select({ count: sql<number>`count(*)::int` })
    .from(widgetSubmissions)
    .innerJoin(
      widgetConfigs,
      eq(widgetConfigs.id, widgetSubmissions.widgetConfigId)
    )
    .where(
      and(
        eq(widgetConfigs.teamId, teamId),
        gte(widgetSubmissions.createdAt, sql`now() - interval '1 hour'`)
      )
    )
  return row?.count ?? 0
}

// Pure seat gate: a team may hold at most `seats` members. Throws the
// plan-limit error when full. Exported for unit tests.
export function assertSeatAvailable(
  memberCount: number,
  seats: number
): void {
  if (memberCount >= seats) {
    throw planLimitError(
      `up to ${seats} seat${seats === 1 ? `` : `s`}. Add seats or upgrade to invite more teammates.`
    )
  }
}

// Team-create gate (teams.create): the invisible free-tier abuse guard —
// a FREE user may own at most FREE_OWNED_TEAMS_CAP teams (storage
// farming: N free teams × the per-team storage budget). Paid users and
// self-hosted instances are uncapped.
export async function assertCanCreateTeam(userId: string): Promise<void> {
  if (!isCloudInstance()) return

  const { plan } = await getUserPlan(userId)
  if (plan !== `free`) return

  const owned = await countOwnedTeams(userId)
  if (owned >= FREE_OWNED_TEAMS_CAP) {
    throw planLimitError(
      `up to ${FREE_OWNED_TEAMS_CAP} teams on the free plan. Upgrade to create more.`
    )
  }
}

// Invite-time seat check (team-invites.create/accept). Current member
// count must be below the purchased seat count. Downgrade policy (L19/§3.2):
// this ONLY blocks NEW invites — it never removes or locks out existing
// members.
export async function assertCanInviteMember(
  teamId: string
): Promise<void> {
  if (!isCloudInstance()) return

  const [{ limits }, usage] = await Promise.all([
    getTeamPlan(teamId),
    getTeamUsage(teamId),
  ])

  assertSeatAvailable(usage.members, limits.seats)
}

// EXP-725: invite CAPACITY, the read-only sibling of assertCanInviteMember.
// The natives gate their invite control on it (App Store 3.1.1: the control
// is removed at the cap, never explained), so no plan vocabulary leaves here:
// `remaining` is a bare count, `null` = unlimited (JSON cannot carry
// Infinity). Unlike the gate it counts PENDING invites too, so two owners
// minting in parallel converge on "hidden" before a seat is actually taken.
export type InviteCapacity = { remaining: number | null }

// Pending = unaccepted AND unexpired. Expired rows are dead weight the accept
// path rejects anyway, so they must not hold a seat.
export async function countPendingInvites(teamId: string): Promise<number> {
  const [row] = await db
    .select({ count: sql<number>`count(*)::int` })
    .from(teamInvites)
    .where(
      and(
        eq(teamInvites.teamId, teamId),
        isNull(teamInvites.acceptedAt),
        gt(teamInvites.expiresAt, new Date())
      )
    )
  return row?.count ?? 0
}

// Lean member count: this runs on every members/invites shape change on the
// natives, so it must not drag getTeamUsage's storage sums along.
export async function countTeamMembers(teamId: string): Promise<number> {
  const [row] = await db
    .select({ count: sql<number>`count(*)::int` })
    .from(teamMembers)
    .where(eq(teamMembers.teamId, teamId))
  return row?.count ?? 0
}

// Pure, exported for unit tests. Non-finite seats (comp floor, unlimited)
// → null; otherwise clamped at zero so an over-seat team reads as "none",
// never negative.
export function resolveInviteCapacity(
  seats: number,
  members: number,
  pending: number
): number | null {
  if (!Number.isFinite(seats)) return null
  return Math.max(0, seats - members - pending)
}

export async function getInviteCapacity(
  teamId: string
): Promise<InviteCapacity> {
  if (!isCloudInstance()) return { remaining: null }

  const [{ limits }, members, pending] = await Promise.all([
    getTeamPlan(teamId),
    countTeamMembers(teamId),
    countPendingInvites(teamId),
  ])

  return { remaining: resolveInviteCapacity(limits.seats, members, pending) }
}

// Pure widget gate: every tier may create widgets, capped at the tier's
// widgetConfigs allowance (1 on Free). Exported for unit tests.
export function assertWidgetCreatable(
  _plan: PlanTier,
  limits: PlanLimits,
  currentCount: number
): void {
  if (currentCount >= limits.widgetConfigs) {
    throw planLimitError(
      `up to ${limits.widgetConfigs} widget config${
        limits.widgetConfigs === 1 ? `` : `s`
      }. Upgrade to add more.`
    )
  }
}

// Pure helpdesk gate: the support inbox is a paid feature (no per-tier count —
// it's a per-team boolean). Exported for unit tests.
export function assertHelpdeskUsable(plan: PlanTier): void {
  if (plan === `free`) {
    throw planLimitError(
      `the helpdesk on the Team plan. Upgrade to enable support conversations.`
    )
  }
}

// Helpdesk gate (teams.update helpdesk_enabled flip + support-thread
// creation). Self-hosted is unlimited.
export async function assertCanUseHelpdesk(teamId: string): Promise<void> {
  if (!isCloudInstance()) return
  const { plan } = await getTeamPlan(teamId)
  assertHelpdeskUsable(plan)
}

// Widget-create gate (widgets.create). Self-hosted is unlimited.
export async function assertCanCreateWidget(
  teamId: string
): Promise<void> {
  if (!isCloudInstance()) return

  const [{ plan, limits }, usage] = await Promise.all([
    getTeamPlan(teamId),
    getTeamUsage(teamId),
  ])

  assertWidgetCreatable(plan, limits, usage.widgetConfigs)
}

export async function assertWithinStorageLimit(
  teamId: string,
  additionalBytes: number
): Promise<void> {
  if (!isCloudInstance()) return

  const [{ limits }, usage] = await Promise.all([
    getTeamPlan(teamId),
    getTeamUsage(teamId),
  ])

  if (limits.storageMb === Infinity) return

  const limitBytes = limits.storageMb * 1024 * 1024
  const currentBytes = usage.storageMb * 1024 * 1024
  if (currentBytes + additionalBytes > limitBytes) {
    throw planLimitError(
      `up to ${limits.storageMb >= 1024 ? `${Math.round(limits.storageMb / 1024)} GB` : `${limits.storageMb} MB`} of attachment storage. Upgrade to upload more.`
    )
  }
}
