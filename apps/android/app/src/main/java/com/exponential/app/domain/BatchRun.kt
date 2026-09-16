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
// The covered set has two sources, in this order:
//   1. `coding_sessions.batch_issue_ids` — written at start, so the name is
//      right from the run's first second (the composer's order, preserved).
//   2. the issues sharing the row's `branch` — what `pr_open` stamped on both
//      sides (EXP-545), which names batches started before the column existed
//      or by a client too old to send it, from the moment their PR opens.
//
// The twin of web `lib/batch-run.ts`, desktop `domain::batch_run` and iOS
// `BatchRun.swift`: same order, same `+N`, same fallback string, same test
// names.

/** The one string a batch with no knowable issues shows. Byte-identical ×4. */
const val BATCH_RUN_FALLBACK = "Batch run"

/** The launcher's batch branch marker (`exp/batch-<id8>`), deliberately
 *  lowercase so it can never parse as an issue branch. */
const val BATCH_RUN_BRANCH_PREFIX = "exp/batch-"

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
 * The issues a batch run covers, in NAMING order: the stored order when the
 * row recorded it, else the branch-mates oldest first (a deterministic order
 * every client reaches the same way — `created_at` is on every issue row,
 * identifiers break the tie).
 */
fun batchRunIssues(
    session: CodingSessionEntity,
    issues: List<IssueEntity>,
): List<IssueEntity> {
    if (!isBatchRun(session)) return emptyList()
    val ids = batchRunIssueIds(session.batchIssueIds)
    if (ids.isNotEmpty()) {
        val byId = issues.associateBy { it.id }
        return ids.mapNotNull(byId::get)
    }
    val branch = session.branch?.takeIf { it.startsWith(BATCH_RUN_BRANCH_PREFIX) }
        ?: return emptyList()
    // ISO-8601 UTC stamps order lexicographically, like every other list here.
    return issues
        .filter { it.branch == branch }
        .sortedWith(compareBy({ it.createdAt }, { it.identifier }))
}

/**
 * Name a batch run. [issues] is whatever the caller has synced; only the
 * covered ones are read. A batch whose issues are all unknown (no stored ids,
 * no PR yet — or a row whose issues left the viewer's teams) keeps the old
 * generic label rather than inventing one.
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
