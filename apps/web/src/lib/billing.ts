import { TRPCError } from "@trpc/server"
import { and, desc, eq, inArray, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  workspaceMembers,
  workspaces,
  attachments,
  creem_subscriptions,
  widgetConfigs,
  users,
} from "@/db/schema"
import { getFeedbackWorkspaceId, isCloudInstance } from "@/lib/bootstrap-cloud"
import { PLAN_LIMIT_MESSAGE_PREFIX } from "@/lib/plan-limit-error"

export type PlanTier = `free` | `pro` | `business` | `unlimited`

// Per-seat model (masterplan v5 §3.2, L19–L22). The ONLY monetized axes are
// seats (team size), storage per workspace, and feedback-widget configs.
// Projects, repositories, and coding-session capacity are unlimited on every
// tier. Push + email notification delivery and remote steer are FREE on every
// tier and are never plan-gated — do NOT add booleans for them here.
type PlanLimits = {
  // Purchased seats a workspace may fill with non-agent members. Free = 1;
  // paid tiers override this placeholder with the subscription's purchased
  // quantity (see planFromSubscription).
  seats: number
  // Attachment storage budget per workspace, in megabytes.
  storageMb: number
  // Feedback-widget configs a workspace may create — a Pro+ feature. Free = 0
  // (widget gated off entirely), Pro = 1, Business = unlimited.
  widgetConfigs: number
}

// NOTE: the `seats` value on the paid tiers is only a placeholder — the real
// seat allowance is the subscription's purchased quantity, applied in
// planFromSubscription. Free stays at a hard 1 (the owner alone).
const PLAN_LIMITS: Record<PlanTier, PlanLimits> = {
  free: {
    seats: 1,
    storageMb: 250,
    widgetConfigs: 0,
  },
  pro: {
    seats: 1,
    storageMb: 5120,
    widgetConfigs: 1,
  },
  business: {
    seats: 1,
    storageMb: 51200,
    widgetConfigs: Infinity,
  },
  unlimited: {
    seats: Infinity,
    storageMb: Infinity,
    widgetConfigs: Infinity,
  },
}

// Invisible abuse guard (§3.2): a FREE user may own at most this many
// workspaces. Not shown in any pricing UI — it only exists to stop storage
// farming (N free workspaces × the per-workspace storage budget). Paid users
// have no cap.
export const FREE_OWNED_WORKSPACES_CAP = 10

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
  if (productId === process.env.CREEM_BUSINESS_PRODUCT_ID) return `business`
  if (productId === process.env.CREEM_BUSINESS_YEARLY_PRODUCT_ID) return `business`
  if (productId === process.env.CREEM_PRO_PRODUCT_ID) return `pro`
  // Fail closed: an unrecognized product id (rotated/decommissioned product,
  // unset CREEM_*_PRODUCT_ID env) must not grant paid entitlements — a
  // Business customer silently under-provisioned to Pro would go unnoticed,
  // and a legacy product would be over-granted forever.
  if (!warnedUnknownProductIds.has(productId)) {
    warnedUnknownProductIds.add(productId)
    console.warn(
      `[billing] subscription productId "${productId}" matches no configured CREEM_*_PRODUCT_ID — treating as free (check the env configuration)`
    )
  }
  return `free`
}

const ACTIVE_STATUSES = [`active`, `trialing`, `paid`]

// Rank order for the comp-tier floor (EXP-49): an admin-granted complimentary
// tier (workspaces.comp_tier) can only ever RAISE a workspace's effective
// tier, never lower it.
const TIER_RANK: Record<PlanTier, number> = {
  free: 0,
  pro: 1,
  business: 2,
  unlimited: 3,
}

// Defensive parse of the raw workspaces.comp_tier column value. `free` is not
// a valid comp value (a floor of free is a no-op), and an unknown string must
// be IGNORED rather than crash or distort plan resolution — the column is
// plain text, not a Postgres enum.
export function parseCompTier(
  value: string | null | undefined
): PlanTier | null {
  if (value === `pro` || value === `business` || value === `unlimited`) {
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

// Pure resolution: a workspace's plan + effective limits from its single active
// workspace-bound subscription (or `null` → free). The subscription's purchased
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

// Workspace-bound plan resolution (L19). A subscription belongs to ONE
// workspace (creem_subscriptions.workspaceId) — no owner fan-out. When a
// workspace somehow carries more than one active subscription we take the one
// with the most seats so a team is never accidentally under-provisioned.
// An admin-granted comp tier (workspaces.comp_tier) acts as a FLOOR over the
// Creem-derived tier — effective plan = max of the two by rank (EXP-49).
export async function getWorkspacePlan(
  workspaceId: string
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
          eq(creem_subscriptions.workspaceId, workspaceId),
          inArray(creem_subscriptions.status, ACTIVE_STATUSES)
        )
      )
      .orderBy(desc(creem_subscriptions.seats))
      .limit(1),
    db
      .select({ compTier: workspaces.compTier })
      .from(workspaces)
      .where(eq(workspaces.id, workspaceId))
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
// workspace. Only used by the free-tier owned-workspace abuse guard and the
// userPlan pre-gate — seats are not meaningful cross-workspace, so this returns
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
    if (tier === `business`) {
      bestPlan = `business`
      break
    }
    if (tier === `pro` && bestPlan === `free`) {
      bestPlan = `pro`
    }
  }

  return { plan: bestPlan, limits: PLAN_LIMITS[bestPlan] }
}

