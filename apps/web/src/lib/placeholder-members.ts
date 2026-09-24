import { randomUUID } from "crypto"
import { and, eq, isNotNull, isNull, sql } from "drizzle-orm"
import {
  attachments,
  comments,
  issueEvents,
  issues,
  issueSubscribers,
  notifications,
  teamInvites,
  teamMembers,
  users,
  workflows,
} from "@/db/schema"
import type { db as Database } from "@/db/connection"
import { replaceMentionTokens } from "@/lib/mention-refs"

// EXP-630 placeholder members.
//
// An email invite (Members settings, the Linear import's member mapping, MCP
// exponential_invites_create) puts the person on the roster AT ONCE: a `users`
// row with the invitee's name and address but no credentials, flagged by
// `users.placeholder_at`, plus the team_members row. Every client renders it
// like any member (assignee pickers, comment authors, avatars), so imported
// issues and comments are attributed to the right people before they have
// clicked anything.
//
// Claiming. The pending invite carries `placeholder_user_id`. The person
// either
//   (a) signs in THROUGH the placeholder's email — Google/Apple/OIDC link
//       onto the existing row (Better Auth account linking), a sign-in code
//       or a password reset land on it directly — and the row simply becomes
//       theirs (`claimPlaceholder`: the flag clears, the provider's name and
//       picture replace the invited ones, the invite is marked accepted); or
//   (b) accepts the invite link from a DIFFERENT account (a password signup,
//       another address) — `mergePlaceholderIntoUser` moves the placeholder's
//       team-scoped attributions and membership onto that account, then drops
//       the placeholder row when nothing references it any more.
//
// Works over the root db or a transaction (account-deletion.ts precedent).
type DbOrTx = Pick<
  typeof Database,
  `select` | `insert` | `delete` | `update` | `execute`
>

export interface PlaceholderIdentity {
  name: string
  email: string
}

/** The mailbox local part — the same default Better Auth's user hook uses
 * for a code sign-in that never asked for a name. */
export function placeholderNameFromEmail(email: string): string {
  const local = email.split(`@`)[0]?.trim() ?? ``
  return local || email
}

export function normalizeInviteEmail(email: string): string {
  return email.trim().toLowerCase()
}

/**
 * Resolve the identity an invite creates the placeholder with: the typed name
 * wins, an empty one falls back to the mailbox. Pure — locked by the tests.
 */
export function resolvePlaceholderIdentity(input: {
  email: string
  name?: string | null
}): PlaceholderIdentity {
  const email = normalizeInviteEmail(input.email)
  const typed = (input.name ?? ``).trim()
  return { email, name: typed || placeholderNameFromEmail(email) }
}

/**
 * Create the placeholder user + membership. The caller has already run the
 * seat gate and checked that no user owns the address.
 */
export async function createPlaceholderMember(
  tx: DbOrTx,
  input: {
    teamId: string
    role: `owner` | `member`
    identity: PlaceholderIdentity
    now?: Date
  }
): Promise<{ userId: string }> {
  const now = input.now ?? new Date()
  const userId = randomUUID()
  await tx.insert(users).values({
    id: userId,
    name: input.identity.name,
    email: input.identity.email,
    // Verified on purpose: Better Auth's implicit account linking
    // (oauth2/link-account, `requireLocalEmailVerified` defaults to true)
    // refuses to attach a Google/Apple/OIDC login to an UNVERIFIED local
    // row — claim path (a) would always end in `account_not_linked`. The
    // row has no credential, so the flag unlocks nothing by itself; the
    // invite email already went to this address.
    emailVerified: true,
    placeholderAt: now,
    createdAt: now,
    updatedAt: now,
  })
  await tx.insert(teamMembers).values({
    teamId: input.teamId,
    userId,
    role: input.role,
  })
  return { userId }
}

/**
 * Path (a): the placeholder row became a real account. Clears the flag and
 * marks every pending invite bound to it accepted, so member lists stop
 * badging the row even when the person never opened the invite page (they
 * signed in through the mailbox instead). The row already sits on a roster,
 * so the claim is onboarding evidence too (lib/auth/onboarding.ts): stamp
 * `onboardingCompletedAt` (where null) so the first-run create-or-join
 * wizard never shows. Idempotent.
 */
export async function claimPlaceholder(
  tx: DbOrTx,
  userId: string,
  now: Date = new Date()
): Promise<void> {
  const claimed = await tx
    .update(users)
    .set({
      placeholderAt: null,
      onboardingCompletedAt: sql`coalesce(${users.onboardingCompletedAt}, ${now})`,
      updatedAt: now,
    })
    .where(and(eq(users.id, userId), isNotNull(users.placeholderAt)))
    .returning({ id: users.id })
  if (claimed.length === 0) return
  await tx
    .update(teamInvites)
    .set({ acceptedAt: now })
    .where(
      and(
        eq(teamInvites.placeholderUserId, userId),
        isNull(teamInvites.acceptedAt)
      )
    )
}

