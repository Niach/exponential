package com.exponential.app.ui.session

import org.junit.Assert.assertEquals
import org.junit.Test

// EXP-78: only the TRAILING consecutive run of question items is answerable —
// any later event means the desktop TUI moved on.
class AgentFeedTest {

    @Test
    fun `returns the trailing consecutive question run`() {
        val feed = listOf<AgentFeedItem>(
            AgentFeedItem.Narration(1, "working"),
            question(2),
            AgentFeedItem.Tool(3, "Edit", "src/a.ts"),
            question(4),
            question(5),
        )
        assertEquals(setOf(4L, 5L), trailingQuestionIds(feed))
    }

    @Test
    fun `empty when the feed ends with a non-question`() {
        val feed = listOf(
            question(1),
            AgentFeedItem.Narration(2, "moved on"),
        )
        assertEquals(emptySet<Long>(), trailingQuestionIds(feed))
    }

    @Test
    fun `handles an all-question feed and an empty feed`() {
        assertEquals(setOf(1L, 2L), trailingQuestionIds(listOf(question(1), question(2))))
        assertEquals(emptySet<Long>(), trailingQuestionIds(emptyList()))
    }

    @Test
    fun `trailing questions are unaffected by tool runs before them`() {
        val feed = listOf(tool(1), tool(2), question(3))
        assertEquals(setOf(3L), trailingQuestionIds(feed))
    }

    // EXP-97: consecutive runs of >=2 tool calls collapse into one render row.

    @Test
    fun `collapses runs of two or more consecutive tools, leaves the rest single`() {
        val feed = listOf(
            AgentFeedItem.Narration(1, "working"),
            tool(2),
            tool(3),
            tool(4),
            AgentFeedItem.UserMessage(5, "hi"),
            tool(6),
        )
        assertEquals(
            listOf<AgentFeedRow>(
                AgentFeedRow.Single(feed[0]),
                AgentFeedRow.ToolRun(listOf(tool(2), tool(3), tool(4))),
                AgentFeedRow.Single(feed[4]),
                AgentFeedRow.Single(feed[5]),
            ),
            groupToolRuns(feed),
        )
    }

    @Test
    fun `a lone tool between other kinds stays a single row`() {
        val feed = listOf(tool(1), AgentFeedItem.Narration(2, "x"), tool(3))
        assertEquals(feed.map { AgentFeedRow.Single(it) }, groupToolRuns(feed))
    }

    @Test
    fun `two runs split by a narration stay separate runs`() {
        val feed = listOf(tool(1), tool(2), AgentFeedItem.Narration(3, "x"), tool(4), tool(5))
        assertEquals(
            listOf<AgentFeedRow>(
                AgentFeedRow.ToolRun(listOf(tool(1), tool(2))),
                AgentFeedRow.Single(feed[2]),
                AgentFeedRow.ToolRun(listOf(tool(4), tool(5))),
            ),
            groupToolRuns(feed),
        )
    }

    @Test
    fun `an all-tool feed is one run and an empty feed has no rows`() {
        val feed = listOf(tool(1), tool(2), tool(3))
        assertEquals(listOf<AgentFeedRow>(AgentFeedRow.ToolRun(feed)), groupToolRuns(feed))
        assertEquals(emptyList<AgentFeedRow>(), groupToolRuns(emptyList()))
    }

    @Test
    fun `run id stays the first tool's id as the trailing run grows`() {
        val feed = listOf<AgentFeedItem>(AgentFeedItem.Narration(1, "x"), tool(2), tool(3))
        assertEquals(2L, groupToolRuns(feed)[1].id)
        assertEquals(2L, groupToolRuns(feed + tool(4))[1].id)
    }

    @Test
    fun `questions adjacent to tools are never absorbed into a run`() {
        val feed = listOf(tool(1), tool(2), question(3), question(4))
        assertEquals(
            listOf<AgentFeedRow>(
                AgentFeedRow.ToolRun(listOf(tool(1), tool(2))),
                AgentFeedRow.Single(feed[2]),
                AgentFeedRow.Single(feed[3]),
            ),
            groupToolRuns(feed),
        )
    }

    private fun tool(id: Long) = AgentFeedItem.Tool(id, "Edit", "src/a.ts")

    private fun question(id: Long) = AgentFeedItem.Question(
        id = id,
        text = "Which color?",
        options = listOf(QuestionOption("Red", "1"), QuestionOption("Blue", "2")),
        multiSelect = false,
    )
}
