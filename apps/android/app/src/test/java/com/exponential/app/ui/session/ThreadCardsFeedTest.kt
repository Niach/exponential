package com.exponential.app.ui.session

import com.exponential.app.domain.ActivityFeedState
import com.exponential.app.domain.AgentFeedItem
import com.exponential.app.domain.AgentFeedRow
import com.exponential.app.domain.ScriptCards
import com.exponential.app.domain.WorkCardLaneTarget
import com.exponential.app.domain.WorkCardRow
import com.exponential.app.domain.WorkflowAgent
import com.exponential.app.domain.WorkflowPhase
import com.exponential.app.domain.WorkflowState
import com.exponential.app.domain.applyActivityEvent
import com.exponential.app.domain.foldedPhases
import com.exponential.app.domain.groupFeedRows
import com.exponential.app.domain.isScriptCall
import com.exponential.app.domain.laneAgent
import com.exponential.app.domain.scriptCardInput
import com.exponential.app.domain.workCard
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1259 D12-D14 on the Android feed: the settling `tool_update.script`
 * decodes tolerantly onto its tool row (STEER-WIRE §1b), a script call never
 * joins a generic tool fold, a settled comment call names its comment and
 * issue, and a workflow's card maps its lanes back to the agents.
 */
class ThreadCardsFeedTest {

    private fun event(raw: String): JsonObject = Json.parseToJsonElement(raw).jsonObject

    private fun ActivityFeedState.applying(raw: String) = applyActivityEvent(event(raw))

    @Test
    fun `a script settle carries its log onto the row and the card reads it`() {
        val state = ActivityFeedState()
            .applying("""{"kind":"tool","name":"mcp__codemode__exec","id":"s1","toolKind":"script"}""")
            .applying(
                """{"kind":"tool_update","id":"s1","status":"completed","at":1700000000000,
                "script":{"count":3,"elapsedMs":3040,"source":"await 1",
                "calls":[{"tool":"exponential.exponential_issues_get","subject":"EXP-1","ok":true,"ms":4},
                {"tool":"exponential.exponential_issues_get","subject":"EXP-2","ok":true},
                {"subject":"no tool"},
                {"tool":"linear.list_issues","ok":false}]}}""",
            )
        val row = state.feed.single() as AgentFeedItem.Tool
        assertTrue(row.isScriptCall)
        assertTrue(row.settled)
        assertEquals(1_700_000_000_000L, row.settledAt)
        val log = row.script!!
        assertEquals(3, log.count)
        assertEquals(3040L, log.elapsedMs)
        assertEquals("await 1", log.source)
        // The malformed call drops, never the log.
        assertEquals(3, log.calls.size)
        assertFalse(log.calls.last().ok)
        val card = ScriptCards.of(row.scriptCardInput())
        assertEquals("Ran a script", card.title)
        assertEquals("3 calls · 3.0 s", card.meta)
        assertEquals(listOf("Read issue", "linear.list_issues"), card.groups.map { it.label })
        assertEquals("1 failed", card.groups.last().failed)
    }

    @Test
    fun `a script call never joins a generic tool fold`() {
        val state = ActivityFeedState()
            .applying("""{"kind":"tool","name":"Bash","id":"t1","toolKind":"execute"}""")
            .applying("""{"kind":"tool","name":"mcp__codemode__exec","id":"s1"}""")
            .applying("""{"kind":"tool","name":"Bash","id":"t2","toolKind":"execute"}""")
            .applying("""{"kind":"tool","name":"Bash","id":"t3","toolKind":"execute"}""")
        val rows = groupFeedRows(state.feed)
        assertEquals(3, rows.size)
        assertTrue(rows[0] is AgentFeedRow.Single)
        assertTrue(((rows[1] as AgentFeedRow.Single).item as AgentFeedItem.Tool).isScriptCall)
        assertEquals(2, (rows[2] as AgentFeedRow.ToolRun).items.size)
    }

    @Test
    fun `an unsettled script is the running title alone`() {
        val state = ActivityFeedState()
            .applying("""{"kind":"tool","name":"mcp.codemode.exec","id":"s1"}""")
        val row = state.feed.single() as AgentFeedItem.Tool
        assertTrue(row.isScriptCall)
        assertNull(row.script)
        assertEquals("Running a script", ScriptCards.of(row.scriptCardInput()).title)
    }

    @Test
    fun `a settled comment call names its comment and its issue`() {
        val state = ActivityFeedState()
            .applying(
                """{"kind":"tool","name":"mcp__exponential__exponential_comments_create","id":"c1","detail":"EXP-9"}""",
            )
            .applying(
                """{"kind":"tool_update","id":"c1","status":"completed",
                "preview":{"refs":[{"kind":"comment","id":"cm-1","title":"Status"},
                {"kind":"issue","id":"is-1","identifier":"VAPP-84"}]}}""",
            )
        val row = state.feed.single() as AgentFeedItem.Tool
        assertTrue(isCommentCallCard(row))
        val target = commentCallTarget(row)
        assertEquals("cm-1", target.commentId)
        assertEquals("Status", target.commentText)
        assertEquals("VAPP-84", target.issueKey)
        // Running or failed, it keeps its ordinary row.
        assertFalse(isCommentCallCard(row.copy(settled = false)))
        assertFalse(isCommentCallCard(row.copy(failed = true)))
    }

    @Test
    fun `a workflow card maps a lane back to its agent`() {
        val workflow = WorkflowState(
            id = "wf",
            name = "build",
            status = "running",
            phases = listOf(WorkflowPhase(1, "Plan"), WorkflowPhase(2, "Build")),
            agents = listOf(
                WorkflowAgent(1, "plan", phaseIndex = 1, agentId = "a1", state = "done", durationMs = 60_000),
                WorkflowAgent(2, "web", phaseIndex = 2, agentId = "a2", state = "running"),
                WorkflowAgent(3, "ios", phaseIndex = 2, agentId = "a3", state = "done", durationMs = 5_000),
            ),
        )
        val card = workflow.workCard()
        assertEquals("2 / 3", card.progress)
        assertEquals("1m", card.elapsed)
        assertTrue(card.rows.first() is WorkCardRow.Folded)
        assertEquals("a3", workflow.laneAgent(WorkCardLaneTarget(1, 1))?.agentId)
        val folded = workflow.foldedPhases(card)
        assertEquals(listOf("Plan"), folded.map { it.title })
        assertEquals("a1", workflow.laneAgent(WorkCardLaneTarget(0, 0))?.agentId)
    }
}