/**
 * Path (b): the invite was accepted by another account. Everything the
 * placeholder holds IN THIS TEAM moves to the accepter — assignments,
 * authorship, activity, uploads, subscriptions, inbox rows, the membership
 * (unless the accepter is a member already, in which case the placeholder's
 * seat is simply freed) — and `@placeholder` mentions in the team's bodies
 * are rewritten to the accepter's address. Scoped to the team on purpose: the
 * invite token proves nothing about the address, so a placeholder that other
 * teams invited stays theirs. Runs under the preserve-timestamps guard so the
 * attribution rewrite does not restamp thousands of imported issues.
 *
 * Finally the placeholder row is dropped when nothing references it any more
 * (another team's attributions keep it alive — comments cascade with their
 * author, so a referenced row must stay).
 *
 * `merged: false` = the row is NOT (or no longer) an unclaimed placeholder —
 * the flag is re-read inside the transaction, so a claim that raced this
 * accept never gets a real account's attributions rewritten. The caller
 * falls through to an ordinary join then.
 */
export async function mergePlaceholderIntoUser(
  tx: DbOrTx,
  input: {
    placeholderId: string
    userId: string
    teamId: string
    userEmail: string
    // The invite's role — only used when the placeholder's own membership is
    // gone (removed meanwhile) and the accepter has none yet.
    role: `owner` | `member`
  }
): Promise<{ merged: boolean; deletedPlaceholder: boolean }> {
  const { placeholderId, userId, teamId } = input
  if (placeholderId === userId) return { merged: false, deletedPlaceholder: false }

  const [placeholder] = await tx
    .select({ email: users.email, placeholderAt: users.placeholderAt })
    .from(users)
    .where(eq(users.id, placeholderId))
    .limit(1)
  if (!placeholder?.placeholderAt) {
    return { merged: false, deletedPlaceholder: false }
  }

  await tx.execute(
    sql`SELECT set_config('exponential.preserve_timestamps', 'on', true)`
  )

  await tx
    .update(issues)
    .set({ assigneeId: userId })
    .where(and(eq(issues.teamId, teamId), eq(issues.assigneeId, placeholderId)))
  await tx
    .update(issues)
    .set({ creatorId: userId })
    .where(and(eq(issues.teamId, teamId), eq(issues.creatorId, placeholderId)))
  await tx
    .update(comments)
    .set({ authorId: userId })
    .where(and(eq(comments.teamId, teamId), eq(comments.authorId, placeholderId)))
  await tx
    .update(issueEvents)
    .set({ actorUserId: userId })
    .where(
      and(
        eq(issueEvents.teamId, teamId),
        eq(issueEvents.actorUserId, placeholderId)
      )
    )
  await tx
    .update(attachments)
    .set({ uploaderId: userId })
    .where(
      and(
        eq(attachments.teamId, teamId),
        eq(attachments.uploaderId, placeholderId)
      )
    )
  await tx
    .update(workflows)
    .set({ creatorId: userId })
    .where(
      and(eq(workflows.teamId, teamId), eq(workflows.creatorId, placeholderId))
    )

  // Subscriptions: (issue, user) is unique — drop the placeholder's row
  // wherever the accepter already subscribed, move the rest.
  await tx.execute(sql`
    DELETE FROM ${issueSubscribers} AS p
    WHERE p.user_id = ${placeholderId} AND p.team_id = ${teamId}
      AND EXISTS (
        SELECT 1 FROM ${issueSubscribers} AS a
        WHERE a.issue_id = p.issue_id AND a.user_id = ${userId}
      )
  `)
  await tx
    .update(issueSubscribers)
    .set({ userId })
    .where(
      and(
        eq(issueSubscribers.teamId, teamId),
        eq(issueSubscribers.userId, placeholderId)
      )
    )

  // Inbox rows the fan-out addressed to the placeholder for this team's
  // issues (board-anchored) or team-level rows.
  await tx.execute(sql`
    UPDATE ${notifications} AS n SET user_id = ${userId}
    WHERE n.user_id = ${placeholderId}
      AND (n.team_id = ${teamId}
        OR n.board_id IN (SELECT id FROM boards WHERE team_id = ${teamId}))
  `)

  // Membership: hand the placeholder's seat to the accepter, or free it.
  const [existing] = await tx
    .select({ id: teamMembers.id })
    .from(teamMembers)
    .where(and(eq(teamMembers.teamId, teamId), eq(teamMembers.userId, userId)))
    .limit(1)
  if (existing) {
    await tx
      .delete(teamMembers)
      .where(
        and(
          eq(teamMembers.teamId, teamId),
          eq(teamMembers.userId, placeholderId)
        )
      )
  } else {
    const moved = await tx
      .update(teamMembers)
      .set({ userId })
      .where(
        and(
          eq(teamMembers.teamId, teamId),
          eq(teamMembers.userId, placeholderId)
        )
      )
      .returning({ id: teamMembers.id })
    if (moved.length === 0) {
      await tx
        .insert(teamMembers)
        .values({ teamId, userId, role: input.role })
    }
  }

  if (normalizeInviteEmail(placeholder.email) !== normalizeInviteEmail(input.userEmail)) {
    await rewriteMentions(tx, {
      teamId,
      from: normalizeInviteEmail(placeholder.email),
      to: input.userEmail,
    })
  }

  const deletedPlaceholder = await deletePlaceholderIfOrphaned(tx, placeholderId)
  return { merged: true, deletedPlaceholder }
}

