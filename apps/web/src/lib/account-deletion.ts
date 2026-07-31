import { TRPCError } from "@trpc/server"
import { and, eq, inArray, isNull, sql } from "drizzle-orm"
import {
  attachments,
  comments,
  emailBounces,
  githubInstallationRepoGrants,
  issues,
  teams,
  teamInvites,
  teamMembers,
  users,
  verifications,
} from "@/db/schema"
import type { db as Database } from "@/db/connection"
import {
  findActiveSubscriptionsForTeams,
  type CancellableSubscription,
} from "@/lib/billing/creem-subscriptions"
import {
  ANONYMIZED_MENTION_HANDLE,
  replaceMentionTokens,
} from "@/lib/mention-refs"

// Works over the root db or a transaction — structurally typed so the helper
// can run inside the caller's delete transaction (nothing is removed when the
// stranded-owner guard throws).
type DbOrTx = Pick<typeof Database, `select` | `delete` | `update`>

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
 * 3. Delete teams whose entire membership is just this user.
 *
 * 4. Anonymize `@<email>` mentions of the departing user in surviving issue
 *    descriptions and comment bodies (REV2-37), and delete the email residue
 *    that carries no user FK — unsuppressed bounce rows, Better Auth
 *    verification rows, and open invites addressed to them (REV2-75).
 *
 * Returns the ids of the teams actually deleted PLUS the active Creem
 * subscriptions bound to them (captured in-tx BEFORE the delete — the
 * team FK goes `set null` on delete, after which the rows are
 * unfindable). Those are the ONLY subscriptions an account deletion may
 * cancel: the teams they fund are being destroyed. Subscriptions this user
 * bought for teams that SURVIVE stay live and stay bound to their team
 * (REV2-55 + lib/billing/billing-handover.ts) — deleting an account never
 * cancels a plan someone else is still using, and never fails on billing.
 *
 * Also returns every S3 storage key the deletes will strand — the caller
 * cancels the subscriptions and reclaims the keys from S3 after commit
 * (best-effort).
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
      message: `${subject} the only owner of ${names}, which still ${rows.length === 1 ? `has` : `have`} other members. Transfer ownership or remove those members ${tail}.`,
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

  const soloToDelete = solo

  // The address is needed AFTER the users row is gone (mention scrub + email
  // residue), so capture it while it still exists.
  const [me] = await tx
    .select({ email: users.email })
    .from(users)
    .where(eq(users.id, userId))
    .limit(1)
  const email = me?.email ?? null

  // Collect the S3 objects the team deletes below will strand — that DB
  // cascade never touches storage. ONLY attachments in the solo teams being
  // deleted qualify (deduped; collected BEFORE any delete).
  // NOTE (REV2-36): attachments this user UPLOADED into a surviving team are
  // deliberately NOT reclaimed. `attachments.uploader_id` is ON DELETE SET
  // NULL, so the rows outlive the account — any member may embed an image
  // into a teammate's issue, and issues.creator_id is `set null` too, so
  // reclaiming an uploader's blobs would leave broken `![](/api/attachments/…)`
  // embeds scattered through content the deletion must not touch.
  const storageKeys =
    soloToDelete.length === 0
      ? []
      : [
          ...new Set(
            (
              await tx
                .select({ storageKey: attachments.storageKey })
                .from(attachments)
                .where(inArray(attachments.teamId, soloToDelete))
            ).map((row) => row.storageKey)
          ),
        ]

  let deletedTeamIds: string[] = []
  let doomedTeamSubscriptions: CancellableSubscription[] = []
  if (soloToDelete.length > 0) {
    // Captured BEFORE the delete: the team FK on creem_subscriptions is
    // `set null`, so after the delete these rows can no longer be found by
    // team. These are the only subscriptions an account deletion cancels —
    // the teams paying for them cease to exist (see the module docs).
    doomedTeamSubscriptions = await findActiveSubscriptionsForTeams(
      soloToDelete,
      tx
    )
    const deleted = await tx
      .delete(teams)
      .where(inArray(teams.id, soloToDelete))
      .returning({ id: teams.id })
    deletedTeamIds = deleted.map((d) => d.id)
  }

  if (email) {
    // Run AFTER the solo-team deletes so the scan only walks surviving rows.
    await anonymizeMentionsOfDeletedUser(tx, email)
    await deleteEmailResidueForUser(tx, userId, email)
  }

  return { deletedTeamIds, doomedTeamSubscriptions, storageKeys }
}

