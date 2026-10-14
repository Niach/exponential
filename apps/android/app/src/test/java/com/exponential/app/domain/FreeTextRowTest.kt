package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Test

/** EXP-1316: every question card offers the free-text answer as option N+1. */
class FreeTextRowTest {

    private fun question(vararg options: QuestionOption, planMode: Boolean = false) = AgentFeedItem.Question(
        id = 1L,
        text = "Which?",
        options = options.toList(),
        multiSelect = false,
        planMode = planMode,
        wireId = "q1",
    )

    @Test
    fun `a question without one gains the row keyed by its position`() {
        val item = question(QuestionOption("A", "1", description = "first"), QuestionOption("B", "2")).withFreeTextRow()
        assertEquals(listOf("1", "2", "3"), item.options.map { it.key })
        assertEquals(FREE_TEXT_ANSWER_ROW, item.options.last().label)
        assertEquals(true, item.options.last().freeText)
        assertEquals("first", item.options.first().description)
    }

    @Test
    fun `the publisher's own row, a plan and a taken key are respected`() {
        val own = question(QuestionOption("A", "1"), QuestionOption("Type something.", "2", freeText = true))
        assertSame(own, own.withFreeTextRow())
        val plan = question(QuestionOption("Yes", "1"), QuestionOption("No", "2"), planMode = true)
        assertSame(plan, plan.withFreeTextRow())
        val clash = question(QuestionOption("A", "2")).withFreeTextRow()
        assertEquals("free-text", clash.options.last().key)
    }
}
