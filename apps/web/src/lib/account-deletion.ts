import { TRPCError } from "@trpc/server"
import { eq, inArray, or } from "drizzle-orm"
import {
  attachments,
  issues,
  users,
  workspaces,
  workspaceMembers,
} from "@/db/schema"
import type { db as Database } from "@/db/connection"
import { getFeedbackWorkspaceId } from "@/lib/bootstrap-cloud"
import {
  findActiveSubscriptionsForWorkspaces,
  type CancellableSubscription,
} from "@/lib/billing/creem-subscriptions"

// Works over the root db or a transaction — structurally typed so the helper
// can run inside the caller's delete transaction (nothing is removed when the
// stranded-owner guard throws).
type DbOrTx = Pick<typeof Database, `select` | `delete`>

export type MembershipRow = {
  workspaceId: string
  userId: string
  role: string
  isAgent: boolean
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
 * - Agent members (users.isAgent — the synthetic widget bot) are ignored: a
 *   workspace whose only non-agent member is the user classifies `solo` and
 *   is deleted with the account.
 */
export function classifyWorkspacesForUserDeletion(
  memberships: MembershipRow[],
  userId: string
): { stranded: string[]; solo: string[] } {
  // Synthetic agent users (the widget bot) never count as "other members":
  // they hold no owner powers, every member-list surface hides them, and
  // billing/seat counts already exclude them (lib/billing.ts). Counting one
  // here would permanently classify a widget-owning personal workspace as
  // stranded — blocking self-service account deletion with an error that
  // points at an invisible member. The user being deleted always counts as
  // themselves, whatever their own flag (admin-deleting a retained bot must
  // still solo-classify a workspace where the bot is the entire membership).
  const humanRows = memberships.filter(
    (r) => r.userId === userId || !r.isAgent
  )
  const byWorkspace = new Map<string, MembershipRow[]>()
  for (const row of humanRows) {
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
 * Returns the ids of the workspaces actually deleted PLUS the active Creem
 * subscriptions bound to them (captured in-tx BEFORE the delete — the
 * workspace FK goes `set null` on delete, after which the rows are
 * unfindable; a subscription bought by ANOTHER user for one of these
 * workspaces is invisible to the caller's buyer-scoped capture and would
 * keep charging), plus every S3 storage key the deletes (and the users-row
 * cascade the caller runs next) will strand — the caller cancels the
 * subscriptions and reclaims the keys from S3 after commit (best-effort).
 */
export async function guardAndCleanupWorkspacesForUserDeletion(
  tx: DbOrTx,
  userId: string,
  who: `self` | `admin`
): Promise<{
  deletedWorkspaceIds: string[]
  doomedWorkspaceSubscriptions: CancellableSubscription[]
  storageKeys: string[]
}> {
  const myWorkspaceIds = tx
    .select({ id: workspaceMembers.workspaceId })
    .from(workspaceMembers)
    .where(eq(workspaceMembers.userId, userId))
  const memberships = await tx
    .select({
      workspaceId: workspaceMembers.workspaceId,
      userId: workspaceMembers.userId,
      role: workspaceMembers.role,
      isAgent: users.isAgent,
    })
    .from(workspaceMembers)
    .innerJoin(users, eq(users.id, workspaceMembers.userId))
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

  // Never cascade-delete the bootstrap feedback workspace (a sole-admin
  // account deletion would otherwise take every public feedback issue with it).
  const feedbackWorkspaceId = await getFeedbackWorkspaceId()
  const soloToDelete = feedbackWorkspaceId
    ? solo.filter((id) => id !== feedbackWorkspaceId)
    : solo

  // Collect every S3 object the deletes below and the users-row cascade will
  // strand — none of those DB deletes touch storage: (a) attachments in the
  // solo workspaces being deleted, (b) attachments of issues this user created
  // (issues.creator_id cascade), (c) attachments this user uploaded
  // (attachments.uploader_id cascade). Deduped; collected BEFORE any delete.
  const myIssueIds = tx
    .select({ id: issues.id })
    .from(issues)
    .where(eq(issues.creatorId, userId))
  const keyConditions = [
    inArray(attachments.issueId, myIssueIds),
    eq(attachments.uploaderId, userId),
  ]
  if (soloToDelete.length > 0) {
    keyConditions.push(inArray(attachments.workspaceId, soloToDelete))
  }
  const keyRows = await tx
    .select({ storageKey: attachments.storageKey })
    .from(attachments)
    .where(or(...keyConditions))
  const storageKeys = [...new Set(keyRows.map((row) => row.storageKey))]

  if (soloToDelete.length === 0) {
    return {
      deletedWorkspaceIds: [],
      doomedWorkspaceSubscriptions: [],
      storageKeys,
    }
  }
  // Captured BEFORE the delete: the workspace FK on creem_subscriptions is
  // `set null`, so after the delete these rows can no longer be found by
  // workspace — and the buyer-scoped capture the callers run misses
  // subscriptions another user purchased for these workspaces.
  const doomedWorkspaceSubscriptions =
    await findActiveSubscriptionsForWorkspaces(soloToDelete, tx)
  const deleted = await tx
    .delete(workspaces)
    .where(inArray(workspaces.id, soloToDelete))
    .returning({ id: workspaces.id })
  return {
    deletedWorkspaceIds: deleted.map((d) => d.id),
    doomedWorkspaceSubscriptions,
    storageKeys,
  }
}
