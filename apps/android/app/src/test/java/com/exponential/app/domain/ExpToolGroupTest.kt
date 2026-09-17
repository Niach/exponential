package com.exponential.app.domain

import java.io.File as JavaFile
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-948: the fixture is the contract. Every client replays
 * `fixtures/feed/exp-tool-groups.json` through ITS OWN feed-row projection (web
 * `groupFeedRows`, desktop `group_feed_rows`, iOS `AgentFeedRow.rows`, Android
 * the REAL [groupFeedRows] / [projectLaneRows] below) with THESE test names, so
 * the rule that keeps our own MCP calls visible cannot drift between the
 * contract and what a screen actually renders.
 *
 * A case: `feed`, an optional `start` (the render window's first index,
 * EXP-783), an optional `lane` (project THAT subagent's items instead of the
 * main lane), and `expected` = one string per row:
 *   `narration@id` · `user@id` · `tool@id` (a lone tool call, ours or not) ·
 *   `run@id[ids]` (a generic tool run) · `expRun@id[ids]: <caption>` (a run of
 *   OURS) · `subagent@id(lane)`.
 *
 * The wire's `workflowId` has no field of its own on Android: a workflow call
 * is named by the CARD's id, which is the call's own `callId` — so a fixture
 * item carrying one becomes a tool whose `callId` is that id, with the id in
 * the screen's workflow set (and the screen's own [splitWorkflowToolRows] is
 * replayed here for the same reason).
 */
private data class ExpToolGroupCase(
    val name: String,
    val feed: List<AgentFeedItem>,
    val workflowIds: Set<String>,
    val start: Int,
    val lane: String?,
    val expected: List<String>,
)

class ExpToolGroupTest {

    /** The REAL projection — exactly what the session screen renders. */
    private fun realRows(case: ExpToolGroupCase): List<String> {
        val rows = if (case.lane != null) {
            projectLaneRows(case.feed.filter { it.projectionLane() == case.lane }, case.workflowIds)
        } else {
            splitWorkflowToolRows(
                groupFeedRows(case.feed, case.start, case.workflowIds),
                case.workflowIds,
            )
        }
        return rows.map { row ->
            when (row) {
                is AgentFeedRow.ExpRun -> "expRun@${row.id}${ids(row.items)}: " +
                    ExpToolGroup.expToolGroupCaption(row.items)
                is AgentFeedRow.ToolRun -> "run@${row.id}${ids(row.items)}"
                is AgentFeedRow.Edits -> "card@${row.id}${ids(row.items)}"
                is AgentFeedRow.SubagentRun -> "subagent@${row.id}(${row.subagentId})"
                is AgentFeedRow.QuestionStepper -> "ask@${row.id}"
                is AgentFeedRow.Single -> when (val item = row.item) {
                    is AgentFeedItem.Tool -> "tool@${item.id}"
                    is AgentFeedItem.UserMessage -> "user@${item.id}"
                    is AgentFeedItem.Subagent -> "subagent@${item.id}"
                    else -> "narration@${item.id}"
                }
            }
        }
    }

    private fun ids(items: List<AgentFeedItem>) =
        "[${items.joinToString(",") { it.id.toString() }}]"

    @Test
    fun `every fixture case projects byte exact through the real projection`() {
        val cases = expToolGroupCases()
        assertTrue(cases.size >= 14)
        for (case in cases) {
            assertEquals(case.name, case.expected, realRows(case))
        }
    }

    @Test
    fun `the fixture covers a single call, a break, a lane, a workflow and a window`() {
        val names = expToolGroupCases().joinToString("\n") { it.name }
        assertTrue(names.contains("single"))
        assertTrue(names.contains("break"))
        assertTrue(names.contains("failed"))
        assertTrue(names.contains("running"))
        assertTrue(names.contains("subagent"))
        assertTrue(names.contains("workflow"))
        assertTrue(names.contains("window"))
        assertTrue(names.contains("namespace"))
    }

    @Test
    fun `the rule reads only kind, name and the workflow set`() {
        fun tool(name: String, callId: String? = null) =
            AgentFeedItem.Tool(id = 1, name = name, detail = null, callId = callId)
        assertTrue(ExpToolGroup.isExpToolCall(tool("exponential_issues_get")))
        assertTrue(ExpToolGroup.isExpToolCall(tool("mcp__exponential__exponential_pr_open")))
        // Another server's same-named tool is not ours — the contract prefix
        // has to sit right in front of the row name.
        assertFalse(ExpToolGroup.isExpToolCall(tool("mcp__linear__issues_get")))
        assertFalse(ExpToolGroup.isExpToolCall(tool("Bash")))
        // A workflow card's own call is its card, never a group member.
        assertFalse(
            ExpToolGroup.isExpToolCall(tool("exponential_issues_get", callId = "w"), setOf("w")),
        )
        // A workflow id this screen holds no card for is not a workflow call.
        assertTrue(ExpToolGroup.isExpToolCall(tool("exponential_issues_get", callId = "w")))
        assertFalse(ExpToolGroup.isExpToolCall(AgentFeedItem.Narration(id = 1, text = "x")))
    }