/**
 * Drop an unclaimed placeholder row once nothing points at it: no membership
 * left and no attribution anywhere (comments cascade with their author, so a
 * referenced row stays — it then reads like any former member). Never touches
 * a claimed account. Also called by teamMembers.remove.
 */
export async function deletePlaceholderIfOrphaned(
  tx: DbOrTx,
  userId: string
): Promise<boolean> {
  // Hand-qualified SQL: inside a select field drizzle renders bare column
  // names, and `"user_id" = "id"` would resolve `id` to the subquery's table.
  const result = await tx.execute(sql`
    SELECT u.placeholder_at IS NOT NULL
      AND NOT EXISTS (SELECT 1 FROM team_members tm WHERE tm.user_id = u.id)
      AND NOT EXISTS (SELECT 1 FROM issues i WHERE i.assignee_id = u.id OR i.creator_id = u.id)
      AND NOT EXISTS (SELECT 1 FROM comments c WHERE c.author_id = u.id)
      AND NOT EXISTS (SELECT 1 FROM issue_events e WHERE e.actor_user_id = u.id)
      AND NOT EXISTS (SELECT 1 FROM attachments a WHERE a.uploader_id = u.id)
      AND NOT EXISTS (SELECT 1 FROM workflows w WHERE w.creator_id = u.id)
      AS orphaned
    FROM users u WHERE u.id = ${userId}
  `)
  const row = (result as unknown as { rows?: { orphaned: boolean }[] } | undefined)?.rows?.[0]
  if (!row?.orphaned) return false
  await tx.delete(users).where(eq(users.id, userId))
  return true
}

/**
 * The name and picture an OAuth provider vouched for, read from the linked
 * account's id token (Google: `name`/`picture`; OIDC: `name` or the given +
 * family pair; Apple sends no name in its token). Pure — locked by the tests.
 * When the person signs in through a placeholder's email, THIS replaces the
 * invited name: the account is theirs, the Linear/typed name was a stand-in.
 */
export function providerProfileFromClaims(
  claims: unknown
): { name?: string; image?: string } {
  if (!claims || typeof claims !== `object`) return {}
  const record = claims as Record<string, unknown>
  const text = (value: unknown) =>
    typeof value === `string` && value.trim().length > 0 ? value.trim() : null
  const name =
    text(record.name) ??
    [text(record.given_name), text(record.family_name)]
      .filter((part): part is string => part !== null)
      .join(` `)
  const image = text(record.picture)
  return {
    ...(name ? { name } : {}),
    ...(image && /^https:\/\//.test(image) ? { image } : {}),
  }
}

/**
 * Called when an OAuth account lands on an UNCLAIMED placeholder (Better
 * Auth's account-create hook): adopt the provider's profile. A no-op for
 * real accounts — their chosen name is never overwritten by a later link.
 */
export async function adoptProviderProfile(
  tx: DbOrTx,
  userId: string,
  profile: { name?: string; image?: string },
  now: Date = new Date()
): Promise<void> {
  if (!profile.name && !profile.image) return
  await tx
    .update(users)
    .set({ ...profile, updatedAt: now })
    .where(and(eq(users.id, userId), isNotNull(users.placeholderAt)))
}

/** Escape a LIKE/ILIKE pattern operand (emails may legally contain `_`). */
function escapeLikeOperand(value: string): string {
  return value.replace(/[\\%_]/g, (char) => `\\${char}`)
}

// Mentions are the raw `@<email>` literal in bodies (lib/mention-refs.ts);
// the team's issues and comments that carry the placeholder's address get the
// accepter's instead, so the pills keep resolving.
async function rewriteMentions(
  tx: DbOrTx,
  input: { teamId: string; from: string; to: string }
): Promise<void> {
  const needle = `%@${escapeLikeOperand(input.from)}%`
  const swap = (text: string) =>
    replaceMentionTokens(text, (mentioned) =>
      mentioned === input.from ? `@${input.to}` : null
    )

  const issueRows = await tx
    .select({ id: issues.id, description: issues.description })
    .from(issues)
    .where(
      and(eq(issues.teamId, input.teamId), sql`${issues.description} ILIKE ${needle}`)
    )
  for (const row of issueRows) {
    if (!row.description) continue
    const next = swap(row.description)
    if (next === row.description) continue
    await tx.update(issues).set({ description: next }).where(eq(issues.id, row.id))
  }

  const commentRows = await tx
    .select({ id: comments.id, body: comments.body })
    .from(comments)
    .where(and(eq(comments.teamId, input.teamId), sql`${comments.body} ILIKE ${needle}`))
  for (const row of commentRows) {
    const next = swap(row.body)
    if (next === row.body) continue
    await tx.update(comments).set({ body: next }).where(eq(comments.id, row.id))
  }
}
