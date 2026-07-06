package com.exponential.app.ui.components

import com.exponential.app.data.db.UserEntity

// The server no longer syncs user rows for co-members of a public workspace, so a
// userId can resolve to no [UserEntity]. Rather than leak a raw id — or render a
// blank "Someone" that collides for everyone — derive a stable, anonymized
// pseudonym from the id's tail. Shared by every surface that resolves a userId to
// a display name (comments, events, members, assignee, steer).

/** Deterministic anonymized name for an unsynced user: `Member 8F3A`. */
fun memberPseudonym(userId: String?): String {
    if (userId.isNullOrBlank()) return "Someone"
    return "Member ${userId.takeLast(4).uppercase()}"
}

/** Real name, else email, else the anonymized `Member XXXX` pseudonym. */
fun userDisplayName(user: UserEntity?, userId: String?): String =
    user?.name ?: user?.email ?: memberPseudonym(userId ?: user?.id)
