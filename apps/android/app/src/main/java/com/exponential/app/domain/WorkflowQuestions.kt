package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonPrimitive

// EXP-1065 (workflow contract EXP-1082): a workflow's OPEN QUESTIONS — what
// its live runs asked and still wait on. The question lives in
// `coding_sessions.pending_question` (jsonb `{question, askedAt}`, raw text on
// CodingSessionEntity.pendingQuestion), never in a separate table. The ONE
// rule, ×4 (web `lib/workflows/open-questions.ts`, desktop
// `domain::workflow_questions`, iOS `WorkflowQuestions`): a live run
// (`running` / `in_review`) of the workflow, on a NODE, with a non-blank
// question; a planner run's question (no node) reaches the person through the
// notification and its own composer. An open question never changes a node's
// state, only the badge.

/** One open question of a workflow run. */
data class WorkflowOpenQuestion(
    val nodeId: String,
    val sessionId: String,
    val question: String,
    val askedAt: String,
)

object WorkflowQuestions {
    private val liveStatuses = setOf("running", "in_review")

    /** Every open question of [workflowId]'s live runs, oldest first. */
    fun open(sessions: List<CodingSessionEntity>, workflowId: String): List<WorkflowOpenQuestion> =
        sessions
            .asSequence()
            .filter { it.workflowId == workflowId && it.workflowNodeId != null && it.status in liveStatuses }
            .mapNotNull { session ->
                val pending = parse(session.pendingQuestion) ?: return@mapNotNull null
                val question = pending["question"]?.jsonPrimitive?.contentOrNull?.trim()
                if (question.isNullOrEmpty()) return@mapNotNull null
                WorkflowOpenQuestion(
                    nodeId = session.workflowNodeId!!,
                    sessionId = session.id,
                    question = question,
                    askedAt = pending["askedAt"]?.jsonPrimitive?.contentOrNull ?: "",
                )
            }
            .sortedWith(compareBy({ it.askedAt }, { it.sessionId }))
            .toList()

    /** The jsonb text as an object; null for no question or garbled text. */
    private fun parse(raw: String?): JsonObject? {
        val trimmed = raw?.trim().orEmpty()
        if (trimmed.isEmpty()) return null
        return runCatching { Json.parseToJsonElement(trimmed) as? JsonObject }.getOrNull()
    }
}
