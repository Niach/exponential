package com.exponential.app.ui.workflows

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
}
