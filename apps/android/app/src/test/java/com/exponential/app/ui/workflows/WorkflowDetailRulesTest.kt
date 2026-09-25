package com.exponential.app.ui.workflows

import com.exponential.app.data.db.CodingSessionEntity
import com.exponential.app.data.db.WorkflowEntity
import com.exponential.app.data.db.WorkflowNodeEntity
import com.exponential.app.domain.DomainContract
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** EXP-1087: the page rules the workflow detail view-model feeds its screen. */
class WorkflowDetailRulesTest {

    private fun node(id: String, state: String, note: String? = null) = WorkflowNodeEntity(
        id = id,
        workflowId = "wf-1",
        issueId = "issue-$id",
        state = state,
        note = note,
    )

    private fun draft(deviceId: String? = "dev-1", repositoryId: String? = "repo-1", nodes: Int = 2) = WorkflowEntity(
        id = "wf-1",
        teamId = "team-1",
        status = DomainContract.wfStatusDraft,
        deviceId = deviceId,
        repositoryId = repositoryId,
        metrics = """{"nodes":$nodes,"edges":0,"depth":1,"width":$nodes}""",
    )

    @Test
    fun `a holding node's chip caption is its note`() {
        val graph = WorkflowGraph(
            nodes = listOf(
                node("a", DomainContract.wfNodeStateRunning, note = "Waiting for EXP-1 to land"),
                node("b", DomainContract.wfNodeStateRunning),
            ),
        )
        val chips = workflowStrip(graph, emptyList()).flatMap { it.nodes }.associateBy { it.id }
        assertEquals("Waiting for EXP-1 to land", chips.getValue("a").caption)
        assertNotEquals("Waiting for EXP-1 to land", chips.getValue("b").caption)
    }

    @Test
    fun `a startable draft shows no notice`() {
        assertNull(workflowStartNotice(draft(), deviceLabel = "Mac"))
    }

    @Test
    fun `an empty draft names why Start is off`() {
        assertEquals("The workflow has no issues.", workflowStartNotice(draft(nodes = 0), deviceLabel = "Mac"))
    }

    @Test
    fun `a lost repository blocks Start`() {
        assertEquals(
            "The workflow's repository is gone.",
            workflowStartNotice(draft(repositoryId = null), deviceLabel = "Mac"),
        )
    }

    @Test
    fun `Pick device never repeats the pick-a-device sentence`() {
        assertNull(workflowStartNotice(draft(deviceId = null), deviceLabel = null))
    }

    @Test
    fun `Pick device still shows the other reasons`() {
        assertEquals(
            "The workflow has no issues.",
            workflowStartNotice(draft(deviceId = null, nodes = 0), deviceLabel = null),
        )
    }

    @Test
    fun `a started workflow shows no start notice`() {
        val running = draft().copy(status = DomainContract.wfStatusRunning)
        assertNull(workflowStartNotice(running, deviceLabel = "Mac"))
    }

    private fun run(id: String, role: String?, createdAt: String, nodeId: String = "a") = CodingSessionEntity(
        id = id,
        issueId = "issue-a",
        teamId = "team-1",
        userId = "me",
        status = DomainContract.codingSessionStatusRunning,
        startedAt = createdAt,
        createdAt = createdAt,
        updatedAt = createdAt,
        workflowNodeId = nodeId,
        workflowRole = role,
    )

    @Test
    fun `a node's run is its recorded session, else its newest author run, never its reviewer`() {
        val author1 = run("s1", DomainContract.wfSessionRoleAuthor, "2026-09-25T10:00:00Z")
        val author2 = run("s2", DomainContract.wfSessionRoleAuthor, "2026-09-25T11:00:00Z")
        val review = run("s3", "review", "2026-09-25T12:00:00Z")
        val other = run("s4", DomainContract.wfSessionRoleAuthor, "2026-09-25T13:00:00Z", nodeId = "b")
        // Unordered on purpose: the fallback may not lean on the query order.
        val rows = listOf(review, author2, other, author1)
        val byId = rows.associateBy { it.id }
        val a = node("a", DomainContract.wfNodeStateRunning)
        assertEquals("s2", workflowNodeSession(a, byId, rows)?.id)
        assertEquals("s1", workflowNodeSession(a.copy(sessionId = "s1"), byId, rows)?.id)
        assertNull(workflowNodeSession(a, byId, listOf(review)))
    }
}
