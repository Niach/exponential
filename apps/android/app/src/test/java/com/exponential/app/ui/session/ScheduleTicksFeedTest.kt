package com.exponential.app.ui.session

import com.exponential.app.domain.AgentFeedItem
import com.exponential.app.domain.AgentFeedRow
import org.junit.Assert.assertEquals
import org.junit.Test

/** #FEED-317: the main feed's quiet schedule ticks fold into ONE line,
 *  their work with them, the line keeping each tick's `at`. */
class ScheduleTicksFeedTest {

    private fun tick(id: Long, at: Long) =
        AgentFeedRow.Single(AgentFeedItem.UserMessage(id, "Schedule `Check in` fired", at = at))

    private fun quiet(id: Long) =
        AgentFeedRow.Single(AgentFeedItem.Tool(id, "mcp__exponential__exponential_session_self_quiet", null, settled = true))

    private fun tool(id: Long, name: String = "Bash") =
        AgentFeedRow.Single(AgentFeedItem.Tool(id, name, "ls", settled = true))

    @Test
    fun `quiet ticks with work collapse into one line holding every tick`() {
        val rows = listOf(
            tick(1, 100), tool(2), AgentFeedRow.Single(AgentFeedItem.Thought(3, "t1", ended = true)), quiet(4),
            tick(5, 200), quiet(6),
            tick(7, 300), AgentFeedRow.ToolRun(listOf(tool(8).item as AgentFeedItem.Tool, tool(9).item as AgentFeedItem.Tool)), quiet(10),
        )
        val shown = dropHiddenFeedRows(rows)
        assertEquals(listOf(7L), shown.rows.map { it.id })
        assertEquals(mapOf(7L to listOf<Long?>(100, 200, 300)), shown.checkIns)
    }

    @Test
    fun `a show call keeps the tick a normal turn`() {
        val rows = listOf(tick(1, 100), tool(2, "mcp__exponential__exponential_show"), quiet(3))
        val shown = dropHiddenFeedRows(rows)
        assertEquals(listOf(2L, 3L), shown.rows.map { it.id })
        assertEquals(emptyMap<Long, List<Long?>>(), shown.checkIns)
    }
}
