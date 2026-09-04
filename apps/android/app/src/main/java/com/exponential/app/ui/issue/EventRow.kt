package com.exponential.app.ui.issue

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.IssueEventEntity
import com.exponential.app.data.db.LabelEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.ResolvedIssueStatus
import com.exponential.app.domain.relationEventPhrase
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.icons.ExpIcons
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

// Compact Linear-style activity line for non-agent events (status/assignee/label),
// with the shared timeline gutter on the connecting rail (EXP-240). The gutter
// marker is the event type's concept icon — a status change leads with the
// TARGET status's resolved icon in its color (EXP-595, web/desktop parity) —
// and only unknown event types keep the plain dot.
// Phrase functions untouched (EventPhrasesTest locks them).
@Composable
internal fun EventRow(
    event: IssueEventEntity,
    usersById: Map<String, UserEntity>,
    labelsById: Map<String, LabelEntity>,
    statuses: List<ResolvedIssueStatus>,
    lineAbove: Boolean = false,
    lineBelow: Boolean = false,
) {
    val who = userDisplayName(event.actorUserId?.let { usersById[it] }, event.actorUserId)
    val time = relativeTime(event.createdAt)
    val text = buildString {
        append(who).append(' ').append(eventPhrase(event, usersById, labelsById))
        // Only append the separator when there is a time to follow it — an
        // unparseable createdAt must not leave a dangling "·" (EXP-169).
        if (time.isNotEmpty()) append(" · ").append(time)
    }
    val glyph = eventGlyph(event, statuses)
    Row(
        modifier = Modifier.fillMaxWidth().height(IntrinsicSize.Min),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        TimelineGutter(
            lineAbove = lineAbove,
            lineBelow = lineBelow,
            marker = when (glyph) {
                is EventGlyph.Status -> {
                    {
                        StatusIcon(
                            glyph.status,
                            modifier = Modifier.align(Alignment.Center),
                            size = 14.dp,
                        )
                    }
                }
                is EventGlyph.Plain -> {
                    {
                        Icon(
                            glyph.icon,
                            contentDescription = null,
                            modifier = Modifier.align(Alignment.Center).size(14.dp),
                            tint = CommentMeta,
                        )
                    }
                }
                null -> null
            },
        )
        Text(
            text,
            style = MaterialTheme.typography.labelSmall,
            color = CommentMeta,
            modifier = Modifier.padding(vertical = 8.dp),
        )
    }
}

// The leading glyph of one activity row (EXP-595 — mirrors the web `EventRow`
// icon switch and the desktop `EventGlyph`): a shared-registry concept icon,
// or the resolved TARGET status for status changes (EXP-525). Null = no glyph
// (unknown event types keep the plain timeline dot).
internal sealed interface EventGlyph {
    data class Plain(val icon: ImageVector) : EventGlyph
    data class Status(val status: ResolvedIssueStatus) : EventGlyph
}

// [statuses] is the issue's team vocabulary in render order — a status change
// resolves the payload's `toStatusId` (or the legacy enum anchor) against it,
// and the shared fallback chain never fails.
internal fun eventGlyph(
    event: IssueEventEntity,
    statuses: List<ResolvedIssueStatus>,
): EventGlyph? = when (event.type) {
    "status_changed" -> {
        val payload = parsedPayload(event.payload)
        fun field(key: String): String? =
            (payload?.get(key) as? JsonPrimitive)?.contentOrNull?.takeIf { it.isNotBlank() }
        EventGlyph.Status(IssueStatusResolver.resolve(field("toStatusId"), field("to"), statuses))
    }
    "assignee_changed" -> EventGlyph.Plain(ExpIcons.eventAssigneeChanged)
    "label_added", "label_removed" -> EventGlyph.Plain(ExpIcons.eventLabelAdded)
    "board_moved" -> EventGlyph.Plain(ExpIcons.eventBoardMoved)
    "pr_opened" -> EventGlyph.Plain(ExpIcons.prOpen)
    "pr_merged" -> EventGlyph.Plain(ExpIcons.prMerged)
    "priority_changed" -> EventGlyph.Plain(ExpIcons.eventPriorityChanged)
    "relation_added" -> EventGlyph.Plain(ExpIcons.eventRelationAdded)
    "relation_removed" -> EventGlyph.Plain(ExpIcons.eventRelationRemoved)
    else -> null
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
    "relation_added" -> "added a relation"
    "relation_removed" -> "removed a relation"
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
            // EXP-314: newer payloads carry the status ROWS' display names
            // (toName/fromName) alongside the legacy enum anchors — prefer
            // them so a custom status reads by its real name; older rows fall
            // back to the anchor munge.
            val to = field("toName") ?: field("to")?.let { IssueStatus.labelFor(it) }
            val from = field("fromName") ?: field("from")?.let { IssueStatus.labelFor(it) }
            when {
                to == null -> eventVerb(event.type)
                from != null -> "changed the status from $from to $to"
                else -> "changed the status to $to"
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
        // EXP-530: priority wire values render capitalized ("urgent" →
        // "Urgent"); a missing side reads "None" — mirrors web priorityLabel.
        "priority_changed" ->
            "changed priority from ${priorityLabel(field("from"))} to ${priorityLabel(field("to"))}"
        // EXP-736: the wording is contract-shared (relationEventPhrase) —
        // `related` names the act, every other type the resulting state, read
        // from the payload's own SIDE of the edge. It phrases a thin payload
        // too (web parity), so there is no bare-verb fallback here.
        "relation_added", "relation_removed" -> relationEventPhrase(
            added = event.type == "relation_added",
            type = field("type"),
            identifier = field("relatedIdentifier"),
            direction = field("direction"),
        )
        else -> eventVerb(event.type)
    }
}

// EXP-530: whether a synced event renders a timeline row at all. `created`
// rows exist as the automation-trigger substrate — the issue header already
// shows creation, so every client suppresses them (web returns null there).
// The filter lives here (not in the composable) so the rule is lock-testable.
internal fun eventRowVisible(type: String): Boolean = type != "created"

private fun priorityLabel(raw: String?): String =
    raw?.replaceFirstChar { it.uppercaseChar() } ?: "None"

// The event's JSON payload (stored as stringified JSON) as an object, or null
// for missing/unparseable payloads.
private fun parsedPayload(payload: String?): JsonObject? {
    if (payload.isNullOrBlank()) return null
    return runCatching {
        kotlinx.serialization.json.Json.parseToJsonElement(payload) as? JsonObject
    }.getOrNull()
}
