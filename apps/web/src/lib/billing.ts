import { TRPCError } from "@trpc/server"
import { and, eq, isNull, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  workspaceMembers,
  workspaces,
  projects,
  attachments,
  codingSessions,
  repositories,
  creem_subscriptions,
} from "@/db/schema"
import { isCloudInstance } from "@/lib/bootstrap-cloud"
import { PLAN_LIMIT_MESSAGE_PREFIX } from "@/lib/plan-limit-error"

export type PlanTier = `free` | `pro` | `business` | `unlimited`

// Push + email notification delivery are FREE on every tier (never plan-gated)
// — the moat is seats/projects/repos/storage/coding capacity, never "nothing
// gets lost". Do NOT add `push` or `email` booleans here.
type PlanLimits = {
  members: number
  projects: number
  storageMb: number
  // Connected GitHub repositories per workspace (the coding-flow value axis).
  repositories: number
  // How many `running` coding_sessions a workspace may have at once — the
  // capacity axis for the desktop coding superpower.
  concurrentCodingSessions: number
  // Number of non-public workspaces a user may OWN. Capping this closes the
  // hole where one free user spins up N workspaces × the per-workspace project
  // quota. `ownedWorkspaces` is required so the compiler flags every literal.
  ownedWorkspaces: number
}

const PLAN_LIMITS: Record<PlanTier, PlanLimits> = {
  free: {
    members: 1,
    projects: 3,
    storageMb: 50,
    repositories: 1,
    concurrentCodingSessions: 1,
    ownedWorkspaces: 1,
  },
  pro: {
    members: 5,
    projects: 10,
    storageMb: 1024,
    repositories: 10,
    concurrentCodingSessions: 3,
    ownedWorkspaces: 3,
  },
  business: {
    members: 25,
    projects: Infinity,
    storageMb: 10240,
    repositories: Infinity,
    concurrentCodingSessions: Infinity,
    ownedWorkspaces: 10,
  },
  unlimited: {
    members: Infinity,
    projects: Infinity,
    storageMb: Infinity,
    repositories: Infinity,
    concurrentCodingSessions: Infinity,
    ownedWorkspaces: Infinity,
  },
}

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

function productIdToTier(productId: string): PlanTier {
  if (productId === process.env.CREEM_PRO_PRODUCT_ID) return `pro`
  if (productId === process.env.CREEM_BUSINESS_PRODUCT_ID) return `business`
  return `pro`
}

const ACTIVE_STATUSES = [`active`, `trialing`, `paid`]