// Number of workspaces the user OWNS. The bootstrap feedback workspace is
// excluded (it's shared infra that admins "own" but shouldn't be billed for).
export async function countOwnedWorkspaces(userId: string): Promise<number> {
  const feedbackWorkspaceId = await getFeedbackWorkspaceId()
  const [row] = await db
    .select({ count: sql<number>`count(*)::int` })
    .from(workspaceMembers)
    .innerJoin(workspaces, eq(workspaces.id, workspaceMembers.workspaceId))
    .where(
      sql`${workspaceMembers.userId} = ${userId} AND ${workspaceMembers.role} = 'owner'${feedbackWorkspaceId ? sql` AND ${workspaces.id} <> ${feedbackWorkspaceId}` : sql``}`
    )
  return row?.count ?? 0
}

export type WorkspaceUsage = {
  // Human members only — the widget's synthetic isAgent user is excluded so a
  // fresh single-owner workspace reads "1 member", never "2".
  members: number
  storageMb: number
  widgetConfigs: number
}

export async function getWorkspaceUsage(
  workspaceId: string
): Promise<WorkspaceUsage> {
  const [[memberCount], [storageSum], [widgetCount]] = await Promise.all([
    db
      .select({ count: sql<number>`count(*)::int` })
      .from(workspaceMembers)
      .innerJoin(users, eq(users.id, workspaceMembers.userId))
      .where(
        and(
          eq(workspaceMembers.workspaceId, workspaceId),
          eq(users.isAgent, false)
        )
      ),
    db
      .select({
        totalBytes: sql<string>`coalesce(sum(${attachments.sizeBytes}), 0)::bigint`,
      })
      .from(attachments)
      .where(eq(attachments.workspaceId, workspaceId)),
    db
      .select({ count: sql<number>`count(*)::int` })
      .from(widgetConfigs)
      .where(eq(widgetConfigs.workspaceId, workspaceId)),
  ])

  const totalBytes = Number(storageSum.totalBytes)

  return {
    members: memberCount.count,
    storageMb: Math.round((totalBytes / (1024 * 1024)) * 10) / 10,
    widgetConfigs: widgetCount.count,
  }
}

// Pure seat gate (§3.3, L19): a workspace may hold at most `seats` non-agent
// members. Throws the plan-limit error when full. Exported for unit tests.
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

// Invite-time seat check (workspace-invites.create/accept). Current member
// count EXCLUDING isAgent users must be below the purchased seat count.
// Downgrade policy (L19/§3.2): this ONLY blocks NEW invites — it never removes
// or locks out existing members.
export async function assertCanInviteMember(
  workspaceId: string
): Promise<void> {
  if (!isCloudInstance()) return

  const [{ limits }, usage] = await Promise.all([
    getWorkspacePlan(workspaceId),
    getWorkspaceUsage(workspaceId),
  ])

  assertSeatAvailable(usage.members, limits.seats)
}

// Pure widget gate (§3.3(4)): the feedback widget is a Pro+ feature, capped at
// the tier's widgetConfigs allowance. Exported for unit tests.
export function assertWidgetCreatable(
  plan: PlanTier,
  limits: PlanLimits,
  currentCount: number
): void {
  if (plan === `free`) {
    throw planLimitError(
      `the feedback widget on Pro and Business plans. Upgrade to add a widget.`
    )
  }
  if (currentCount >= limits.widgetConfigs) {
    throw planLimitError(
      `up to ${limits.widgetConfigs} widget config${
        limits.widgetConfigs === 1 ? `` : `s`
      }. Upgrade to add more.`
    )
  }
}

// Pure helpdesk gate: the support inbox is a Pro+ feature (no per-tier count —
// it's a per-project boolean). Exported for unit tests.
export function assertHelpdeskUsable(plan: PlanTier): void {
  if (plan === `free`) {
    throw planLimitError(
      `the helpdesk on Pro and Business plans. Upgrade to enable support conversations.`
    )
  }
}

// Helpdesk gate (projects.update helpdesk_enabled flip + support-thread
// creation). Self-hosted is unlimited.
export async function assertCanUseHelpdesk(workspaceId: string): Promise<void> {
  if (!isCloudInstance()) return
  const { plan } = await getWorkspacePlan(workspaceId)
  assertHelpdeskUsable(plan)
}

// Widget-create gate (widgets.create). Self-hosted is unlimited; the bootstrap
// dogfood path inserts directly and is intentionally exempt.
export async function assertCanCreateWidget(
  workspaceId: string
): Promise<void> {
  if (!isCloudInstance()) return

  const [{ plan, limits }, usage] = await Promise.all([
    getWorkspacePlan(workspaceId),
    getWorkspaceUsage(workspaceId),
  ])

  assertWidgetCreatable(plan, limits, usage.widgetConfigs)
}

export async function assertWithinStorageLimit(
  workspaceId: string,
  additionalBytes: number
): Promise<void> {
  if (!isCloudInstance()) return

  const [{ limits }, usage] = await Promise.all([
    getWorkspacePlan(workspaceId),
    getWorkspaceUsage(workspaceId),
  ])

  if (limits.storageMb === Infinity) return

  const limitBytes = limits.storageMb * 1024 * 1024
  const currentBytes = usage.storageMb * 1024 * 1024
  if (currentBytes + additionalBytes > limitBytes) {
    throw planLimitError(
      `up to ${limits.storageMb >= 1024 ? `${Math.round(limits.storageMb / 1024)} GB` : `${limits.storageMb} MB`} of storage. Upgrade to upload more.`
    )
  }
}
