import { TRPCError } from "@trpc/server"
import {
  and,
  eq,
  inArray,
  isNull,
  ne,
  or,
  sql,
} from "drizzle-orm"
import {
  attachments,
  comments,
  conversations,
  messages,
  sessionIssues,
  sessions,
  emailBounces,
  issueDrafts,
  issues,
  teams,
  teamInvites,
  teamMembers,
  users,
  verifications,
} from "@/db/schema"
import type { db as Database } from "@/db/connection"
import { collectTeamStorageKeys } from "@/lib/storage/team-storage-keys"
import { collectAttachmentStorageKeys } from "@/lib/storage/issue-attachments"
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
 * Also returns every S3 storage key the deletes will strand, plus the ids of
 * the cross-account coding sessions step 5 ended (EXP-445) — the caller
 * cancels the subscriptions, reclaims the keys from S3 and fans out the relay
 * kills after commit (all best-effort).
 *
 * 5. End the live coding sessions this account is one half of: the runs it
 *    REQUESTED on a teammate's shared server device (reassigned to the host so
 *    they survive the cascade) and the runs it HOSTS for teammates.
 */
export async function guardAndCleanupTeamsForUserDeletion(
  tx: DbOrTx,
  userId: string,
  who: `self` | `admin`
): Promise<{
  deletedTeamIds: string[]
  doomedTeamSubscriptions: CancellableSubscription[]
  storageKeys: string[]
  endedSessionIds: string[]
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

  // EXP-445/1291: sessions that span TWO accounts — the requester who
  // started them and the device owner hosting them on a shared device
  // (EXP-432). Both directions are settled here, inside the delete
  // transaction, so the users-row cascade can never race them.
  const endedSessionIds: string[] = []
  const now = new Date()

  // Sessions this user STARTED on somebody else's machine (NEW-1: the host
  // is derived from the devices rows: no own row for the device, a
  // teammate's row sharing the team). `user_id` cascades, so the delete
  // would VAPORIZE these rows (a shared thread is team data, and a vanished
  // row is no kill signal for the host's device). Re-attributing them to the
  // host keeps them as host-owned rows, ended when live; the visibility
  // mirrors (`owner_id`) follow, or the messages and issue links would
  // cascade away with the account.
  // The host = the lowest-id owner of a devices row for the session's
  // machine that shares the session's team (`lib/sessions/host.ts`).
  const hostOfSession = sql`(SELECT d.user_id FROM devices d WHERE d.device_id = ${sessions.deviceId} AND d.user_id <> ${userId} AND ${sessions.teamId} = ANY(d.shared_team_ids) ORDER BY d.user_id LIMIT 1)`
  const requested = await tx
    .update(sessions)
    .set({ userId: hostOfSession, updatedAt: now })
    .where(
      and(
        eq(sessions.userId, userId),
        sql`NOT EXISTS (SELECT 1 FROM devices d WHERE d.device_id = ${sessions.deviceId} AND d.user_id = ${userId})`,
        sql`EXISTS ${hostOfSession}`
      )
    )
    .returning({ id: sessions.id })
  if (requested.length > 0) {
    const ids = requested.map((row) => row.id)
    await tx
      .update(messages)
      .set({ ownerId: sql`(SELECT s.user_id FROM sessions s WHERE s.id = ${messages.sessionId})` })
      .where(inArray(messages.sessionId, ids))
    await tx
      .update(sessionIssues)
      .set({ ownerId: sql`(SELECT s.user_id FROM sessions s WHERE s.id = ${sessionIssues.sessionId})` })
      .where(inArray(sessionIssues.sessionId, ids))
    // The conversation row too: it cascades with its owner and takes every
    // message with it.
    await tx
      .update(conversations)
      .set({ ownerId: sql`(SELECT s.user_id FROM sessions s WHERE s.id = ${conversations.sessionId})` })
      .where(inArray(conversations.sessionId, ids))
    const ended = await tx
      .update(sessions)
      .set({ status: `ended`, endedAt: now, busy: false, needsInput: false })
      .where(and(inArray(sessions.id, ids), ne(sessions.status, `ended`), isNull(sessions.agentId)))
      .returning({ id: sessions.id })
    endedSessionIds.push(...ended.map((row) => row.id))
  }

  // Live sessions this user HOSTS for a teammate (their device, the
  // starter owns no row for it): the device executing them authenticates as
  // the account being deleted, so they are over.
  const hosted = await tx
    .update(sessions)
    .set({ status: `ended`, endedAt: now, busy: false, needsInput: false, updatedAt: now })
    .where(
      and(
        ne(sessions.userId, userId),
        ne(sessions.status, `ended`),
        isNull(sessions.agentId),
        sql`EXISTS (SELECT 1 FROM devices d WHERE d.user_id = ${userId} AND d.device_id = ${sessions.deviceId})`,
        sql`NOT EXISTS (SELECT 1 FROM devices d WHERE d.user_id = ${sessions.userId} AND d.device_id = ${sessions.deviceId})`
      )
    )
    .returning({ id: sessions.id })
  endedSessionIds.push(...hosted.map((row) => row.id))

  const soloToDelete = solo

  // The address is needed AFTER the users row is gone (mention scrub + email
  // residue), so capture it while it still exists.
  const [me] = await tx
    .select({ email: users.email })
    .from(users)
    .where(eq(users.id, userId))
    .limit(1)
  const email = me?.email ?? null

  // Collect the S3 objects the deletes below will strand — no DB cascade ever
  // touches storage (deduped; collected BEFORE any delete). Two sources:
  //
  // 1. Every attachment in the solo teams being deleted.
  // 2. EXP-878: the attachments of this user's ISSUE DRAFTS in EVERY team.
  //    `issue_drafts.user_id` cascades on the users-row delete and
  //    `attachments.draft_id` cascades with the draft, so those rows vanish
  //    in surviving teams too; without this arm their objects would sit in
  //    the bucket forever and drop out of the team's usage sum. A draft is
  //    private to its author, so nothing else can embed its files.
  //
  // NOTE (REV2-36): attachments this user UPLOADED into a surviving ISSUE
  // are deliberately NOT reclaimed. `attachments.uploader_id` is ON DELETE
  // SET NULL, so the rows outlive the account — any member may embed an image
  // into a teammate's issue, and issues.creator_id is `set null` too, so
  // reclaiming an uploader's blobs would leave broken `![](/api/attachments/…)`
  // embeds scattered through content the deletion must not touch.
  const teamStorageKeys = await collectTeamStorageKeys(tx, soloToDelete)
  const myDraftIds = tx
    .select({ id: issueDrafts.id })
    .from(issueDrafts)
    .where(eq(issueDrafts.userId, userId))
  // EXP-1291: this user's own sessions cascade away with their messages,
  // and a message's files cascade with it — collected in the same pass.
  const myMessageIds = tx
    .select({ id: messages.id })
    .from(messages)
    .where(eq(messages.ownerId, userId))
  const ownedAttachmentRows = await tx
    .select({
      storageKey: attachments.storageKey,
      posterStorageKey: attachments.posterStorageKey,
    })
    .from(attachments)
    .where(
      or(
        inArray(attachments.draftId, myDraftIds),
        inArray(attachments.messageId, myMessageIds)
      )
    )
  const storageKeys = collectAttachmentStorageKeys([
    ...teamStorageKeys.map((storageKey) => ({ storageKey })),
    ...ownedAttachmentRows,
  ])

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

  return {
    deletedTeamIds,
    doomedTeamSubscriptions,
    storageKeys,
    endedSessionIds,
  }
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
