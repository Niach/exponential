package com.exponential.app.data.push

import com.exponential.app.data.auth.ServerAccount

/**
 * Pure routing rules for a tapped push, shared by the two delivery paths: the
 * foreground [FcmService] (which builds the PendingIntent) and the background
 * tap MainActivity receives as launcher-intent extras once the FCM SDK rendered
 * the notification itself.
 *
 * The payload carries its recipient's SERVER user id so multi-account clients
 * can open the notification under the account it belongs to. Android used to
 * drop the deep link whenever that wasn't the active account (REV2-81), which
 * reads as a broken notification; the id now resolves to a signed-in account
 * and the tap switches to it first (the same thing a verified App Link into
 * another account's issue already does).
 */
object PushDeepLinks {

    /** Push `type` of the issue-less helpdesk reply notifications (EXP-180). */
    const val TYPE_SUPPORT_REPLY = "support_reply"

    /** Query param carrying the push's recipient through the deep link. */
    const val PARAM_USER_ID = "userId"

    sealed interface Target {
        data class Issue(val id: String) : Target
        data class SupportThread(val id: String) : Target
    }

    /** What a tapped push should open, or null when it carries no target. */
    fun target(type: String?, issueId: String?, threadId: String?): Target? = when {
        !issueId.isNullOrEmpty() -> Target.Issue(issueId)
        type == TYPE_SUPPORT_REPLY && !threadId.isNullOrEmpty() ->
            Target.SupportThread(threadId)
        else -> null
    }

    /**
     * The `exponential://` link for [target]. [targetUserId] rides along so the
     * MainActivity end of the hop can switch accounts before navigating —
     * without it a cross-account tap would resolve the id against the wrong
     * account's local database.
     */
    fun uri(target: Target, targetUserId: String?): String {
        val base = when (target) {
            is Target.Issue -> "exponential://issue/${target.id}"
            is Target.SupportThread -> "exponential://support/${target.id}"
        }
        if (targetUserId.isNullOrEmpty()) return base
        return "$base?$PARAM_USER_ID=${percentEncode(targetUserId)}"
    }

    // URI-style percent-encoding (never form-style): the reader is
    // Uri.getQueryParameter, which decodes `%XX` but leaves `+` literal.
    private fun percentEncode(value: String): String = buildString {
        for (byte in value.toByteArray(Charsets.UTF_8)) {
            val char = byte.toInt().toChar()
            val unreserved = (char in 'a'..'z') || (char in 'A'..'Z') ||
                (char in '0'..'9') || char in "-._~"
            if (unreserved) append(char) else append('%').append("%02X".format(byte))
        }
    }

    /** Which account a tapped push should open under. */
    sealed interface Account {
        /** Open under the active account: no user hint, or it already matches. */
        data object Active : Account

        /** Switch to this signed-in account before navigating. */
        data class Switch(val id: String) : Account

        /** No signed-in account owns it (signed out since it was sent) — drop. */
        data object Unknown : Account
    }

    fun resolveAccount(
        accounts: List<ServerAccount>,
        activeAccountId: String?,
        targetUserId: String?,
    ): Account {
        // Servers predating the hint (and every non-push producer of these
        // links) omit the user id — stay on the active account, as before.
        if (targetUserId.isNullOrEmpty()) return Account.Active
        val candidates = accounts.filter { it.token != null && it.userId == targetUserId }
        if (candidates.any { it.id == activeAccountId }) return Account.Active
        // The same user signed into two servers resolves to the one they used
        // last — either way the notification opens somewhere it exists.
        val fallback = candidates.maxByOrNull { it.lastUsedAt } ?: return Account.Unknown
        return Account.Switch(fallback.id)
    }
}