/** Escape a LIKE/ILIKE pattern operand (emails may legally contain `_`). */
function escapeLikeOperand(value: string): string {
  return value.replace(/[\\%_]/g, (char) => `\\${char}`)
}

/**
 * REV2-37: mentions are stored as the raw `@<email>` literal in
 * `issues.description` / `comments.body`, and nothing used to rewrite them —
 * so a deleted user's address stayed in former co-members' teams forever, and
 * displayed WORSE than before (clients render a name pill only while the
 * email resolves to a live member; once the users row is gone the raw address
 * shows). Rewrite every remaining occurrence to the neutral
 * ANONYMIZED_MENTION_HANDLE.
 *
 * Deliberately NOT team-scoped: the person may have been mentioned in a team
 * they later left, and erasure means erasure. The ILIKE prefilter keeps the
 * write set to the handful of rows that actually contain the address.
 */
async function anonymizeMentionsOfDeletedUser(
  tx: DbOrTx,
  email: string
): Promise<void> {
  const needle = `%@${escapeLikeOperand(email)}%`
  const target = email.toLowerCase()
  const anonymize = (text: string) =>
    replaceMentionTokens(text, (mentioned) =>
      mentioned === target ? ANONYMIZED_MENTION_HANDLE : null
    )

  const issueRows = await tx
    .select({ id: issues.id, description: issues.description })
    .from(issues)
    .where(sql`${issues.description} ILIKE ${needle}`)
  for (const row of issueRows) {
    if (!row.description) continue
    const next = anonymize(row.description)
    if (next === row.description) continue
    await tx
      .update(issues)
      .set({ description: next, updatedAt: new Date() })
      .where(eq(issues.id, row.id))
  }

  const commentRows = await tx
    .select({ id: comments.id, body: comments.body })
    .from(comments)
    .where(sql`${comments.body} ILIKE ${needle}`)
  for (const row of commentRows) {
    const next = anonymize(row.body)
    if (next === row.body) continue
    await tx
      .update(comments)
      .set({ body: next, updatedAt: new Date() })
      .where(eq(comments.id, row.id))
  }
}

/**
 * REV2-75: the three places that keep the address with no user FK to cascade
 * it away.
 *
 * - `email_bounces`: unsuppressed rows are pure PII residue. SUPPRESSED rows
 *   are kept on purpose — a suppression list is a legitimate-interest record
 *   that must outlive the account, or the next signup on the same address
 *   would immediately re-bounce mail at SES and damage the sending domain.
 * - `verifications`: the raw users-row delete bypasses Better Auth's own
 *   cleanup. Matched by identifier (the email-keyed OTP forms) OR by value
 *   (`reset-password:<token>` / `delete-account-<token>` rows store the user
 *   id), which covers every flow this instance can enable.
 * - `team_invites`: invites addressed TO this person. Only `invited_by_id`
 *   cascades, and the `email` column IS in the team-invites shape allowlist,
 *   so an un-accepted invite would keep syncing the address to every member
 *   of that team forever. Accepted invites are historical records of a real
 *   join, and their address is already redundant — they stay.
 */
async function deleteEmailResidueForUser(
  tx: DbOrTx,
  userId: string,
  email: string
): Promise<void> {
  await tx
    .delete(emailBounces)
    .where(
      and(
        sql`lower(${emailBounces.email}) = ${email.toLowerCase()}`,
        isNull(emailBounces.suppressedAt)
      )
    )

  await tx
    .delete(verifications)
    .where(
      sql`lower(${verifications.identifier}) = ${email.toLowerCase()} OR ${verifications.value} = ${userId}`
    )

  await tx
    .delete(teamInvites)
    .where(
      and(
        sql`lower(${teamInvites.email}) = ${email.toLowerCase()}`,
        isNull(teamInvites.acceptedAt)
      )
    )
}
