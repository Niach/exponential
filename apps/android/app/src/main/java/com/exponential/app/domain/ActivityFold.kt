package com.exponential.app.domain

import com.exponential.app.data.db.IssueEventEntity
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/**
 * EXP-900: activity folding — READ-TIME ONLY. `issue_events` rows are never
 * deleted or rewritten; every client folds the synced rows itself with this
 * pure function (mirrored: web `lib/activity/fold.ts`, desktop
 * `domain::activity_fold`, iOS `ExpCore/ActivityFold.swift`), all locked
 * against `packages/domain-contract/fixtures/activity-fold.json`. A "Show all"
 * toggle on the timeline header returns the unfolded list (EXP-468).
 *
 * THE FOLD RULE: a run of events on the SAME issue and the SAME field by the
 * SAME actor, with no event (or comment, see [ActivityFold.ActivityBarrier])
 * from any OTHER actor on that issue in between, first-to-last within
 * [ActivityFold.FOLD_WINDOW_MS], collapses to its NET effect. A net of
 * "nothing changed" (A → B → A) disappears entirely; a net A → C shows as ONE
 * event carrying the first event's `from*` payload keys and the last event's
 * everything else, timestamped at the LAST event and keyed by ITS id.
 * `created`, `pr_opened` and `pr_merged` never fold ([foldFieldKey] → null)
 * and never break a run of the same actor; a run spanning MORE than the window
 * from first to last is left alone entirely (no partial fold). An event
 * without an actor never folds and breaks every open run on its issue.
 *
 * "Field" derives from the event type: `status_changed` → status,
 * `assignee_changed` → assignee, `priority_changed` → priority,
 * `estimate_changed` → estimate, `board_moved` → board, `label_added` /
 * `label_removed` → that ONE label (`payload.labelId`; an add and a remove
 * of the same label cancel), `relation_added` / `relation_removed` → that
 * one relation (`payload.type` + `payload.relatedIssueId`).
 */
object ActivityFold {

    /**
     * Something that is not an event but still breaks another actor's run: a
     * comment. "Reviewer says no" is usually a comment, not a status change,
     * so the timeline passes its comments here and the dev's progress →
     * review → progress round trip stays visible.
     */
    data class ActivityBarrier(
        val issueId: String,
        val actorUserId: String?,
        val createdAt: String,
    )

    /** First-to-last span a run may cover and still fold. */
    const val FOLD_WINDOW_MS: Long = 10L * 60L * 1000L

    private val EMPTY_PAYLOAD = JsonObject(emptyMap())

    /** The field an event edits, or null for an event that never folds. */
    fun foldFieldKey(event: IssueEventEntity): String? =
        fieldKey(event.type, payloadOf(event))

    /**
     * Input in chronological order (oldest first); output likewise.
     * [barriers] (comments) only ever break runs, they are never returned.
     * Neither the input list nor any input row is mutated: a folded run emits
     * a `copy` of its last event.
     */
    fun foldActivity(
        events: List<IssueEventEntity>,
        barriers: List<ActivityBarrier> = emptyList(),
    ): List<IssueEventEntity> {
        val steps = mutableListOf<Step>()
        // An unparseable timestamp must not teleport its row to the front of
        // the list, so it inherits the previous (chronological) input row's
        // instant and is emitted in place, never folded.
        var previousAt = 0L
        events.forEachIndexed { index, event ->
            val parsed = WireTimestamps.parseEpochMs(event.createdAt)
            if (parsed != null) previousAt = parsed
            steps += Step.Event(
                at = parsed ?: previousAt,
                index = index,
                event = event,
                payload = payloadOf(event),
                datable = parsed != null,
            )
        }
        barriers.forEachIndexed { index, barrier ->
            val at = WireTimestamps.parseEpochMs(barrier.createdAt) ?: return@forEachIndexed
            steps += Step.Barrier(
                at = at,
                // A barrier at the same instant as an event sorts BEFORE it,
                // so a comment written together with a change still separates
                // it from what follows.
                index = index - barriers.size,
                barrier = barrier,
            )
        }
        steps.sortWith(compareBy({ it.at }, { it.index }))

        // Open runs keyed by "issueId actor field".
        val open = LinkedHashMap<String, Run>()
        val out = mutableListOf<Out>()

        fun breakOthers(issueId: String, actorUserId: String?) {
            val iterator = open.entries.iterator()
            while (iterator.hasNext()) {
                val run = iterator.next().value
                if (run.issueId != issueId) continue
                if (actorUserId != null && run.actorUserId == actorUserId) continue
                flushRun(run, out)
                iterator.remove()
            }
        }

        for (step in steps) {
            when (step) {
                is Step.Barrier -> breakOthers(step.barrier.issueId, step.barrier.actorUserId)
                is Step.Event -> {
                    val actor = step.event.actorUserId
                    breakOthers(step.event.issueId, actor)
                    val field = if (actor != null && step.datable) {
                        fieldKey(step.event.type, step.payload)
                    } else {
                        null
                    }
                    if (actor == null || field == null) {
                        out += Out(step.event, step.index, step.at)
                        continue
                    }
                    val key = "${step.event.issueId} $actor $field"
                    val run = open[key]
                    if (run != null) {
                        run.events += step
                    } else {
                        open[key] = Run(step.event.issueId, actor, mutableListOf(step))
                    }
                }
            }
        }
        for (run in open.values) flushRun(run, out)

        out.sortWith(compareBy({ it.at }, { it.index }))
        return out.map { it.event }
    }

