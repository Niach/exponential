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
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-916: the fixture is the contract. Every client replays
 * `fixtures/feed/edit-cards.json` through ITS OWN feed-row projection (web
 * `groupFeedRows`, desktop `group_feed_rows`, iOS `AgentFeedRow.rows`, Android
 * [groupFeedRows] — see `ui/session/AgentFeedTest`) plus its [EditCard.editCard],
 * with THESE test names. This file replays it through a REFERENCE projection
 * that knows only the item kinds the fixture uses, so the grouping rule is
 * spelled out once, here.
 *
 * A case: `feed`, an optional `start` (the render window's first index,
 * EXP-783), an optional `lane` (project THAT subagent's items instead of the
 * main lane), an optional `live` (the id [liveToolRowId] would name), and
 * `expected` = one string per row:
 *   `narration@id` · `user@id` · `tool@id` (a lone non-edit tool, or a
 *   workflow call) · `run@id[ids]` (a tool run) · `subagent@id(lane)` ·
 *   `card@id[ids]: <renderEditCard>`.
 *
 * The wire's `workflowId` has no field of its own on Android: a workflow call
 * is named by the CARD's id, which is the call's own `callId` — so a fixture
 * item carrying one becomes a tool whose `callId` is that id, with the id in
 * the screen's [EditCardFixtureCase.workflowIds] set.
 */
internal data class EditCardFixtureCase(
    val name: String,
    val feed: List<AgentFeedItem>,
    val workflowIds: Set<String>,
    val start: Int,
    val lane: String?,
    val live: Long?,
    val expected: List<String>,
)

/** `fixtures/feed/edit-cards.json`, the ONE source both replays read. */
internal fun editCardFixtureCases(): List<EditCardFixtureCase> =
    Json.parseToJsonElement(editCardFixtureJson()).jsonArray.map { element ->
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
                        name = "Edit",
                        detail = text("detail"),
                        subagentId = subagentId,
                        callId = workflowId,
                        toolKind = text("toolKind"),
                        settled = flag("settled"),
                        failed = flag("failed"),
                        diff = text("diff"),
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
                else -> error("unknown fixture kind $kind")
            }
        }
        EditCardFixtureCase(
            name = case.getValue("name").jsonPrimitive.content,
            feed = feed,
            workflowIds = workflowIds,
            start = case["start"]?.takeUnless { it is JsonNull }?.jsonPrimitive?.int ?: 0,
            lane = case["lane"]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content,
            live = case["live"]?.takeUnless { it is JsonNull }?.jsonPrimitive?.long,
            expected = case.getValue("expected").jsonArray.map { it.jsonPrimitive.content },
        )
    }

class EditCardTest {

    /** The reference projection — the fixture's own grouping rule. */
    private fun project(case: EditCardFixtureCase): List<String> {
        val lane = case.lane
        val feed = if (lane == null) case.feed else case.feed.filter { it.lane() == lane }
        val rows = mutableListOf<String>()
        val seenLanes = mutableSetOf<String>()
        var i = case.start
        while (i < feed.size) {
            val row = feed[i]
            val rowLane = row.lane()
            if (lane == null && rowLane != null) {
                if (seenLanes.add(rowLane)) rows.add("subagent@${row.id}($rowLane)")
                i++
                continue
            }
            if (EditCard.isEditCall(row, case.workflowIds)) {
                val end = EditCard.editRunEnd(feed, i, case.workflowIds)
                val members = feed.subList(i, end + 1).map { it as AgentFeedItem.Tool }
                val view = EditCard.editCard(members, case.live)
                // EXP-938: a run whose every member is dropped is no card.
                if (view.rows.isNotEmpty()) rows.add(
                    "card@${row.id}[${members.joinToString(",") { it.id.toString() }}]: " +
                        EditCard.renderEditCard(view),
                )
                i = end + 1
                continue
            }
            if (row is AgentFeedItem.Tool && row.callId !in case.workflowIds) {
                var end = i
                while (
                    end + 1 < feed.size &&
                    feed[end + 1].let { next ->
                        next is AgentFeedItem.Tool &&
                            next.callId !in case.workflowIds &&
                            next.subagentId == lane &&
                            !EditCard.isEditCall(next, case.workflowIds)
                    }
                ) {
                    end++
                }
                rows.add(
                    if (end == i) {
                        "tool@${row.id}"
                    } else {
                        "run@${row.id}[${feed.subList(i, end + 1).joinToString(",") { it.id.toString() }}]"
                    },
                )
                i = end + 1
                continue
            }
            rows.add(
                when (row) {
                    is AgentFeedItem.Tool -> "tool@${row.id}"
                    is AgentFeedItem.UserMessage -> "user@${row.id}"
                    else -> "narration@${row.id}"
                },
            )
            i++
        }
        return rows
    }

