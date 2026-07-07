import { TRPCError } from "@trpc/server"
import { and, eq, inArray } from "drizzle-orm"
import { workspaces, workspaceMembers } from "@/db/schema"
import type { db as Database } from "@/db/connection"

// Works over the root db or a transaction — structurally typed so the helper
// can run inside the caller's delete transaction (nothing is removed when the
// stranded-owner guard throws).
type DbOrTx = Pick<typeof Database, `select` | `delete`>

export type MembershipRow = {
  workspaceId: string
  userId: string
  role: string
}

/**
 * Classify the workspaces a user belongs to for account deletion. Pure over
 * the membership rows so the orphan guard is unit-testable:
 *
 * - `stranded`: the user is the ONLY owner but OTHER members exist. Deleting
 *   the user would orphan the workspace — the membership cascade leaves the
 *   remaining members with no owner, so invites, billing, and settings all
 *   become unreachable, violating the last-owner invariant that
 *   workspaceMembers.remove/updateRole enforce. Fail closed on these.
 * - `solo`: the user is the entire membership (their personal workspace +
 *   solo workspaces). These are deleted along with the account so no orphaned
 *   data survives — the privacy policy promises deletion of "all associated
 *   data".
 */
export function classifyWorkspacesForUserDeletion(
  memberships: MembershipRow[],
  userId: string
): { stranded: string[]; solo: string[] } {
  const byWorkspace = new Map<string, MembershipRow[]>()
  for (const row of memberships) {
    const list = byWorkspace.get(row.workspaceId) ?? []
    list.push(row)
    byWorkspace.set(row.workspaceId, list)
  }

  const stranded: string[] = []
  const solo: string[] = []
  for (const [workspaceId, rows] of byWorkspace) {
    const mine = rows.filter((r) => r.userId === userId)
    if (mine.length === 0) continue
    const others = rows.filter((r) => r.userId !== userId)
    if (others.length === 0) {
      solo.push(workspaceId)
      continue
    }
    const isOwner = mine.some((r) => r.role === `owner`)
    const otherOwnerExists = others.some((r) => r.role === `owner`)
    if (isOwner && !otherOwnerExists) {
      stranded.push(workspaceId)
    }
  }
  return { stranded, solo }
}

/**
 * The shared workspace-safety step of deleting a user, used by BOTH
 * users.deleteAccount (self-service) and admin.deleteUser. Must run inside
 * the caller's transaction, BEFORE the users-row delete:
 *
 * 1. Fail closed when the user is the sole owner of a workspace that still
 *    has other members (see classifyWorkspacesForUserDeletion) — nothing is
 *    deleted when this throws.
 * 2. Delete workspaces whose entire membership is just this user (the
 *    is_public check keeps the bootstrap feedback board untouchable).
 *
 * Returns the ids of the workspaces actually deleted so the caller can route
 * their Creem subscriptions through cancellation.
 */
export async function guardAndCleanupWorkspacesForUserDeletion(
  tx: DbOrTx,
  userId: string,
  who: `self` | `admin`
): Promise<{ deletedWorkspaceIds: string[] }> {
  const myWorkspaceIds = tx
    .select({ id: workspaceMembers.workspaceId })
    .from(workspaceMembers)
    .where(eq(workspaceMembers.userId, userId))
  const memberships = await tx
    .select({
      workspaceId: workspaceMembers.workspaceId,
      userId: workspaceMembers.userId,
      role: workspaceMembers.role,
    })
    .from(workspaceMembers)
    .where(inArray(workspaceMembers.workspaceId, myWorkspaceIds))

  const { stranded, solo } = classifyWorkspacesForUserDeletion(
    memberships,
    userId
  )

  if (stranded.length > 0) {
    const rows = await tx
      .select({ name: workspaces.name })
      .from(workspaces)
      .where(inArray(workspaces.id, stranded))
    const names = rows.map((w) => `"${w.name}"`).join(`, `)
    const subject = who === `self` ? `You are` : `This user is`
    const tail =
      who === `self`
        ? `before deleting your account`
        : `before deleting this user`
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `${subject} the only owner of ${names}, which still ${rows.length === 1 ? `has` : `have`} other members — transfer ownership or remove those members ${tail}`,
    })
  }

  if (solo.length === 0) {
    return { deletedWorkspaceIds: [] }
  }
  const deleted = await tx
    .delete(workspaces)
    .where(and(inArray(workspaces.id, solo), eq(workspaces.isPublic, false)))
    .returning({ id: workspaces.id })
  return { deletedWorkspaceIds: deleted.map((d) => d.id) }
}
