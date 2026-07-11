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
import com.exponential.app.data.db.UserEntity
import com.exponential.app.ui.components.userDisplayName
import kotlinx.serialization.json.contentOrNull

// Compact Linear-style activity line for non-agent events (status/assignee/label).
// Extracted from CommentThread.kt (pure move — no behavior change).
@Composable
internal fun EventRow(
    event: IssueEventEntity,
    actor: UserEntity?,
    releaseNames: Map<String, String> = emptyMap(),
) {
    val who = userDisplayName(actor, event.actorUserId)
    Row(
        modifier = Modifier.fillMaxWidth().padding(vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Box(Modifier.size(6.dp).clip(CircleShape).background(CommentMeta))
        Text(
            "$who ${eventPhrase(event, releaseNames)} · ${relativeTime(event.createdAt)}",
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
    "release_added" -> "added this to a release"
    "release_removed" -> "removed this from a release"
    else -> type.replace('_', ' ')
}

// A richer phrase for release membership events: resolves the release name
// from the payload's `releaseId` against the synced releases table. A deleted
// release leaves no name behind — fall back to eventVerb's generic wording.
internal fun eventPhrase(event: IssueEventEntity, releaseNames: Map<String, String>): String {
    if (event.type != "release_added" && event.type != "release_removed") {
        return eventVerb(event.type)
    }
    val name = eventPayloadField(event.payload, "releaseId")?.let { releaseNames[it] }
        ?: return eventVerb(event.type)
    return if (event.type == "release_added") {
        "added this to release $name"
    } else {
        "removed this from release $name"
    }
}

// Pull a string scalar out of an issue_event's JSON payload (stored as
// stringified JSON). Null for missing/blank values or unparseable payloads.
private fun eventPayloadField(payload: String?, key: String): String? {
    if (payload.isNullOrBlank()) return null
    return runCatching {
        val obj = kotlinx.serialization.json.Json
            .parseToJsonElement(payload) as? kotlinx.serialization.json.JsonObject
        (obj?.get(key) as? kotlinx.serialization.json.JsonPrimitive)?.contentOrNull
    }.getOrNull()?.takeIf { it.isNotBlank() }
}
