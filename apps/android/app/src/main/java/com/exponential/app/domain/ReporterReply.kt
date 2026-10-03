package com.exponential.app.domain

/**
 * SLOP-4: a widget submission IS an issue, and the conversation with its
 * reporter is COMMENTS. The ONE pinned copy set for reporter comments and the
 * "Reply to reporter" toggle — `packages/domain-contract/fixtures/
 * reporter-reply.json`, byte-locked ×4 (`ReporterReplyTest`; web reads the
 * json itself, iOS `ReporterReply`, desktop `domain::reporter_reply`).
 */
object ReporterReply {
    /** The composer's leading-row pill. */
    const val TOGGLE_LABEL = "Reply to reporter"

    /** The field placeholder while the pill is ON; `{name}` = [displayName]. */
    const val PLACEHOLDER_ON = "Reply to {name}… (emailed to them)"

    /** A reporter with no name on their submission. */
    const val ANONYMOUS_NAME = "Anonymous visitor"

    /** A member comment whose author row is gone (left the team, deleted). */
    const val FORMER_MEMBER_NAME = "Former member"

    /** Caption after the time on a reporter's comment (where "via MCP" sits). */
    const val REPORTER_CAPTION = "reporter"

    /** Caption on a member's reply the server emailed to the reporter. */
    const val TO_REPORTER_CAPTION = "to reporter"

    /** `comments.create` answered `reporterEmailed: true`. */
    const val SENT_TOAST = "Reply emailed to the reporter."

    /** `reporterEmailed: false` — saved, shows on the reporter page, no mail. */
    const val NOT_SENT_TOAST = "Reply saved. No email was sent: this server has no mail transport."

    /** The `reporter_reply` notification rows' label. */
    const val NOTIFICATION_LABEL = "Reporter replied"

    /** The name a reporter's words carry: the submission's `reporterName`
     *  (trimmed), else the anonymous label — web `reporterDisplayName`. */
    fun displayName(reporterName: String?): String =
        reporterName?.trim()?.takeIf { it.isNotEmpty() } ?: ANONYMOUS_NAME

    /** The composer placeholder while the toggle is ON. */
    fun placeholderOn(name: String): String = PLACEHOLDER_ON.replace("{name}", name)
}
