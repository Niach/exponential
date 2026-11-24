package com.exponential.app.ui.session

import com.exponential.app.domain.AgentFeedItem
import com.exponential.app.domain.AgentFeedRow
import com.exponential.app.domain.ChatExpress
import com.exponential.app.ui.chat.feedExpressRowKey
import com.exponential.app.ui.styleguide.EXPRESS_SAMPLE_TEXT
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-2074 / #EXP-2076 (chat-express.json): a subagent lane's narration is
 * FINAL by the main lane's rule per lane, keyed like the main lane's; every
 * read-only place reads the sample's pure prose.
 */
class ExpressLaneTest {
    @Test
    fun aLaneRowIsFinalOnceARowOfItsLaneFollows() {
        // The last row of a lane still streaming (the turn runs, the subagent is not done).
        assertFalse(laneNarrationFinal(index = 2, count = 3, working = true, done = false))
        // Another row of its own lane follows it.
        assertTrue(laneNarrationFinal(index = 1, count = 3, working = true, done = false))
    }

    @Test
    fun aLaneRowIsFinalOnceItsTurnEndsOrItsSubagentIsDone() {
        assertTrue(laneNarrationFinal(index = 2, count = 3, working = false, done = false))
        assertTrue(laneNarrationFinal(index = 2, count = 3, working = true, done = true))
    }

    @Test
    fun aSubagentNarrationIsKeyedLikeTheMainLane() {
        assertEquals("n:s1:msg_sa1", feedExpressRowKey("s1", "msg_sa1", 11L))
        assertEquals("e:s1:11", feedExpressRowKey("s1", null, 11L))
        assertEquals("m:m9", feedExpressRowKey("s1", "synced:m9", 11L))
    }

    @Test
    fun theLaneKeepsANarrationWithABlock() {
        val narration = AgentFeedItem.Narration(11L, EXPRESS_SAMPLE_TEXT, messageId = "msg_sa1", subagentId = "sa1", seq = 11L)
        val run = AgentFeedRow.SubagentRun(
            id = 1L,
            subagentId = "sa1",
            agentType = "Explore",
            completed = true,
            detail = null,
            items = listOf(narration),
        )
        val rows = subagentLaneRows(run, emptySet())
        assertEquals(1, rows.size)
        assertEquals(narration, (rows[0] as AgentFeedRow.Single).item)
    }

    @Test
    fun theSampleReadsAsItsProse() {
        // The caption, the call card and the summary band read this, never the block.
        assertEquals("Checked the three lanes.\n\nAll green.", ChatExpress.plainProse(EXPRESS_SAMPLE_TEXT))
        assertEquals("Ship 0.14.140 tomorrow.", ChatExpress.plainProse("Ship 0.14.140 tomorrow."))
    }
}
