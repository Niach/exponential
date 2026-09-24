package com.exponential.app.domain

import com.exponential.app.data.db.TeamInviteEntity

/**
 * EXP-630 placeholder members: a member whose email invite is still
 * unaccepted has not joined yet. [PENDING] = the link still works, [EXPIRED]
 * = it lapsed or was revoked (the roster row stays until removed; web's
 * "Resend invite" mints a fresh link). Mirrors web `lib/placeholder-status.ts`
 * — Android keeps the invite surface itself web-only (EXP-725), so the badge
 * is all four clients share here.
 */
enum class PlaceholderStatus(val label: String) {
    PENDING("Invited"),
    EXPIRED("Invite expired"),
}

/**
 * The badge each placeholder member's row wears, keyed by the placeholder
 * user id. An invite counts only while it is bound to a placeholder AND
 * unaccepted; an [expiresAt] that won't parse reads as lapsed (web's
 * `new Date(invalid) > now` is false too).
 */
fun placeholderStatuses(
    invites: List<TeamInviteEntity>,
    nowMs: Long = System.currentTimeMillis(),
): Map<String, PlaceholderStatus> {
    val result = LinkedHashMap<String, PlaceholderStatus>()
    for (invite in invites) {
        val userId = invite.placeholderUserId ?: continue
        if (invite.acceptedAt != null) continue
        val expiresMs = WireTimestamps.parseEpochMs(invite.expiresAt)
        val status = if (expiresMs != null && expiresMs > nowMs) {
            PlaceholderStatus.PENDING
        } else {
            PlaceholderStatus.EXPIRED
        }
        // A pending link outranks an expired one for the same member.
        if (result[userId] == PlaceholderStatus.PENDING) continue
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
