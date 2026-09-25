package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import org.junit.Assert.assertEquals
import org.junit.Ignore
import org.junit.Test

/** EXP-1082: the open-question selector (EXP-1065 implements it). */
class WorkflowQuestionsTest {

    private fun run(
        id: String,
        workflowId: String?,
        nodeId: String?,
        status: String = "running",
        question: String? = null,
    ) = CodingSessionEntity(
        id = id,
        teamId = "team-1",
        userId = "user-1",
        status = status,
        workflowId = workflowId,
        workflowNodeId = nodeId,
        workflowRole = DomainContract.wfSessionRoleAuthor,
        startedAt = "2026-09-25T09:00:00Z",
        createdAt = "2026-09-25T09:00:00Z",
        updatedAt = "2026-09-25T09:00:00Z",
        pendingQuestion = question?.let { """{"question":"$it","askedAt":"2026-09-25T10:00:00Z"}""" },
    )

    @Ignore("EXP-1065")
    @Test
    fun `lists the open question of each live run of the workflow`() {
        val sessions = listOf(
            run("s1", "wf-1", "n1", question = "Which API?"),
            run("s2", "wf-1", "n2"),
            run("s3", "wf-1", "n3", status = "ended", question = "Stale?"),
            run("s4", "wf-2", "n4", question = "Other workflow?"),
        )
        assertEquals(
            listOf(WorkflowOpenQuestion("n1", "s1", "Which API?", "2026-09-25T10:00:00Z")),
            WorkflowQuestions.open(sessions, "wf-1"),
        )
    }
}
