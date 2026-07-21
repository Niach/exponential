import { TRPCError } from "@trpc/server"
import { eq, inArray, or } from "drizzle-orm"
import {
  attachments,
  githubInstallationRepoGrants,
  teams,
  teamMembers,
} from "@/db/schema"
import type { db as Database } from "@/db/connection"
import { getFeedbackTeamId } from "@/lib/bootstrap-cloud"
import {
  findActiveSubscriptionsForTeams,
  type CancellableSubscription,
} from "@/lib/billing/creem-subscriptions"

// Works over the root db or a transaction — structurally typed so the helper
// can run inside the caller's delete transaction (nothing is removed when the
// stranded-owner guard throws).
type DbOrTx = Pick<typeof Database, `select` | `delete`>

export type MembershipRow = {
  teamId: string
  userId: string
  role: string
}

/**
 * Classify the teams a user belongs to for account deletion. Pure over
 * the membership rows so the orphan guard is unit-testable:
 *
 * - `stranded`: the user is the ONLY owner but OTHER members exist. Deleting
 *   the user would orphan the team — the membership cascade leaves the
 *   remaining members with no owner, so invites, billing, and settings all
 *   become unreachable, violating the last-owner invariant that
 *   teamMembers.remove/updateRole enforce. Fail closed on these.
 * - `solo`: the user is the entire membership (their personal team +
 *   solo teams). These are deleted along with the account so no orphaned
 *   data survives — the privacy policy promises deletion of "all associated
 *   data".
 */
export function classifyTeamsForUserDeletion(
  memberships: MembershipRow[],
  userId: string
): { stranded: string[]; solo: string[] } {
  const byTeam = new Map<string, MembershipRow[]>()
  for (const row of memberships) {
    const list = byTeam.get(row.teamId) ?? []
    list.push(row)
    byTeam.set(row.teamId, list)
  }

  const stranded: string[] = []
  const solo: string[] = []
  for (const [teamId, rows] of byTeam) {
    const mine = rows.filter((r) => r.userId === userId)
    if (mine.length === 0) continue
    const others = rows.filter((r) => r.userId !== userId)
    if (others.length === 0) {
      solo.push(teamId)
      continue
    }
    const isOwner = mine.some((r) => r.role === `owner`)
    const otherOwnerExists = others.some((r) => r.role === `owner`)
    if (isOwner && !otherOwnerExists) {
      stranded.push(teamId)
    }
  }
  return { stranded, solo }
}

/**
 * The shared team-safety step of deleting a user, used by BOTH
 * users.deleteAccount (self-service) and admin.deleteUser. Must run inside
 * the caller's transaction, BEFORE the users-row delete:
 *
 * 1. Fail closed when the user is the sole owner of a team that still
 *    has other members (see classifyTeamsForUserDeletion) — nothing is
 *    deleted when this throws.
 * 2. Delete the GitHub repo grants this user proved (the FK would only null
 *    them out, leaving permanently unreachable rows that keep entitling the
 *    team to browse/connect the departed user's private repos).
 * 3. Delete teams whose entire membership is just this user (the
 *    getFeedbackTeamId() guard keeps the bootstrap feedback team
 *    untouchable).
 *
 * Returns the ids of the teams actually deleted PLUS the active Creem
 * subscriptions bound to them (captured in-tx BEFORE the delete — the
 * team FK goes `set null` on delete, after which the rows are
 * unfindable; a subscription bought by ANOTHER user for one of these
 * teams is invisible to the caller's buyer-scoped capture and would
 * keep charging), plus every S3 storage key the deletes (and the users-row
 * cascade the caller runs next) will strand — the caller cancels the
 * subscriptions and reclaims the keys from S3 after commit (best-effort).
 */
export async function guardAndCleanupTeamsForUserDeletion(
  tx: DbOrTx,
  userId: string,
  who: `self` | `admin`
): Promise<{
  deletedTeamIds: string[]
  doomedTeamSubscriptions: CancellableSubscription[]
  storageKeys: string[]
}> {
  const myTeamIds = tx
    .select({ id: teamMembers.teamId })
    .from(teamMembers)
    .where(eq(teamMembers.userId, userId))
  const memberships = await tx
    .select({
      teamId: teamMembers.teamId,
      userId: teamMembers.userId,
      role: teamMembers.role,
    })
    .from(teamMembers)
    .where(inArray(teamMembers.teamId, myTeamIds))

  const { stranded, solo } = classifyTeamsForUserDeletion(memberships, userId)

  if (stranded.length > 0) {
    const rows = await tx
      .select({ name: teams.name })
      .from(teams)
      .where(inArray(teams.id, stranded))
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

  // GitHub repo grants this user proved: a grant row means "team W may
  // see/connect this repo because user U proved user-scoped GitHub access",
  // so the entitlement must die with the user (assertRepoGrant matches on
  // team+installation+repo alone — an ownerless row would keep entitling
  // the team forever). The FK cascades on user delete as the schema-level
  // backstop; this explicit delete makes the revocation part of the guard
  // transaction itself rather than a side effect of whichever statement later
  // removes the users row.
  await tx
    .delete(githubInstallationRepoGrants)
    .where(eq(githubInstallationRepoGrants.grantedByUserId, userId))

  // Never cascade-delete the bootstrap feedback team (a sole-admin
  // account deletion would otherwise take every public feedback issue with it).
  const feedbackTeamId = await getFeedbackTeamId()
  const soloToDelete = feedbackTeamId
    ? solo.filter((id) => id !== feedbackTeamId)
    : solo

  // Collect every S3 object the deletes below and the users-row cascade will
  // strand — none of those DB deletes touch storage: (a) attachments in the
  // solo teams being deleted, (b) attachments this user uploaded
  // (attachments.uploader_id cascade). Deduped; collected BEFORE any delete.
  // NOTE: issues this user CREATED are NOT reclaimed — issues.creator_id is
  // ON DELETE SET NULL, so those issues (and their attachments) survive the
  // account deletion; reclaiming their blobs would strand live images.
  const keyConditions = [eq(attachments.uploaderId, userId)]
  if (soloToDelete.length > 0) {
    keyConditions.push(inArray(attachments.teamId, soloToDelete))
  }
  const keyRows = await tx
    .select({ storageKey: attachments.storageKey })
    .from(attachments)
    .where(or(...keyConditions))
  const storageKeys = [...new Set(keyRows.map((row) => row.storageKey))]

  if (soloToDelete.length === 0) {
    return {
      deletedTeamIds: [],
      doomedTeamSubscriptions: [],
      storageKeys,
    }
  }
  // Captured BEFORE the delete: the team FK on creem_subscriptions is
  // `set null`, so after the delete these rows can no longer be found by
  // team — and the buyer-scoped capture the callers run misses
  // subscriptions another user purchased for these teams.
  const doomedTeamSubscriptions = await findActiveSubscriptionsForTeams(
    soloToDelete,
    tx
  )
  const deleted = await tx
    .delete(teams)
    .where(inArray(teams.id, soloToDelete))
    .returning({ id: teams.id })
  return {
    deletedTeamIds: deleted.map((d) => d.id),
    doomedTeamSubscriptions,
    storageKeys,
  }
}