    @Test
    fun `every fixture case projects byte exact`() {
        for (case in editCardFixtureCases()) {
            assertEquals(case.name, case.expected, project(case))
        }
    }

    @Test
    fun `the fixture covers a split, a live row, a stub, a lane and a window`() {
        val names = editCardFixtureCases().joinToString("\n") { it.name }
        assertTrue(names.contains("splits"))
        assertTrue(names.contains("live"))
        assertTrue(names.contains("failed"))
        assertTrue(names.contains("done"))
        assertTrue(names.contains("truncated"))
        assertTrue(names.contains("subagent"))
        assertTrue(names.contains("window"))
    }

    @Test
    fun `the rule reads only kind, toolKind and workflowId`() {
        fun tool(kind: String?, callId: String? = null) =
            AgentFeedItem.Tool(id = 1, name = "Edit", detail = null, callId = callId, toolKind = kind)
        assertTrue(EditCard.isEditCall(tool("edit")))
        assertTrue(EditCard.isEditCall(tool("delete")))
        assertTrue(EditCard.isEditCall(tool("move")))
        assertFalse(EditCard.isEditCall(tool("read")))
        assertFalse(EditCard.isEditCall(tool(null)))
        assertFalse(EditCard.isEditCall(tool("edit", callId = "w"), setOf("w")))
        // A workflow id this screen holds no card for is not a workflow call.
        assertTrue(EditCard.isEditCall(tool("edit", callId = "w")))
        assertFalse(EditCard.isEditCall(AgentFeedItem.Narration(id = 1, text = "x")))
    }

    @Test
    fun `the copy is the contract's`() {
        assertEquals("1 file edited", EditCard.editCardTitle(1))
        assertEquals("4 files edited", EditCard.editCardTitle(4))
        assertEquals(5, EditCard.PREVIEW)
        assertNull(EditCard.editCardMoreLabel(5))
        assertEquals("3 more", EditCard.editCardMoreLabel(8))
        // The list is the GENERATED contract constant, never restated here.
        assertEquals(DomainContract.toolKindEditValues, EditCard.KINDS)
    }

    @Test
    fun `a live row is only the card's LAST member`() {
        val items = listOf(
            AgentFeedItem.Tool(id = 1, name = "Edit", detail = "a.ts", toolKind = "edit"),
            AgentFeedItem.Tool(id = 2, name = "Edit", detail = "b.ts", toolKind = "edit"),
        )
        assertNull(EditCard.editCard(items, 1).liveIndex)
        assertEquals(1, EditCard.editCard(items, 2).liveIndex)
        assertNull(EditCard.editCard(items, null).liveIndex)
    }
}

/** The lane an item belongs to — null is the main transcript. */
internal fun AgentFeedItem.lane(): String? = when (this) {
    is AgentFeedItem.Tool -> subagentId
    is AgentFeedItem.Narration -> subagentId
    is AgentFeedItem.UserMessage -> subagentId
    else -> null
}

private fun editCardFixtureJson(): String {
    val candidates = listOf(
        "../../../packages/domain-contract/fixtures/feed/edit-cards.json",
        "../../packages/domain-contract/fixtures/feed/edit-cards.json",
        "packages/domain-contract/fixtures/feed/edit-cards.json",
    )
    val file = candidates.map(::JavaFile).firstOrNull { it.isFile }
        ?: error("edit-cards.json not found from ${JavaFile(".").absolutePath}")
    return file.readText()
}
