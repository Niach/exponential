package com.exponential.app.domain

import com.exponential.app.data.db.TeamInviteEntity

/**
 * EXP-630 placeholder members: a member whose email invite is still
 * unaccepted has not joined yet. [PENDING] = the link still works, [EXPIRED]
 * = it lapsed or was revoked (the roster row stays until removed; web's
 * "Resend invite" mints a fresh link). Mirrors web `lib/placeholder-status.ts`
 * — Android keeps the invite surface itself web-only (EXP-725), so the badge
 * is all four clients share here.
 *
 * EXP-1076 adds [UNSENT]: the Linear import seats everyone it found so
 * attributions land, but nobody was ever asked to join — those rows carry
 * `sent_at` null (and an already-lapsed `expires_at`, so expiry readers treat
 * the token as dead). "Invite expired" would be a lie there; it reads
 * "Not invited".
 *
 * [rank] is web's `RANK`: a member may hold several rows (a superseded link,
 * then a fresh one) and wears the best news — a live link outranks a lapsed
 * one, and any issued link outranks "never invited".
 */
enum class PlaceholderStatus(val label: String, internal val rank: Int) {
    UNSENT("Not invited", 0),
    PENDING("Invited", 2),
    EXPIRED("Invite expired", 1),
}

/**
 * The badge each placeholder member's row wears, keyed by the placeholder
 * user id. An invite counts only while it is bound to a placeholder AND
 * unaccepted; a null (or empty) `sent_at` is [PlaceholderStatus.UNSENT]
 * whatever the expiry says; else an [expiresAt] that won't parse reads as
 * lapsed (web's `new Date(invalid) > now` is false too).
 */
fun placeholderStatuses(
    invites: List<TeamInviteEntity>,
    nowMs: Long = System.currentTimeMillis(),
): Map<String, PlaceholderStatus> {
    val result = LinkedHashMap<String, PlaceholderStatus>()
    for (invite in invites) {
        val userId = invite.placeholderUserId ?: continue
        if (invite.acceptedAt != null) continue
        val status = if (invite.sentAt.isNullOrEmpty()) {
            // Never sent ⇒ never expired: the import stamps
            // `expires_at = created_at` on purpose, so expiry says nothing
            // about this row.
            PlaceholderStatus.UNSENT
        } else {
            val expiresMs = WireTimestamps.parseEpochMs(invite.expiresAt)
            if (expiresMs != null && expiresMs > nowMs) {
                PlaceholderStatus.PENDING
            } else {
                PlaceholderStatus.EXPIRED
            }
        }
        // pending > expired > unsent for the same member, in any input order.
        val current = result[userId]
        if (current != null && current.rank >= status.rank) continue
        result[userId] = status
    }
    return result
}

/**
 * The team's PENDING invites: unaccepted AND unexpired — web
 * `members-section.tsx`'s "Pending invites" predicate. The DAO's
 * `accepted_at IS NULL` alone is not enough since EXP-630 (#810): revoking a
 * placeholder's invite keeps the row with `expires_at = now` (member lists
 * read the lapsed row as "Invite expired"), so revoked and naturally expired
 * links both sit in the unaccepted set. Filtered here rather than in SQL:
 * `expires_at` arrives as Postgres text OR ISO, which don't string-sort
 * against each other.
 */
fun pendingInvites(
    invites: List<TeamInviteEntity>,
    nowMs: Long = System.currentTimeMillis(),
): List<TeamInviteEntity> = invites.filter { invite ->
    invite.acceptedAt == null &&
        (WireTimestamps.parseEpochMs(invite.expiresAt) ?: Long.MIN_VALUE) > nowMs
}
