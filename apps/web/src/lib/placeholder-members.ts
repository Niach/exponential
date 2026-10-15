import { randomBytes, randomUUID } from "crypto"
import { and, eq, inArray, isNotNull, isNull, sql } from "drizzle-orm"
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
 * EXP-1076: the roster row WITHOUT an invitation. The Linear import seats
 * everyone it found so issues and comments are attributed from day one, but
 * nobody asked those people to join — so its placeholder port writes an
 * invite row whose link was never issued: `sent_at` NULL (Members renders
 * "Not invited" with a "Send invite" action) and `expires_at = now`, so every
 * expiry-based reader (accept, the seat counts in lib/billing.ts, pending
 * lists on all four clients) already treats the token as dead. The token is
 * still random and unique — the row is a placeholder binding, not a
 * credential anybody holds.
 *
 * Called by `addPlaceholderToRoster` below (the import's placeholder port and
 * FEED-108's by-hand "Add without inviting"); `email` is taken as input
 * rather than looked up so the caller's existing `users` read is not
 * repeated. `teamInvites.create` mints the real link later (stamping
 * `sentAt`, seat-gated as a first send).
 */
export async function createUnsentPlaceholderInvite(
  tx: DbOrTx,
  input: {
    teamId: string
    placeholderUserId: string
    invitedById: string
    email: string
    role: `owner` | `member`
    now?: Date
  }
): Promise<{ inviteId: string }> {
  const now = input.now ?? new Date()
  const inviteId = randomUUID()
  await tx.insert(teamInvites).values({
    id: inviteId,
    teamId: input.teamId,
    invitedById: input.invitedById,
    role: input.role,
    token: randomBytes(32).toString(`hex`),
    email: normalizeInviteEmail(input.email),
    placeholderUserId: input.placeholderUserId,
    sentAt: null,
    expiresAt: now,
    createdAt: now,
    updatedAt: now,
  })
  return { inviteId }
}

/**
 * EXP-1141: does this placeholder hold a team_members row on ANY roster? A
 * seated one belongs to that team (an invite elsewhere stays unbound, a
 * by-hand add elsewhere is refused); a seatless one is nobody's.
 */
export async function isSeatedAnywhere(
  tx: Pick<DbOrTx, `select`>,
  userId: string
): Promise<boolean> {
  const [row] = await tx
    .select({ id: teamMembers.id })
    .from(teamMembers)
    .where(eq(teamMembers.userId, userId))
    .limit(1)
  return Boolean(row)
}

/**
 * Is this seatless placeholder claimed, or referenced by ANOTHER team? A
 * provider login already linked to it (the claim hook has not flipped the
 * flag yet), an attribution in another team's issues, comments, events or
 * uploads, or a pending invite from another team all mean the identity is
 * not this team's to seat: once seated here, the re-invite path could
 * re-address it — and with it everything the other team's rows point at.
 * Hand-qualified SQL like `deletePlaceholderIfOrphaned`.
 */
export async function placeholderReferencedOutside(
  tx: Pick<DbOrTx, `execute`>,
  userId: string,
  teamId: string
): Promise<boolean> {
  const result = await tx.execute(sql`
    SELECT EXISTS (SELECT 1 FROM accounts acc WHERE acc.user_id = ${userId})
      OR EXISTS (SELECT 1 FROM issues i WHERE (i.assignee_id = ${userId} OR i.creator_id = ${userId}) AND i.team_id IS DISTINCT FROM ${teamId}::uuid)
      OR EXISTS (SELECT 1 FROM comments c WHERE c.author_id = ${userId} AND c.team_id IS DISTINCT FROM ${teamId}::uuid)
      OR EXISTS (SELECT 1 FROM issue_events e WHERE e.actor_user_id = ${userId} AND e.team_id IS DISTINCT FROM ${teamId}::uuid)
      OR EXISTS (SELECT 1 FROM attachments a WHERE a.uploader_id = ${userId} AND a.team_id IS DISTINCT FROM ${teamId}::uuid)
      OR EXISTS (SELECT 1 FROM team_invites ti WHERE ti.placeholder_user_id = ${userId} AND ti.team_id <> ${teamId}::uuid AND ti.accepted_at IS NULL)
      AS referenced_outside
  `)
  const row = (
    result as unknown as { rows?: { referenced_outside: boolean }[] } | undefined
  )?.rows?.[0]
  return Boolean(row?.referenced_outside)
}

export type AddPlaceholderResult =
  /** A fresh users + team_members row and the unsent invite binding. */
  | { kind: `created`; userId: string }
  /** A seatless placeholder referenced only by this team, re-seated here. */
  | { kind: `rebound`; userId: string }
  /** The address is already on THIS roster (a real account or a placeholder). */
  | { kind: `member`; userId: string }
  /** A real account, or a placeholder seated/referenced/invited elsewhere. */
  | { kind: `outside` }

/**
 * THE roster-without-an-invitation path (EXP-1076 for the import, FEED-108
 * by hand): put a person on the team as a placeholder member, mailing nothing
 * and spending no seat. The Linear import's port and `teamMembers.addPlaceholder`
 * both run this, so an address resolves the same way wherever the add starts.
 * Always role `member` — a row nobody can sign in as must never be a team's
 * only owner; promote later with `teamMembers.updateRole`.
 *
 * Order: the per-address advisory lock first (two teams, or the import and
 * an owner, adding one address at once serialize on it); then the address:
 * unknown → a fresh placeholder; on this roster → `member`; a real account or
 * a placeholder SEATED elsewhere → `outside` (a typed address proves nothing,
 * the person joins by invite); a seatless placeholder → dropped and re-created
 * when nothing references it (EXP-1132), re-seated here when only this team
 * does (EXP-1141: a removed member's comments stay attributable), refused when
 * another team does (`placeholderReferencedOutside`).
 *
 * No `placeholderClaimable()` gate: a never-invited row is a roster entry; the
 * later "Send invite" still binds a link, and accepting from a password
 * account merges the row. Runs inside the caller's transaction.
 */
export async function addPlaceholderToRoster(
  tx: DbOrTx,
  input: {
    teamId: string
    email: string
    name?: string | null
    invitedById: string
    now?: Date
  }
): Promise<AddPlaceholderResult> {
  const now = input.now ?? new Date()
  const identity = resolvePlaceholderIdentity({
    email: input.email,
    name: input.name,
  })
  await tx.execute(
    sql`SELECT pg_advisory_xact_lock(hashtext(${`placeholder_address:${identity.email}`}))`
  )
  const [owner] = await tx
    .select({ id: users.id, placeholderAt: users.placeholderAt })
    .from(users)
    .where(sql`lower(${users.email}) = ${identity.email}`)
    .limit(1)
  if (owner) {
    const [member] = await tx
      .select({ id: teamMembers.id })
      .from(teamMembers)
      .where(
        and(eq(teamMembers.teamId, input.teamId), eq(teamMembers.userId, owner.id))
      )
      .limit(1)
    if (member) return { kind: `member`, userId: owner.id }
    if (!owner.placeholderAt || (await isSeatedAnywhere(tx, owner.id))) {
      return { kind: `outside` }
    }
    if (!(await deletePlaceholderIfOrphaned(tx, owner.id))) {
      if (await placeholderReferencedOutside(tx, owner.id, input.teamId)) {
        return { kind: `outside` }
      }
      await tx.insert(teamMembers).values({
        teamId: input.teamId,
        userId: owner.id,
        role: `member`,
      })
      // A link the owner already sent (EXP-1141, bound but unseated) stays
      // live; only a row with no binding here gets the unsent one.
      const [bound] = await tx
        .select({ id: teamInvites.id })
        .from(teamInvites)
        .where(
          and(
            eq(teamInvites.teamId, input.teamId),
            eq(teamInvites.placeholderUserId, owner.id),
            isNull(teamInvites.acceptedAt)
          )
        )
        .limit(1)
      if (!bound) {
        await createUnsentPlaceholderInvite(tx, {
          teamId: input.teamId,
          placeholderUserId: owner.id,
          invitedById: input.invitedById,
          email: identity.email,
          role: `member`,
          now,
        })
      }
      return { kind: `rebound`, userId: owner.id }
    }
  }
  const { userId } = await createPlaceholderMember(tx, {
    teamId: input.teamId,
    role: `member`,
    identity,
    now,
  })
  await createUnsentPlaceholderInvite(tx, {
    teamId: input.teamId,
    placeholderUserId: userId,
    invitedById: input.invitedById,
    email: identity.email,
    role: `member`,
    now,
  })
  return { kind: `created`, userId }
}

/**
 * Path (a): the placeholder row became a real account. Clears the flag and
 * marks every pending invite bound to it accepted, so member lists stop
 * badging the row even when the person never opened the invite page (they
 * signed in through the mailbox instead). The row already sits on a roster,
 * so the claim is onboarding evidence too (lib/auth/onboarding.ts): stamp
 * `onboardingCompletedAt` (where null) so the first-run create-or-join
 * wizard never shows. Idempotent.
 *
 * Accepted edge (EXP-1076): a person who signs in through the placeholder's
 * mailbox WITHOUT ever having been invited (the import seats them, `sent_at`
 * NULL) lands here too and becomes a seated member with no seat gate — seat
 * gates block new INVITES, never members already on the roster.
 *
 * EXP-1141: only invites of teams that SEAT the row are stamped. An invite
 * bound to a seatless placeholder (a removed member with comments, re-invited)
 * is a join still to happen — stamping it here would consume the link before
 * the person reaches the invite page, which seats them (teamInvites.accept,
 * the placeholder-is-the-accepter path) under the ordinary seat gate.
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
        isNull(teamInvites.acceptedAt),
        sql`exists (select 1 from ${teamMembers} where ${teamMembers.teamId} = ${teamInvites.teamId} and ${teamMembers.userId} = ${userId})`
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
 * a claimed account: neither one whose `placeholder_at` was cleared nor one a
 * provider login already linked an `accounts` row to (someone signed in
 * through the placeholder's mailbox before the claim hook flipped the flag).
 * Also called by teamMembers.remove. Mirrored by the 0150 purge migration.
 */
export async function deletePlaceholderIfOrphaned(
  tx: DbOrTx,
  userId: string
): Promise<boolean> {
  // Hand-qualified SQL: inside a select field drizzle renders bare column
  // names, and `"user_id" = "id"` would resolve `id` to the subquery's table.
  const result = await tx.execute(sql`
    SELECT u.placeholder_at IS NOT NULL
      AND NOT EXISTS (SELECT 1 FROM accounts acc WHERE acc.user_id = u.id)
      AND NOT EXISTS (SELECT 1 FROM team_members tm WHERE tm.user_id = u.id)
      AND NOT EXISTS (SELECT 1 FROM issues i WHERE i.assignee_id = u.id OR i.creator_id = u.id)
      AND NOT EXISTS (SELECT 1 FROM comments c WHERE c.author_id = u.id)
      AND NOT EXISTS (SELECT 1 FROM issue_events e WHERE e.actor_user_id = u.id)
      AND NOT EXISTS (SELECT 1 FROM attachments a WHERE a.uploader_id = u.id)
      AS orphaned
    FROM users u WHERE u.id = ${userId}
  `)
  const row = (result as unknown as { rows?: { orphaned: boolean }[] } | undefined)?.rows?.[0]
  if (!row?.orphaned) return false
  await tx.delete(users).where(eq(users.id, userId))
  return true
}

/**
 * EXP-1132: the placeholder members of teams about to be deleted. A team
 * delete cascades their team_members rows (and the team's issues/comments)
 * but never the `users` rows — read them BEFORE the delete and hand them to
 * `deletePlaceholdersIfOrphaned` after it, or they linger forever as
 * zero-team ghosts that a later invite to the same address cannot bind to.
 */
export async function collectPlaceholderMemberIds(
  tx: DbOrTx,
  teamIds: string[]
): Promise<string[]> {
  if (teamIds.length === 0) return []
  const rows = await tx
    .select({ id: users.id })
    .from(teamMembers)
    .innerJoin(users, eq(users.id, teamMembers.userId))
    .where(
      and(inArray(teamMembers.teamId, teamIds), isNotNull(users.placeholderAt))
    )
  return [...new Set(rows.map((row) => row.id))]
}

/** `deletePlaceholderIfOrphaned` over a batch (team deletes). */
export async function deletePlaceholdersIfOrphaned(
  tx: DbOrTx,
  userIds: string[]
): Promise<void> {
  for (const userId of userIds) {
    await deletePlaceholderIfOrphaned(tx, userId)
  }
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
