package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * EXP-788: the composer is the free-text path of a pending card. These pin
 * WHICH card a typed message answers, on WHICH key, and when it stays a plain
 * steer — the one decision the send path and the placeholder share.
 */
class ComposerAnswerRoutingTest {

    private fun plan(id: Long, wireId: String? = "p$id") = AgentFeedItem.Question(
        id = id,
        text = "## Plan",
        options = listOf(
            QuestionOption("Yes", "1"),
            QuestionOption("Yes, and start with a fresh context", "2"),
            QuestionOption("No, keep planning", "3", "Sends your next message back to planning"),
        ),
        multiSelect = false,
        planMode = true,
        wireId = wireId,
    )

    private fun question(id: Long, freeText: Boolean = true, wireId: String? = "q$id") =
        AgentFeedItem.Question(
            id = id,
            text = "Which color?",
            options = buildList {
                add(QuestionOption("Red", "1"))
                add(QuestionOption("Blue", "2"))
                if (freeText) add(QuestionOption("Type something.", "text", freeText = true))
            },
            multiSelect = false,
            wireId = wireId,
        )

    private fun narration(id: Long) = AgentFeedItem.Narration(id, "working")

    @Test
    fun aPendingPlanAnswersOnItsRejectOptionWithThePlanPlaceholder() {
        val target = composerAnswerTarget(listOf(narration(1), plan(2)), emptyMap(), "make it smaller")
        assertEquals(listOf("3"), target?.keys)
        assertEquals(2L, target?.question?.id)
        assertEquals("Tell the agent what to change, or pick an option above", target?.placeholder)
    }

    @Test
    fun aPendingQuestionAnswersOnItsFreeTextOptionWithTheQuestionPlaceholder() {
        val target = composerAnswerTarget(listOf(question(1)), emptyMap(), "teal")
        assertEquals(listOf("text"), target?.keys)
        assertEquals("Answer directly, or pick an option above", target?.placeholder)
    }

    @Test
    fun aQuestionWithoutAFreeTextOptionHasNoFreeTextPath() {
        assertNull(composerAnswerTarget(listOf(question(1, freeText = false)), emptyMap(), "teal"))
    }

    @Test
    fun aLockedCardIsNotAnswerableButAFailedLockReopensIt() {
        val feed = listOf(plan(1))
        assertNull(composerAnswerTarget(feed, mapOf("p1" to AnswerState.Sending), "x"))
        assertNull(composerAnswerTarget(feed, mapOf("p1" to AnswerState.Acked), "x"))
        assertEquals(listOf("3"), composerAnswerTarget(feed, mapOf("p1" to AnswerState.Failed), "x")?.keys)
    }

    @Test
    fun aResolvedOrIdLessCardNeverTakesTheMessage() {
        assertNull(composerAnswerTarget(listOf(plan(1).copy(resolved = true)), emptyMap(), "x"))
        assertNull(composerAnswerTarget(listOf(plan(1, wireId = null)), emptyMap(), "x"))
        assertNull(composerAnswerTarget(emptyList(), emptyMap(), "x"))
    }

    @Test
    fun aSlashCommandIsNeverAnAnswer() {
        assertNull(composerAnswerTarget(listOf(plan(1)), emptyMap(), "/clear"))
        assertNull(composerAnswerTarget(listOf(plan(1)), emptyMap(), "  /compact"))
        // The placeholder derivation asks with no text at all.
        assertEquals(listOf("3"), composerAnswerTarget(listOf(plan(1)), emptyMap())?.keys)
    }

    @Test
    fun theNewestOpenCardWins() {
        val feed = listOf(plan(1).copy(resolved = true), question(2), plan(3))
        assertEquals(3L, composerAnswerTarget(feed, emptyMap(), "x")?.question?.id)
        // The plan answered, the question is what still waits.
        assertEquals(
            2L,
            composerAnswerTarget(feed, mapOf("p3" to AnswerState.Acked), "x")?.question?.id,
        )
    }

    @Test
    fun aMultiStepAskAnswersOnItsFirstOpenStepAndNeverOnItsSubmitStep() {
        val step1 = question(1).copy(askId = "ask", index = 1, total = 2, wireId = "s1")
        val step2 = question(2).copy(askId = "ask", index = 2, total = 2, wireId = "s2")
        val submit = question(3, freeText = false).copy(askId = "ask", wireId = "s3")
        val feed = listOf(step1, step2, submit)
        assertEquals("s1", composerAnswerTarget(feed, emptyMap(), "x")?.question?.wireId)
        assertEquals(
            "s2",
            composerAnswerTarget(feed, mapOf("s1" to AnswerState.Acked), "x")?.question?.wireId,
        )
        assertNull(
            composerAnswerTarget(
                feed,
                mapOf("s1" to AnswerState.Acked, "s2" to AnswerState.Acked),
                "x",
            ),
        )
    }
}
