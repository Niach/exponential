package com.exponential.app.ui.components

import com.exponential.app.data.db.CommentEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.isFromReporter
import com.exponential.app.data.db.isToReporter
import com.exponential.app.data.db.isViaMcp
import com.exponential.app.domain.ReporterReply

// The server no longer syncs user rows for co-members of a public team, so a
// userId can resolve to no [UserEntity]. Rather than leak a raw id — or render a
// blank "Someone" that collides for everyone — derive a stable, anonymized
// pseudonym from the id's tail. Shared by every surface that resolves a userId to
// a display name (comments, events, members, assignee, steer).

/** Deterministic anonymized name for an unsynced user: `Member 8F3A`. */
fun memberPseudonym(userId: String?): String {
    if (userId.isNullOrBlank()) return "Someone"
    return "Member ${userId.takeLast(4).uppercase()}"
}

/**
 * Real name, else email, else the anonymized `Member XXXX` pseudonym.
 *
 * `name` is treated as missing when blank, not just null: Better Auth stores an
 * empty string (NOT NULL column) for an Apple-ID login that arrives without a
 * name, so a nil-only check would render a blank Text + empty initials. Falling
 * back to the email matches web's truthy `displayUserName`.
 */
fun userDisplayName(user: UserEntity?, userId: String?): String =
    user?.name?.takeIf { it.isNotBlank() } ?: user?.email ?: memberPseudonym(userId ?: user?.id)

/**
 * SLOP-4 (fixture `reporter-reply.json`, web `authorLabel`): the name a
 * comment card carries. A `reporter` comment has no users row — it names the
 * submission's reporter ([reporterName], else "Anonymous visitor"); a member
 * comment names its synced author; a member whose row is gone (left the team,
 * deleted) reads "Former member".
 */
fun commentAuthorName(comment: CommentEntity, author: UserEntity?, reporterName: String?): String {
    if (comment.isFromReporter) return ReporterReply.displayName(reporterName)
    if (author == null) return ReporterReply.FORMER_MEMBER_NAME
    return userDisplayName(author, comment.authorId)
}

/**
 * The muted caption after the time (web `commentCaption`, fixture
 * `reporter-reply.json` `captions`): "reporter" (the widget reporter wrote
 * it), then "to reporter" (a member's reply that was emailed out), then
 * "via MCP" (an agent posted it). Every part that applies shows, joined by
 * " · "; none = no caption.
 */
fun commentCaption(comment: CommentEntity): String? = listOfNotNull(
    ReporterReply.REPORTER_CAPTION.takeIf { comment.isFromReporter },
    ReporterReply.TO_REPORTER_CAPTION.takeIf { comment.isToReporter },
    VIA_MCP_CAPTION.takeIf { comment.isViaMcp },
).takeIf { it.isNotEmpty() }?.joinToString(CAPTION_SEPARATOR)

internal const val VIA_MCP_CAPTION = "via MCP"
internal const val CAPTION_SEPARATOR = " · "
