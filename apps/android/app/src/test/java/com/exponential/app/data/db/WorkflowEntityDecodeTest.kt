package com.exponential.app.data.db

import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.launchOptions
import com.exponential.app.domain.shape
import com.exponential.app.domain.workflowNodeBudget
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-981: wire-decode vectors for the two workflow shapes. Every column of
 * the `/api/shapes/workflows` and `/api/shapes/workflow-nodes` allowlists, in
 * both wire dialects (Electric's snake_case and tRPC's camelCase), plus the
 * tolerant jsonb reads: a required field missing on the wire drops the row
 * forever, and an unknown key inside `launch`/`metrics` must be IGNORED, not
 * thrown on.
 */
class WorkflowEntityDecodeTest {

    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun `the snake_case workflow row decodes with every column`() {
        val row = """
            {
              "id": "wf-1",
              "team_id": "team-1",
              "repository_id": "repo-1",
              "name": "EXP-14 +3",
              "status": "draft",
              "device_id": "dev-1",
              "launch": {"agent":"claude","model":"opus","subagentModel":"fable","maxParallel":5},
              "gate": "human",
              "start_on": "contract",
              "integration_branch": "exp/wf-abcd1234",
              "final_pr_url": null,
              "final_pr_number": null,
              "final_pr_state": null,
              "decisions": "",
              "metrics": {"nodes":4,"edges":3,"depth":2,"width":2,"cycles":[["EXP-2","EXP-3"]],"cycleEdges":["n1\nn2"]},
              "started_at": null,
              "ended_at": null,
              "created_at": "2026-09-19 10:00:00+00",
              "updated_at": "2026-09-19 10:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(WorkflowEntity.serializer(), row)
        assertEquals("wf-1", entity.id)
        assertEquals("team-1", entity.teamId)
        assertEquals("repo-1", entity.repositoryId)
        assertEquals("EXP-14 +3", entity.name)
        assertEquals(DomainContract.wfStatusDraft, entity.status)
        assertEquals("dev-1", entity.deviceId)
        assertEquals(DomainContract.wfGateHuman, entity.gate)
        assertEquals(DomainContract.wfStartOnContract, entity.startOn)
        assertEquals("exp/wf-abcd1234", entity.integrationBranch)
        assertNull(entity.finalPrNumber)

        val launch = entity.launchOptions
        assertEquals("claude", launch.agent)
        assertEquals("opus", launch.model)
        assertEquals("fable", launch.subagentModel)
        assertEquals(5, launch.maxParallel)

        val shape = entity.shape
        assertEquals(4, shape.nodes)
        assertEquals(2, shape.depth)
        assertEquals(2, shape.width)
        assertEquals(listOf(listOf("EXP-2", "EXP-3")), shape.cycles)
        assertEquals(listOf("n1\nn2"), shape.cycleEdges)
    }

    @Test
    fun `the camelCase tRPC twin decodes too`() {
        val row = """
            {
              "id": "wf-2",
              "teamId": "team-1",
              "name": "EXP-9",
              "status": "running",
              "deviceId": "dev-1",
              "integrationBranch": "exp/wf-00000000",
              "finalPrNumber": 42,
              "startOn": "pr_open",
              "createdAt": "2026-09-19 10:00:00+00",
              "updatedAt": "2026-09-19 10:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(WorkflowEntity.serializer(), row)
        assertEquals("wf-2", entity.id)
        assertEquals(DomainContract.wfStatusRunning, entity.status)
        assertEquals(DomainContract.wfStartOnPrOpen, entity.startOn)
        assertEquals(42, entity.finalPrNumber)
        // Absent jsonb reads as the defaults, never as a dropped row.
        assertEquals(DomainContract.workflowMaxParallelDefault, entity.launchOptions.maxParallel)
        assertEquals(0, entity.shape.nodes)
        assertTrue(entity.shape.cycles.isEmpty())
    }

    @Test
    fun `unknown keys inside the jsonb columns are ignored`() {
        val row = """
            {
              "id": "wf-3",
              "team_id": "team-1",
              "name": "EXP-1",
              "launch": {"agent":"codex","somethingNew":true,"maxParallel":"7"},
              "metrics": {"nodes":"3","depth":1,"width":3,"cycles":[],"newCounter":9},
              "integration_branch": "exp/wf-11111111",
              "created_at": "2026-09-19 10:00:00+00",
              "updated_at": "2026-09-19 10:00:00+00"
            }
        """.trimIndent()
        val entity = json.decodeFromString(WorkflowEntity.serializer(), row)
        assertEquals("codex", entity.launchOptions.agent)
        // Postgres sometimes hands an integer over as its TEXT form.
        assertEquals(7, entity.launchOptions.maxParallel)
        assertEquals(3, entity.shape.nodes)
        // Everything the row did not say keeps its default.
        assertEquals("", entity.launchOptions.subagentModel)
    }

    @Test
    fun `the node row decodes its layout, its members and its touches`() {
        val row = """
            {
              "id": "node-1",
              "workflow_id": "wf-1",
              "team_id": "team-1",
              "issue_id": "issue-1",
              "member_issue_ids": ["issue-2", "issue-3"],
              "kind": "contract",
              "state": "blocked",
              "risk": "high",
              "wave": 0,
              "lane": 2,
              "on_cycle": true,
              "session_id": null,
              "attempt": 0,
              "base_branch": "exp/wf-abcd1234",
              "approved_at": "2026-09-19 11:00:00+00",
              "checkpoint_at": "2026-09-19 11:30:00+00",
              "after_node_ids": ["node-7", "node-9"],
              "note": "The rebase hit a conflict in apps/web/src/lib/workflows.ts",
              "budget": {"tokens": 120000, "minutes": 30},
              "touches": "{apps/web/**,packages/ui/**}",
              "created_at": "2026-09-19 10:00:00+00",
              "updated_at": "2026-09-19 10:00:00+00"
            }
        """.trimIndent()
        val node = json.decodeFromString(WorkflowNodeEntity.serializer(), row)
        assertEquals("node-1", node.id)
        assertEquals("wf-1", node.workflowId)
        assertEquals("issue-1", node.issueId)
        assertEquals(listOf("issue-2", "issue-3"), node.memberIssueIds)
        assertEquals(DomainContract.wfNodeKindContract, node.kind)
        assertEquals(DomainContract.wfNodeStateBlocked, node.state)
        assertEquals(DomainContract.wfRiskHigh, node.risk)
        assertEquals(0, node.wave)
        assertEquals(2, node.lane)
        assertTrue(node.onCycle)
        // A Postgres text[] arrives as its array literal inside a string.
        assertEquals(listOf("apps/web/**", "packages/ui/**"), node.touches)
        // EXP-982: the gate stamp and the engine's sentence.
        assertEquals("2026-09-19 11:00:00+00", node.approvedAt)
        assertEquals(
            "The rebase hit a conflict in apps/web/src/lib/workflows.ts",
            node.note,
        )
        // EXP-983: the contract stamp and the engine's serialization edges.
        assertEquals("2026-09-19 11:30:00+00", node.checkpointAt)
        assertEquals(listOf("node-7", "node-9"), node.afterNodeIds)
        val budget = workflowNodeBudget(node.budget)
        assertEquals(120000, budget?.tokens)
        assertEquals(30, budget?.minutes)
    }

    @Test
    fun `a bare node row defaults everything the wire left out`() {
        val row = """
            {
              "id": "node-2",
              "workflowId": "wf-1",
              "issueId": "issue-9",
              "approvedAt": "2026-09-19 11:00:00+00",
              "createdAt": "2026-09-19 10:00:00+00",
              "updatedAt": "2026-09-19 10:00:00+00"
            }
        """.trimIndent()
        val node = json.decodeFromString(WorkflowNodeEntity.serializer(), row)
        // The camelCase tRPC twin of the two EXP-982 columns; an unapproved
        // node simply says nothing, which is what the gate reads as "waiting".
        assertEquals("2026-09-19 11:00:00+00", node.approvedAt)
        assertNull(node.note)
        assertEquals(DomainContract.wfNodeKindLeaf, node.kind)
        assertEquals(DomainContract.wfNodeStateBlocked, node.state)
        assertEquals(DomainContract.wfRiskMedium, node.risk)
        assertEquals(0, node.wave)
        assertEquals(0, node.lane)
        assertTrue(node.memberIssueIds.isEmpty())
        assertTrue(node.touches.isEmpty())
        assertNull(workflowNodeBudget(node.budget))
        // EXP-983: a node that published nothing and collided with nobody.
        assertNull(node.checkpointAt)
        assertTrue(node.afterNodeIds.isEmpty())
    }

    @Test
    fun `the serialization edges decode in every wire dialect`() {
        // Electric ships a jsonb cell as its JSON TEXT inside a string; tRPC
        // and tests hand over a native array. Anything else — a null, a
        // malformed cell — reads as EMPTY rather than dropping the node row.
        fun nodeWith(afterNodeIds: String) = json.decodeFromString(
            WorkflowNodeEntity.serializer(),
            """
                {
                  "id": "node-3",
                  "workflow_id": "wf-1",
                  "issue_id": "issue-3",
                  "checkpointAt": "2026-09-19 12:00:00+00",
                  "after_node_ids": $afterNodeIds,
                  "created_at": "2026-09-19 10:00:00+00",
                  "updated_at": "2026-09-19 10:00:00+00"
                }
            """.trimIndent(),
        )
        assertEquals(listOf("node-1"), nodeWith("""["node-1"]""").afterNodeIds)
        assertEquals(listOf("node-1", "node-2"), nodeWith("\"[\\\"node-1\\\",\\\"node-2\\\"]\"").afterNodeIds)
        assertTrue(nodeWith("null").afterNodeIds.isEmpty())
        assertTrue(nodeWith("\"not an array\"").afterNodeIds.isEmpty())
        // The camelCase tRPC twin of the contract stamp.
        assertEquals("2026-09-19 12:00:00+00", nodeWith("[]").checkpointAt)
    }
}