export async function getWorkspacePlan(
  workspaceId: string
): Promise<{ plan: PlanTier; limits: PlanLimits }> {
  if (!isCloudInstance()) {
    return { plan: `unlimited`, limits: PLAN_LIMITS.unlimited }
  }

  const owners = await db
    .select({ userId: workspaceMembers.userId })
    .from(workspaceMembers)
    .where(
      sql`${workspaceMembers.workspaceId} = ${workspaceId} AND ${workspaceMembers.role} = 'owner'`
    )

  if (owners.length === 0) {
    return { plan: `free`, limits: PLAN_LIMITS.free }
  }

  const ownerIds = owners.map((o) => o.userId)

  const subs = await db
    .select({
      productId: creem_subscriptions.productId,
      status: creem_subscriptions.status,
    })
    .from(creem_subscriptions)
    .where(
      sql`${creem_subscriptions.referenceId} IN (${sql.join(
        ownerIds.map((id) => sql`${id}`),
        sql`, `
      )}) AND ${creem_subscriptions.status} IN (${sql.join(
        ACTIVE_STATUSES.map((s) => sql`${s}`),
        sql`, `
      )})`
    )

  if (subs.length === 0) {
    return { plan: `free`, limits: PLAN_LIMITS.free }
  }

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

// User-scoped entitlement: the best plan a user is entitled to, independent of
// any single workspace. The owned-workspace cap is a per-user limit, so it must
// resolve from the user's own subscriptions, not a workspace's owner set.
export async function getUserPlan(
  userId: string
): Promise<{ plan: PlanTier; limits: PlanLimits }> {
  if (!isCloudInstance()) {
    return { plan: `unlimited`, limits: PLAN_LIMITS.unlimited }
  }

  const subs = await db
    .select({
      productId: creem_subscriptions.productId,
      status: creem_subscriptions.status,
    })
    .from(creem_subscriptions)
    .where(
      sql`${creem_subscriptions.referenceId} = ${userId} AND ${creem_subscriptions.status} IN (${sql.join(
        ACTIVE_STATUSES.map((s) => sql`${s}`),
        sql`, `
      )})`
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

// Number of non-public workspaces the user OWNS. The public feedback workspace
// is excluded (it's shared infra that admins "own" but shouldn't be billed for).
export async function countOwnedWorkspaces(userId: string): Promise<number> {
  const [row] = await db
    .select({ count: sql<number>`count(*)::int` })
    .from(workspaceMembers)
    .innerJoin(workspaces, eq(workspaces.id, workspaceMembers.workspaceId))
    .where(
      sql`${workspaceMembers.userId} = ${userId} AND ${workspaceMembers.role} = 'owner' AND ${workspaces.isPublic} = false`
    )
  return row?.count ?? 0
}

// Gate workspace creation by the per-plan owned-workspace cap. Never called on
// the auto-created first workspace (ensureDefault is exempt by design), so a
// user with zero owned workspaces always passes.
export async function assertCanCreateWorkspace(userId: string): Promise<void> {
  if (!isCloudInstance()) return

  const { limits } = await getUserPlan(userId)
  if (limits.ownedWorkspaces === Infinity) return

  const owned = await countOwnedWorkspaces(userId)
  if (owned >= limits.ownedWorkspaces) {
    throw planLimitError(
      `up to ${limits.ownedWorkspaces} workspace${
        limits.ownedWorkspaces === 1 ? `` : `s`
      } you can own. Upgrade to create more.`
    )
  }
}

export type WorkspaceUsage = {
  members: number
  projects: number
  storageMb: number
  repositories: number
}

export async function getWorkspaceUsage(
  workspaceId: string
): Promise<WorkspaceUsage> {
  const [[memberCount], [projectCount], [storageSum], [repoCount]] =
    await Promise.all([
      db
        .select({ count: sql<number>`count(*)::int` })
        .from(workspaceMembers)
        .where(eq(workspaceMembers.workspaceId, workspaceId)),
      db
        .select({ count: sql<number>`count(*)::int` })
        .from(projects)
        .where(eq(projects.workspaceId, workspaceId)),
      db
        .select({
          totalBytes: sql<string>`coalesce(sum(${attachments.sizeBytes}), 0)::bigint`,
        })
        .from(attachments)
        .where(eq(attachments.workspaceId, workspaceId)),
      db
        .select({ count: sql<number>`count(*)::int` })
        .from(repositories)
        .where(
          and(
            eq(repositories.workspaceId, workspaceId),
            isNull(repositories.archivedAt)
          )
        ),
    ])

  const totalBytes = Number(storageSum.totalBytes)

  return {
    members: memberCount.count,
    projects: projectCount.count,
    storageMb: Math.round((totalBytes / (1024 * 1024)) * 10) / 10,
    repositories: repoCount.count,
  }
}

export async function assertWithinPlanLimits(
  workspaceId: string,
  resource: `members` | `projects` | `repositories`
): Promise<void> {
  if (!isCloudInstance()) return

  const [{ limits }, usage] = await Promise.all([
    getWorkspacePlan(workspaceId),
    getWorkspaceUsage(workspaceId),
  ])

  const limit = limits[resource]
  const current = usage[resource]

  if (current >= limit) {
    const nouns: Record<typeof resource, [singular: string, plural: string]> = {
      members: [`member`, `members`],
      projects: [`project`, `projects`],
      repositories: [`connected repository`, `connected repositories`],
    }
    const noun = nouns[resource][limit === 1 ? 0 : 1]
    throw planLimitError(`up to ${limit} ${noun}. Upgrade to add more.`)
  }
}

// Concurrent coding-session capacity: how many `running` sessions a workspace
// may have at once. Checked at codingSessions.start (and fail-fast at remote
// steer.startSession); self-hosted is always unlimited.
export async function assertWithinCodingSessionLimit(
  workspaceId: string
): Promise<void> {
  if (!isCloudInstance()) return

  const { limits } = await getWorkspacePlan(workspaceId)
  if (limits.concurrentCodingSessions === Infinity) return

  const [row] = await db
    .select({ count: sql<number>`count(*)::int` })
    .from(codingSessions)
    .where(
      and(
        eq(codingSessions.workspaceId, workspaceId),
        eq(codingSessions.status, `running`)
      )
    )

  if ((row?.count ?? 0) >= limits.concurrentCodingSessions) {
    throw planLimitError(
      `up to ${limits.concurrentCodingSessions} concurrent coding session${
        limits.concurrentCodingSessions === 1 ? `` : `s`
      }. End a running session or upgrade to run more at once.`
    )
  }
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
