package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

// Mirrors apps/web/src/lib/trpc/notifications.ts — inbox mark-read.
@Serializable
private data class NotificationIdInput(@SerialName("id") val id: String)

@Serializable
private data class NotificationIssueInput(@SerialName("issueId") val issueId: String)

@Serializable
private object NotificationsEmptyInput

/** Thread 58cb32df: `notifications.markAllRead({ teamId })` — only that
 *  team's rows (phones show ONE active team). */
@Serializable
private data class NotificationsTeamInput(@SerialName("teamId") val teamId: String)

@Singleton
class NotificationsApi @Inject constructor(private val trpc: TrpcClient) {

    suspend fun markRead(accountId: String, id: String) {
        trpc.mutationUnit(
            accountId,
            path = "notifications.markRead",
            input = NotificationIdInput(id),
            inputSerializer = NotificationIdInput.serializer(),
        )
    }

    /** Opening an issue clears its inbox entries (EXP-92) — the read-on-open
     * safety net for push taps and app links that skip the inbox. */
    suspend fun markReadByIssue(accountId: String, issueId: String) {
        trpc.mutationUnit(
            accountId,
            path = "notifications.markReadByIssue",
            input = NotificationIssueInput(issueId),
            inputSerializer = NotificationIssueInput.serializer(),
        )
    }

    /** Mark all read; [teamId] = only that team's rows (absent = every team). */
    suspend fun markAllRead(accountId: String, teamId: String? = null) {
        if (teamId == null) {
            trpc.mutationUnit(
                accountId,
                path = "notifications.markAllRead",
                input = NotificationsEmptyInput,
                inputSerializer = NotificationsEmptyInput.serializer(),
            )
        } else {
            trpc.mutationUnit(
                accountId,
                path = "notifications.markAllRead",
                input = NotificationsTeamInput(teamId),
                inputSerializer = NotificationsTeamInput.serializer(),
            )
        }
    }
}
