package com.exponential.app.ui.issue

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.IssueEventEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.IssueStatus
import com.exponential.app.ui.components.userDisplayName
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

// Compact Linear-style activity line for non-agent events (status/assignee/label).
// Extracted from CommentThread.kt (pure move — no behavior change).
@Composable
internal fun EventRow(
    event: IssueEventEntity,
    usersById: Map<String, UserEntity>,
    labelsById: Map<String, LabelEntity>,
) {
    val who = userDisplayName(event.actorUserId?.let { usersById[it] }, event.actorUserId)
    val time = relativeTime(event.createdAt)
    val text = buildString {
        append(who).append(' ').append(eventPhrase(event, usersById, labelsById))
        // Only append the separator when there is a time to follow it — an
        // unparseable createdAt must not leave a dangling "·" (EXP-169).
        if (time.isNotEmpty()) append(" · ").append(time)
    }
    Row(
        modifier = Modifier.fillMaxWidth().padding(vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Box(Modifier.size(6.dp).clip(CircleShape).background(CommentMeta))
        Text(
            text,
            style = MaterialTheme.typography.labelSmall,
            color = CommentMeta,
        )
    }
}

// Human-readable verb for a synced issue event. Covers the surviving event kinds
// (issueEventTypeValues in the domain contract); anything else degrades to the
// type name with underscores spaced out.
internal fun eventVerb(type: String): String = when (type) {
    "status_changed" -> "changed the status"
    "assignee_changed" -> "changed the assignee"
    "label_added" -> "added a label"
    "label_removed" -> "removed a label"
    "pr_opened" -> "opened a pull request"
    "pr_merged" -> "merged the pull request"
    "board_moved" -> "moved this to another board"
    else -> type.replace('_', ' ')
}

// A richer phrase for the events whose payload carries detail (EXP-169 —
// mirrors iOS EventPhrases.swift). Missing payloads or unsynced lookup rows
// degrade to the bare verb; board_moved (EXP-57) is self-contained. The
// user/label maps are deliberately non-defaulted: a call site that forgets
// them must fail to compile, not silently render pseudonyms and bare verbs.
internal fun eventPhrase(
    event: IssueEventEntity,
    usersById: Map<String, UserEntity>,
    labelsById: Map<String, LabelEntity>,
): String {
    // One parse per phrase — status_changed/board_moved read two keys.
    val payload = parsedPayload(event.payload)
    fun field(key: String): String? =
        (payload?.get(key) as? JsonPrimitive)?.contentOrNull?.takeIf { it.isNotBlank() }
    return when (event.type) {
        "status_changed" -> {
            val to = field("to")
            val from = field("from")
            when {
                to == null -> eventVerb(event.type)
                from != null ->
                    "changed the status from ${IssueStatus.labelFor(from)} to ${IssueStatus.labelFor(to)}"
                else -> "changed the status to ${IssueStatus.labelFor(to)}"
            }
        }
        "assignee_changed" -> {
            val to = field("to")
            if (to == null) "unassigned this issue"
            else "assigned ${userDisplayName(usersById[to], to)}"
        }
        "label_added", "label_removed" -> {
            val verb = if (event.type == "label_added") "added" else "removed"
            field("labelId")?.let { labelsById[it]?.name }?.let { "$verb label $it" }
                ?: eventVerb(event.type)
        }
        "pr_opened", "pr_merged" -> {
            val verb = if (event.type == "pr_opened") "opened" else "merged"
            field("prNumber")?.let { "$verb PR #$it" } ?: eventVerb(event.type)
        }
        "board_moved" -> {
            val from = field("fromIdentifier")
            val to = field("toIdentifier")
            if (from != null && to != null) {
                "moved this to another board ($from → $to)"
            } else {
                eventVerb(event.type)
            }
        }
        else -> eventVerb(event.type)
    }
}

// The event's JSON payload (stored as stringified JSON) as an object, or null
// for missing/unparseable payloads.
private fun parsedPayload(payload: String?): JsonObject? {
    if (payload.isNullOrBlank()) return null
    return runCatching {
        kotlinx.serialization.json.Json.parseToJsonElement(payload) as? JsonObject
    }.getOrNull()
}
