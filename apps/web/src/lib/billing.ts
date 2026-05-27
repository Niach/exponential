import { TRPCError } from "@trpc/server"
import { eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  workspaceMembers,
  projects,
  attachments,
  creem_subscriptions,
} from "@/db/schema"
import { isCloudInstance } from "@/lib/bootstrap-cloud"

export type PlanTier = `free` | `pro` | `business` | `unlimited`

type PlanLimits = {
  members: number
  projects: number
  storageMb: number
  push: boolean
}

const PLAN_LIMITS: Record<PlanTier, PlanLimits> = {
  free: { members: 1, projects: 3, storageMb: 50, push: false },
  pro: { members: 5, projects: 10, storageMb: 1024, push: true },
  business: { members: 25, projects: Infinity, storageMb: 10240, push: true },
  unlimited: {
    members: Infinity,
    projects: Infinity,
    storageMb: Infinity,
    push: true,
  },
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

export type WorkspaceUsage = {
  members: number
  projects: number
  storageMb: number
}

export async function getWorkspaceUsage(
  workspaceId: string
): Promise<WorkspaceUsage> {
  const [[memberCount], [projectCount], [storageSum]] = await Promise.all([
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
  ])

  const totalBytes = Number(storageSum.totalBytes)

  return {
    members: memberCount.count,
    projects: projectCount.count,
    storageMb: Math.round((totalBytes / (1024 * 1024)) * 10) / 10,
  }
}

export async function assertWithinPlanLimits(
  workspaceId: string,
  resource: `members` | `projects`
): Promise<void> {
  if (!isCloudInstance()) return

  const [{ limits }, usage] = await Promise.all([
    getWorkspacePlan(workspaceId),
    getWorkspaceUsage(workspaceId),
  ])

  const limit = limits[resource]
  const current = usage[resource]

  if (current >= limit) {
    throw new TRPCError({
      code: `FORBIDDEN`,
      message: `Your plan allows up to ${limit} ${resource}. Upgrade to add more.`,
    })
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
    throw new TRPCError({
      code: `FORBIDDEN`,
      message: `Your plan allows up to ${limits.storageMb >= 1024 ? `${Math.round(limits.storageMb / 1024)} GB` : `${limits.storageMb} MB`} of storage. Upgrade to upload more.`,
    })
  }
}

export async function canUsePush(workspaceId: string): Promise<boolean> {
  if (!isCloudInstance()) return true
  const { limits } = await getWorkspacePlan(workspaceId)
  return limits.push
}
