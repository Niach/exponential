package com.exponential.app.ui.session

import com.exponential.app.domain.ActivityFeedState
import com.exponential.app.domain.applyActivityEvent
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertSame
import org.junit.Test

/**
 * EXP-1521: a working agent re-sends its `turn` slot every couple of seconds
 * (the clock, the tokens, "Writing…"). The composer and the Dock read their
 * own slices of the live state, and a tick must leave both slices EQUAL, so
 * `distinctUntilChanged` drops it and neither the field nor the Dock
 * recomposes. Read whole, every tick recomposed the composer mid-word.
 */
class ComposerActivityTickTest {
    private var seq = 0L

    private fun ActivityFeedState.apply(json: String): ActivityFeedState =
        applyActivityEvent(Json.parseToJsonElement(json).jsonObject, { false }, seq++)

    private fun working(): ActivityFeedState = ActivityFeedState()
        .apply("""{"kind":"narration","text":"Looking at the queue."}""")
        .apply("""{"kind":"turn","state":"started","startedAt":1000,"tokens":100}""")

    private fun tick(state: ActivityFeedState, i: Int): ActivityFeedState =
        state.apply("""{"kind":"turn","state":"started","startedAt":1000,"tokens":${100 + i * 120},"writing":{"tool":"Write","startedAt":2000}}""")

    @Test
    fun aTurnTickChangesNeitherTheComposerNorTheDockSlice() {
        var state = working()
        val composer = composerActivityOf(state)
        val dock = dockActivityOf(state)
        repeat(30) { i ->
            val next = tick(state, i + 1)
            // The live state did change (the working row reads it)...
            assertNotEquals(state, next)
            // ...but not what the composer or the Dock read.
            assertEquals(composer, composerActivityOf(next))
            assertEquals(dock, dockActivityOf(next))
            // The feed list itself is untouched by a tick (its identity holds).
            assertSame(state.feed, next.feed)
            state = next
        }
    }

    @Test
    fun whatTheComposerReadsStillGetsThrough() {
        val state = working()
        val asking = state.apply(
            """{"kind":"question","id":"q1","text":"Ship it?","options":[{"label":"Yes","key":"1"},{"label":"No","key":"2"}]}""",
        )
        assertEquals(false, composerActivityOf(state).asking)
        assertEquals(true, composerActivityOf(asking).asking)

        val compacting = state.apply("""{"kind":"compaction","phase":"started","trigger":"manual"}""")
        assertEquals(true, composerActivityOf(compacting).compacting)

        val ended = state.apply("""{"kind":"turn","state":"ended"}""")
        assertNotEquals(composerActivityOf(state), composerActivityOf(ended))
    }
}