    @Test
    fun `the caption is the contract's plural copy`() {
        fun call(id: Long, settled: Boolean = true, failed: Boolean = false) = AgentFeedItem.Tool(
            id = id,
            name = "exponential_issues_get",
            detail = null,
            settled = settled,
            failed = failed,
        )
        assertEquals(
            "Read 2 issues",
            ExpToolGroup.expToolGroupCaption(listOf(call(1), call(2))),
        )
        assertEquals(
            "Reading 2 issues",
            ExpToolGroup.expToolGroupCaption(listOf(call(1), call(2, settled = false))),
        )
        assertEquals(
            "Read 2 issues · 1 failed",
            ExpToolGroup.expToolGroupCaption(listOf(call(1), call(2, failed = true))),
        )
        // The strings are the GENERATED contract's, never restated here.
        val at = DomainContract.expToolNames.indexOf("issues_get")
        assertEquals("Reading {n} issues", DomainContract.expToolProgressiveMany[at])
        assertEquals("Read {n} issues", DomainContract.expToolDoneMany[at])
        assertEquals("", ExpToolGroup.expToolGroupCaption(emptyList()))
    }

    @Test
    fun `a call of ours never joins a plain tool run`() {
        val ours = AgentFeedItem.Tool(2, "exponential_issues_get", null)
        val rows = groupFeedRows(listOf(AgentFeedItem.Tool(1, "Bash", "ls"), ours, AgentFeedItem.Tool(3, "Bash", "ls")))
        assertEquals(listOf(1L, 2L, 3L), rows.map { it.id })
        assertEquals(AgentRowClass.Tool, rows[1].rowClass)
        // Two neighbours of the SAME tool are one run, and its id never moves.
        val pair = groupFeedRows(listOf(ours, ours.copy(id = 3)))
        assertEquals(1, pair.size)
        assertEquals(2L, pair.single().id)
        assertEquals(listOf(2L, 3L), (pair.single() as AgentFeedRow.ExpRun).items.map { it.id })
        assertEquals(AgentRowClass.Tool, pair.single().rowClass)
        // Two DIFFERENT tools of ours are two rows.
        val mixed = groupFeedRows(listOf(ours, ours.copy(id = 3, name = "exponential_issues_list")))
        assertEquals(listOf(2L, 3L), mixed.map { it.id })
    }

    /** `fixtures/feed/exp-tool-groups.json`, the ONE source every client reads. */
    private fun expToolGroupCases(): List<ExpToolGroupCase> =
        Json.parseToJsonElement(expToolGroupFixtureJson()).jsonArray.map { element ->
            val case = element.jsonObject
            val workflowIds = mutableSetOf<String>()
            val feed = case.getValue("feed").jsonArray.map { row ->
                val item = row.jsonObject
                fun text(key: String): String? =
                    item[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content
                fun flag(key: String): Boolean =
                    item[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.boolean ?: false
                val id = item.getValue("id").jsonPrimitive.long
                val subagentId = text("subagentId")
                when (val kind = item.getValue("kind").jsonPrimitive.content) {
                    "tool" -> {
                        val workflowId = text("workflowId")
                        if (workflowId != null) workflowIds.add(workflowId)
                        AgentFeedItem.Tool(
                            id = id,
                            name = text("name").orEmpty(),
                            detail = text("detail"),
                            subagentId = subagentId,
                            callId = workflowId,
                            toolKind = text("toolKind"),
                            settled = flag("settled"),
                            failed = flag("failed"),
                        )
                    }
                    "narration" -> AgentFeedItem.Narration(
                        id = id,
                        text = text("text").orEmpty(),
                        subagentId = subagentId,
                    )
                    "user_message" -> AgentFeedItem.UserMessage(
                        id = id,
                        text = text("text").orEmpty(),
                        subagentId = subagentId,
                    )
                    "subagent" -> AgentFeedItem.Subagent(
                        id = id,
                        subagentId = subagentId.orEmpty(),
                        agentType = text("agentType").orEmpty(),
                        completed = flag("completed"),
                    )
                    else -> error("unknown fixture kind $kind")
                }
            }
            ExpToolGroupCase(
                name = case.getValue("name").jsonPrimitive.content,
                feed = feed,
                workflowIds = workflowIds,
                start = case["start"]?.takeUnless { it is JsonNull }?.jsonPrimitive?.int ?: 0,
                lane = case["lane"]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content,
                expected = case.getValue("expected").jsonArray.map { it.jsonPrimitive.content },
            )
        }
}

/** The lane a row RENDERS in — unlike the test helper `lane()` this counts a
 *  subagent's own lifecycle edge, which IS its lane's header. */
private fun AgentFeedItem.projectionLane(): String? = when (this) {
    is AgentFeedItem.Subagent -> subagentId
    is AgentFeedItem.Tool -> subagentId
    is AgentFeedItem.Narration -> subagentId
    is AgentFeedItem.UserMessage -> subagentId
    else -> null
}

private fun expToolGroupFixtureJson(): String {
    val candidates = listOf(
        "../../../packages/domain-contract/fixtures/feed/exp-tool-groups.json",
        "../../packages/domain-contract/fixtures/feed/exp-tool-groups.json",
        "packages/domain-contract/fixtures/feed/exp-tool-groups.json",
    )
    val file = candidates.map(::JavaFile).firstOrNull { it.isFile }
        ?: error("exp-tool-groups.json not found from ${JavaFile(".").absolutePath}")
    return file.readText()
}
