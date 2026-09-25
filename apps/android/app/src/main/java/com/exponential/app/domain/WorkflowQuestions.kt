package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity

// EXP-1082: a workflow's OPEN QUESTIONS — what its live runs asked and still
// wait on. The question lives in `coding_sessions.pending_question` (jsonb
// `{question, askedAt}`, raw text on CodingSessionEntity.pendingQuestion),
// never in a separate table. STUB: EXP-1065 implements `open`, ×4 with web
// `lib/workflow-questions.ts`.

/** One open question of a workflow run. */
data class WorkflowOpenQuestion(
    val nodeId: String,
    val sessionId: String,
    val question: String,
    val askedAt: String,
)

object WorkflowQuestions {
    /** STUB (EXP-1065): every open question of [workflowId]'s live runs. */
    @Suppress("UNUSED_PARAMETER")
    fun open(sessions: List<CodingSessionEntity>, workflowId: String): List<WorkflowOpenQuestion> = emptyList()
}