    private sealed interface Step {
        val at: Long
        val index: Int

        data class Event(
            override val at: Long,
            override val index: Int,
            val event: IssueEventEntity,
            val payload: JsonObject,
            /** False when `createdAt` did not parse — such a row never folds. */
            val datable: Boolean,
        ) : Step

        data class Barrier(
            override val at: Long,
            override val index: Int,
            val barrier: ActivityBarrier,
        ) : Step
    }

    private class Run(
        val issueId: String,
        val actorUserId: String,
        val events: MutableList<Step.Event>,
    )

    private data class Out(val event: IssueEventEntity, val index: Int, val at: Long)

    private fun flushRun(run: Run, out: MutableList<Out>) {
        val events = run.events
        if (events.size == 1) {
            val only = events[0]
            out += Out(only.event, only.index, only.at)
            return
        }
        val first = events.first()
        val last = events.last()
        if (last.at - first.at > FOLD_WINDOW_MS) {
            events.forEach { out += Out(it.event, it.index, it.at) }
            return
        }
        if (beforeOf(first) == afterOf(last)) return
        out += Out(
            event = last.event.copy(payload = encode(mergePayload(first, last))),
            index = last.index,
            at = last.at,
        )
    }

    /** The last event wearing the first event's `from*` keys. */
    private fun mergePayload(first: Step.Event, last: Step.Event): JsonObject {
        val merged = LinkedHashMap<String, JsonElement>()
        for ((key, value) in last.payload) if (!key.startsWith("from")) merged[key] = value
        for ((key, value) in first.payload) if (key.startsWith("from")) merged[key] = value
        return JsonObject(merged)
    }

    private fun fieldKey(type: String, payload: JsonObject): String? = when (type) {
        "status_changed" -> "status"
        "assignee_changed" -> "assignee"
        "priority_changed" -> "priority"
        "estimate_changed" -> "estimate"
        "board_moved" -> "board"
        "label_added", "label_removed" ->
            optionalString(payload["labelId"])?.let { "label:$it" }
        "relation_added", "relation_removed" -> {
            val relationType = optionalString(payload["type"])
            val relatedIssueId = optionalString(payload["relatedIssueId"])
            if (relationType != null && relatedIssueId != null) {
                "relation:$relationType:$relatedIssueId"
            } else {
                null
            }
        }
        else -> null
    }

    // The "before" side of the first event and the "after" side of the last
    // one, as comparable strings. Equal = the run changed nothing. A status
    // compares on the precise `statusId` pair when the payload carries one
    // (EXP-314) and on the legacy enum otherwise; presence toggles (labels,
    // relations) compare as present/absent.
    private fun beforeOf(step: Step.Event): String {
        val payload = step.payload
        return when (step.event.type) {
            "status_changed" ->
                asString(payload["fromStatusId"]) ?: asString(payload["from"]) ?: ""
            "assignee_changed", "priority_changed", "estimate_changed" -> asString(payload["from"]) ?: ""
            "board_moved" -> asString(payload["fromBoardId"]) ?: ""
            "label_added", "relation_added" -> "absent"
            "label_removed", "relation_removed" -> "present"
            else -> ""
        }
    }

    private fun afterOf(step: Step.Event): String {
        val payload = step.payload
        return when (step.event.type) {
            "status_changed" ->
                asString(payload["toStatusId"]) ?: asString(payload["to"]) ?: ""
            "assignee_changed", "priority_changed", "estimate_changed" -> asString(payload["to"]) ?: ""
            "board_moved" -> asString(payload["toBoardId"]) ?: ""
            "label_added", "relation_added" -> "present"
            "label_removed", "relation_removed" -> "absent"
            else -> ""
        }
    }

    /** The payload column is a JSON STRING; a non-object (or junk) reads empty. */
    private fun payloadOf(event: IssueEventEntity): JsonObject {
        val raw = event.payload?.takeIf { it.isNotBlank() } ?: return EMPTY_PAYLOAD
        return runCatching { Json.parseToJsonElement(raw) as? JsonObject }
            .getOrNull() ?: EMPTY_PAYLOAD
    }

    private fun encode(payload: JsonObject): String =
        Json.encodeToString(JsonObject.serializer(), payload)

    /** JS `String(value)` for a present, non-null value; null when absent. */
    private fun asString(value: JsonElement?): String? {
        val primitive = value as? JsonPrimitive ?: return value?.toString()
        return if (primitive is JsonNull) null else primitive.content
    }

    /** JS `typeof value === "string" && value.length > 0`. */
    private fun optionalString(value: JsonElement?): String? {
        val primitive = value as? JsonPrimitive ?: return null
        if (!primitive.isString) return null
        return primitive.content.takeIf { it.isNotEmpty() }
    }
}
