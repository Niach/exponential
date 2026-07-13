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

    private fun question(id: Long) = AgentFeedItem.Question(
        id = id,
        text = "Which color?",
        options = listOf(QuestionOption("Red", "1"), QuestionOption("Blue", "2")),
        multiSelect = false,
    )
}
