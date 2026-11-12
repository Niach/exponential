package com.exponential.app.domain

import java.time.Instant

/**
 * #EXP-1797 (issue-lifecycle.json `trash`): the team's issue trash ×4, the
 * `Deleted issues` band on the team settings Boards page (web `DeletedIssues`,
 * TS `issueTrashCaption` / `issueTrashActions` / `issueTrashRowText` /
 * `isIssuePurgeDue`). Locked by `IssueTrashTest`.
 */
object IssueTrash {
    const val RETENTION_DAYS = 30L
    const val BAND = "Deleted issues"
    const val ROW = "{identifier} {title}"
    const val RESTORE = "Restore"
    const val PURGE = "Delete forever"

    enum class Action(val key: String) { Restore("restore"), Purge("purge") }

    private const val HOUR_MS = 3_600_000L
    private const val DAY_MS = 24 * HOUR_MS

    /** The purge countdown: in hours inside the last two days, else in whole days (both rounded up). */
    fun caption(purgeAt: String?, now: Instant = Instant.now()): String {
        val at = parse(purgeAt) ?: return "Purges soon"
        val ms = at.toEpochMilli() - now.toEpochMilli()
        if (ms <= 0) return "Purging soon"
        if (ms <= 48 * HOUR_MS) return "Purges in ~${ceilDiv(ms, HOUR_MS)}h"
        return "Purges in ~${ceilDiv(ms, DAY_MS)}d"
    }

    /** The row's pills for a member of [role], display order (`access.purge` = owners). */
    fun actions(role: String): List<Action> =
        if (role == DomainContract.teamRoleOwner) listOf(Action.Restore, Action.Purge) else listOf(Action.Restore)

    fun rowText(identifier: String, title: String): String =
        ROW.replace("{identifier}", identifier).replace("{title}", title)

    /** The sweep's rule: due once deletedAt + retention has passed. */
    fun isPurgeDue(deletedAt: String?, now: Instant = Instant.now()): Boolean {
        val at = parse(deletedAt) ?: return false
        return at.toEpochMilli() + RETENTION_DAYS * DAY_MS <= now.toEpochMilli()
    }

    private fun ceilDiv(a: Long, b: Long) = (a + b - 1) / b

    private fun parse(raw: String?): Instant? =
        raw?.takeIf { it.isNotBlank() }?.let { runCatching { Instant.parse(it.trim()) }.getOrNull() }
}
