package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.IssueEntity
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonPrimitive

// EXP-876 — how a BATCH run is NAMED.
//
// Every batch row used to read "Batch run" — one string for every batch this
// team ever ran, so two of them in a list (or one running beside last night's)
// could not be told apart at all.
//
// A batch names itself after the issues it covers: the first one's identifier
// with `+N` for the rest in the mono slot, that issue's title as the subject —
// the same two-part row an issue run renders, so one layout keeps serving
// every kind (EXP-874). The trailing control is still none: EXP-893 took the
// open-issue circle off every session row on every client, and it was exactly
// the control a multi-issue run could never answer.
//
// The covered set has ONE source: `coding_sessions.batch_issue_ids`, written
// at start (the composer's order, preserved) and backfilled server-side for
// every batch that predates the column (EXP-972), so a row without it is
// simply not a batch anyone can name. The branch-mates fallback (the issues
// `pr_open` stamped with the run's `exp/batch-<id8>` branch) is gone.
//
// The twin of web `lib/batch-run.ts`, desktop `domain::batch_run` and iOS
// `BatchRun.swift`: same order, same `+N`, same fallback string, same test
// names.

/** The one string a batch with no knowable issues shows. Byte-identical ×4. */
const val BATCH_RUN_FALLBACK = "Batch run"

/** What a batch row shows: `EXP-874 +2` beside the first issue's title. */
data class BatchRunName(
    /** The mono lead-in — null when no covered issue is known. */
    val identifier: String?,
    val subject: String,
)

/**
 * Read `coding_sessions.batch_issue_ids`. Same tolerance as every other jsonb
 * column here: the store hands it over as raw TEXT, and anything that is not a
 * list of non-empty strings names nothing rather than throwing.
 */
fun batchRunIssueIds(raw: String?): List<String> {
    val text = raw?.trim()?.takeIf { it.isNotEmpty() } ?: return emptyList()
    val array = runCatching { batchRunJson.parseToJsonElement(text) }.getOrNull() as? JsonArray
        ?: return emptyList()
    return array.mapNotNull { element ->
        val primitive = element as? JsonPrimitive ?: return@mapNotNull null
        primitive.takeIf { it.isString }?.content?.takeIf { it.isNotEmpty() }
    }
}

private val batchRunJson = Json { ignoreUnknownKeys = true; isLenient = true }

/** An issue-less, action-less run — the batch. (A chat run carries the
 *  reserved `Chat` snapshot, an action run its own, EXP-615.) */
fun isBatchRun(session: CodingSessionEntity): Boolean =
    session.issueId == null && session.actionName == null

/**
 * The issues a batch run covers, in NAMING order: the order the row stored.
 * An id whose issue has not synced is skipped, never a blank row.
 */
fun batchRunIssues(
    session: CodingSessionEntity,
    issues: List<IssueEntity>,
): List<IssueEntity> {
    if (!isBatchRun(session)) return emptyList()
    val ids = batchRunIssueIds(session.batchIssueIds)
    if (ids.isEmpty()) return emptyList()
    val byId = issues.associateBy { it.id }
    return ids.mapNotNull(byId::get)
}

/**
 * Name a batch run. [issues] is whatever the caller has synced; only the
 * covered ones are read. A batch whose issues are all unknown (no stored ids
 * — or a row whose issues left the viewer's teams) keeps the old generic
 * label rather than inventing one.
 */
fun batchRunName(
    session: CodingSessionEntity,
    issues: List<IssueEntity>,
): BatchRunName {
    val covered = batchRunIssues(session, issues)
    val first = covered.firstOrNull()
        ?: return BatchRunName(identifier = null, subject = BATCH_RUN_FALLBACK)
    // The STORED count wins over the resolved one: a batch of three whose
    // middle issue has not synced is still a batch of three, and "+1" would
    // quietly understate what the run is working on.
    val total = maxOf(batchRunIssueIds(session.batchIssueIds).size, covered.size)
    return BatchRunName(
        identifier = if (total > 1) "${first.identifier} +${total - 1}" else first.identifier,
        subject = first.title.trim().ifBlank { "Untitled issue" },
    )
}
