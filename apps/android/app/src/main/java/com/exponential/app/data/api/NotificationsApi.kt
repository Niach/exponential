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

/** Session 58cb32df: `notifications.markAllRead({ teamId })` — only that
 *  team's rows (phones show ONE active team). */
@Serializable
private data class NotificationsTeamInput(@SerialName("teamId") val teamId: String)

/** #EXP-1815: `user_notification_prefs.source_prefs` on the wire; absent = on. */
@Serializable
data class SourcePrefsDto(
    @SerialName("assistant") val assistant: Boolean? = null,
    @SerialName("temporary") val temporary: Boolean? = null,
    @SerialName("agents") val agents: Map<String, Boolean>? = null,
)

/** The part of `notifications.emailPrefs` the phones read (the rest is web's email card). */
@Serializable
data class NotificationPrefsDto(@SerialName("sourcePrefs") val sourcePrefs: SourcePrefsDto? = null)

@Serializable
private data class SourcePrefsInput(@SerialName("sourcePrefs") val sourcePrefs: SourcePrefsDto)

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

    /** #EXP-1815: the member's source switches (`notifications.emailPrefs`). */
    suspend fun sourcePrefs(accountId: String): SourcePrefsDto =
        trpc.query(
            accountId,
            path = "notifications.emailPrefs",
            input = NotificationsEmptyInput,
            inputSerializer = NotificationsEmptyInput.serializer(),
            outputSerializer = NotificationPrefsDto.serializer(),
        ).sourcePrefs ?: SourcePrefsDto()

    /** #EXP-1815: writes the WHOLE switches object (after `toggleSourcePref`). */
    suspend fun updateSourcePrefs(accountId: String, prefs: SourcePrefsDto) {
        trpc.mutationUnit(
            accountId,
            path = "notifications.updateEmailPrefs",
            input = SourcePrefsInput(prefs),
            inputSerializer = SourcePrefsInput.serializer(),
        )
    }
}
