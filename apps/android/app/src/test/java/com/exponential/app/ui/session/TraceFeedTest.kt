package com.exponential.app.ui.session

import com.exponential.app.domain.ActivityFeedState
import com.exponential.app.domain.AgentFeedItem
import com.exponential.app.domain.AgentFeedRow
import com.exponential.app.domain.applyActivityEvent
import com.exponential.app.domain.durationMs
import com.exponential.app.domain.groupFeedRows
import com.exponential.app.domain.openWaits
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** EXP-1324: the two wire additions the Trace + Queue read — `thought` edges
 *  and `background_tasks[].startedAt` — plus the subagent's `at` stamps. */
class TraceFeedTest {

    private fun ActivityFeedState.applying(raw: String) =
        applyActivityEvent(Json.parseToJsonElement(raw).jsonObject as JsonObject)

    @Test
    fun `a thought opens running and its ended edge patches the same row`() {
        val started = ActivityFeedState().applying("""{"kind":"thought","id":"t1","state":"started","at":1}""")
        val open = started.feed.single() as AgentFeedItem.Thought
        assertEquals("t1", open.thoughtId)
        assertFalse(open.ended)
        val ended = started.applying("""{"kind":"thought","id":"t1","state":"ended","durationMs":5400}""")
        val row = ended.feed.single() as AgentFeedItem.Thought
        assertTrue(row.ended)
        assertEquals(5400L, row.durationMs)
        assertEquals(open.id, row.id)
    }

    @Test
    fun `an unknown ended id and an id-less frame are dropped`() {
        val state = ActivityFeedState()
            .applying("""{"kind":"thought","id":"nope","state":"ended","durationMs":10}""")
            .applying("""{"kind":"thought","state":"started"}""")
        assertTrue(state.feed.isEmpty())
    }

    @Test
    fun `a turn end or the next thought settles an open one without a duration`() {
        val turnEnded = ActivityFeedState()
            .applying("""{"kind":"thought","id":"t1","state":"started"}""")
            .applying("""{"kind":"turn","state":"ended"}""")
        val settled = turnEnded.feed.single() as AgentFeedItem.Thought
        assertTrue(settled.ended)
        assertNull(settled.durationMs)

        val next = ActivityFeedState()
            .applying("""{"kind":"thought","id":"t1","state":"started"}""")
            .applying("""{"kind":"thought","id":"t2","state":"started"}""")
        val (first, second) = next.feed.map { it as AgentFeedItem.Thought }
        assertTrue(first.ended)
        assertFalse(second.ended)
    }

    @Test
    fun `a thought started in another lane leaves the others' open thoughts running`() {
        // Parallel subagents each reason in their own lane: s2's `started`
        // settles neither the main run's thought nor s1's; only s1's NEXT
        // thought settles s1's, and the turn's end settles every lane.
        val state = ActivityFeedState()
            .applying("""{"kind":"thought","id":"m1","state":"started"}""")
            .applying("""{"kind":"thought","id":"a1","state":"started","subagentId":"s1"}""")
            .applying("""{"kind":"thought","id":"b1","state":"started","subagentId":"s2"}""")
        fun open(s: ActivityFeedState) =
            s.feed.filterIsInstance<AgentFeedItem.Thought>().filter { !it.ended }.map { it.thoughtId }
        assertEquals(listOf("m1", "a1", "b1"), open(state))

        val sameLane = state.applying("""{"kind":"thought","id":"a2","state":"started","subagentId":"s1"}""")
        assertEquals(listOf("m1", "b1", "a2"), open(sameLane))

        val turnEnded = sameLane.applying("""{"kind":"turn","state":"ended"}""")
        assertEquals(emptyList<String>(), open(turnEnded))
    }

    @Test
    fun `a subagent's thought rides its lane`() {
        val state = ActivityFeedState()
            .applying("""{"kind":"subagent","id":"s1","agentType":"Explore","status":"started","at":1000}""")
            .applying("""{"kind":"thought","id":"t1","state":"started","subagentId":"s1"}""")
        val run = groupFeedRows(state.feed).single() as AgentFeedRow.SubagentRun
        assertTrue(run.items.single() is AgentFeedItem.Thought)
    }

    @Test
    fun `a subagent's duration comes off its edges' at stamps`() {
        val started = ActivityFeedState()
            .applying("""{"kind":"subagent","id":"s1","agentType":"Explore","status":"started","at":1000}""")
        val running = groupFeedRows(started.feed).single() as AgentFeedRow.SubagentRun
        assertEquals(31_000L, running.durationMs(nowMs = 32_000))
        val done = started.applying("""{"kind":"subagent","id":"s1","status":"completed","at":73000}""")
        val run = groupFeedRows(done.feed).single() as AgentFeedRow.SubagentRun
        assertEquals(72_000L, run.durationMs(nowMs = 999_999))
        val bare = ActivityFeedState()
            .applying("""{"kind":"subagent","id":"s2","agentType":"Explore","status":"started"}""")
        assertNull((groupFeedRows(bare.feed).single() as AgentFeedRow.SubagentRun).durationMs(5))
    }

    @Test
    fun `a background task keeps its wire start across republishes`() {
        val first = ActivityFeedState().applying(
            """{"kind":"background_tasks","tasks":[{"id":"b1","kind":"shell","description":"Captures","startedAt":1792000000000}]}""",
        )
        assertEquals(1792000000000L, first.backgroundTasks.single().startedAt)
        val again = first.applying(
            """{"kind":"background_tasks","tasks":[{"id":"b1","kind":"shell","description":"Captures"}]}""",
        )
        assertEquals(1792000000000L, again.backgroundTasks.single().startedAt)
        val older = ActivityFeedState().applying(
            """{"kind":"background_tasks","tasks":[{"id":"b2","kind":"shell","description":"Logs"}]}""",
        )
        assertNull(older.backgroundTasks.single().startedAt)
    }

    @Test
    fun `open waits name the call and its subject`() {
        val state = ActivityFeedState()
            .applying("""{"kind":"tool","id":"c1","name":"TaskOutput","toolKind":"wait","detail":"the web captures"}""")
            .applying("""{"kind":"tool","id":"c2","name":"Monitor","toolKind":"wait"}""")
        val waits = openWaits(state.feed)
        assertEquals(listOf("c1" to "the web captures", "c2" to "Monitor"), waits.map { it.id to it.subject })
        val settled = state.applying("""{"kind":"tool_update","id":"c1","status":"completed"}""")
        assertEquals(listOf("c2"), openWaits(settled.feed).map { it.id })
    }
}
