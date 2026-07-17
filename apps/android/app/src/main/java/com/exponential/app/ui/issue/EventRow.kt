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
import kotlinx.serialization.json.contentOrNull

// Compact Linear-style activity line for non-agent events (status/assignee/label).
// Extracted from CommentThread.kt (pure move — no behavior change).
@Composable
internal fun EventRow(
    event: IssueEventEntity,
    actor: UserEntity?,
    usersById: Map<String, UserEntity> = emptyMap(),
    labelsById: Map<String, LabelEntity> = emptyMap(),
) {
    val who = userDisplayName(actor, event.actorUserId)
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
    "project_moved" -> "moved this to another project"
    else -> type.replace('_', ' ')
}

// Human status label for a wire enum value. NOT IssueStatus.fromWire — that
// falls back to Backlog and would mislabel unknown statuses from newer servers.
internal fun statusLabel(wire: String): String =
    IssueStatus.entries.firstOrNull { it.wire == wire }?.label ?: wire.replace('_', ' ')

// A richer phrase for the events whose payload carries detail (EXP-169 —
// mirrors iOS EventPhrases.swift). Missing payloads or unsynced lookup rows
// degrade to the bare verb; project_moved (EXP-57) is self-contained.
internal fun eventPhrase(
    event: IssueEventEntity,
    usersById: Map<String, UserEntity> = emptyMap(),
    labelsById: Map<String, LabelEntity> = emptyMap(),
): String = when (event.type) {
    "status_changed" -> {
        val to = eventPayloadField(event.payload, "to")
        val from = eventPayloadField(event.payload, "from")
        when {
            to == null -> eventVerb(event.type)
            from != null -> "changed the status from ${statusLabel(from)} to ${statusLabel(to)}"
            else -> "changed the status to ${statusLabel(to)}"
        }
    }
    "assignee_changed" -> {
        val to = eventPayloadField(event.payload, "to")
        if (to == null) "unassigned this issue"
        else "assigned ${userDisplayName(usersById[to], to)}"
    }
    "label_added" -> eventPayloadField(event.payload, "labelId")
        ?.let { labelsById[it]?.name }
        ?.let { "added label $it" }
        ?: eventVerb(event.type)
    "label_removed" -> eventPayloadField(event.payload, "labelId")
        ?.let { labelsById[it]?.name }
        ?.let { "removed label $it" }
        ?: eventVerb(event.type)
    "pr_opened" -> eventPayloadField(event.payload, "prNumber")
        ?.let { "opened PR #$it" }
        ?: eventVerb(event.type)
    "pr_merged" -> eventPayloadField(event.payload, "prNumber")
        ?.let { "merged PR #$it" }
        ?: eventVerb(event.type)
    "project_moved" -> {
        val from = eventPayloadField(event.payload, "fromIdentifier")
        val to = eventPayloadField(event.payload, "toIdentifier")
        if (from != null && to != null) {
            "moved this to another project ($from → $to)"
        } else {
            eventVerb(event.type)
        }
    }
    else -> eventVerb(event.type)
}

// Pull a string scalar out of an issue_event's JSON payload (stored as
// stringified JSON). Null for missing/blank values or unparseable payloads;
// JSON numbers (e.g. prNumber) come back as their string content.
internal fun eventPayloadField(payload: String?, key: String): String? {
    if (payload.isNullOrBlank()) return null
    return runCatching {
        val obj = kotlinx.serialization.json.Json
            .parseToJsonElement(payload) as? kotlinx.serialization.json.JsonObject
        (obj?.get(key) as? kotlinx.serialization.json.JsonPrimitive)?.contentOrNull
    }.getOrNull()?.takeIf { it.isNotBlank() }
}
