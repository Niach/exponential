package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-1245: the owner's turn facts off the relay feed — observed turn edges,
// placed messages, and the run's start opening the first turn. Test names
// mirror web `session-turn-events.test.ts`.
class SessionTurnEventsTest {
    private val start = 1_000L

    private fun message(id: Long, text: String, at: Long? = null, subagentId: String? = null) =
        TurnFeedRow(id = id, isUserMessage = true, text = text, subagentId = subagentId, at = at)

    @Test
    fun `yields nothing while only the run's start is known`() {
        val log = emptyTurnLog()
        recordTurnSlot(log, TURN_STATE_ENDED, null, 2_000)
        recordFeedMessages(log, emptyList(), 2_000)
        assertEquals(emptyList<SessionTurnEvent>(), turnEventsOf(log, emptyList(), start))
    }

    @Test
    fun `records a started edge and an ended edge seen to follow it`() {
        val log = emptyTurnLog()
        recordTurnSlot(log, TURN_STATE_STARTED, 5_000, 5_100)
        recordTurnSlot(log, TURN_STATE_STARTED, 5_000, 6_000)
        recordTurnSlot(log, TURN_STATE_ENDED, 5_000, 9_000)
        assertEquals(
            listOf(SessionTurnEvent.Turn(started = true, at = 5_000), SessionTurnEvent.Turn(started = false, at = 9_000)),
            log.edges,
        )
    }

    @Test
    fun `never invents an end for a slot already ended on arrival`() {
        val log = emptyTurnLog()
        recordTurnSlot(log, TURN_STATE_ENDED, 5_000, 9_000)
        assertTrue(log.edges.isEmpty())
    }

    @Test
    fun `places a message by its at, a live arrival by now, never a replay's bulk`() {
        val log = emptyTurnLog()
        val replay = listOf(
            message(1, "old one"),
            TurnFeedRow(id = 2, isUserMessage = false, text = "x"),
            message(3, "old two"),
        )
        recordFeedMessages(log, replay, 2_000)
        assertTrue(log.messageAt.isEmpty())
        val live = replay + message(4, "follow-up")
        recordFeedMessages(log, live, 7_000)
        val stamped = live + message(5, "stamped", at = 8_000)
        recordFeedMessages(log, stamped, 9_500)
        assertEquals(listOf(4L to 7_000L, 5L to 8_000L), log.messageAt.toList())
    }

    @Test
    fun `skips a subagent's message`() {
        val log = emptyTurnLog()
        recordFeedMessages(log, emptyList(), 1)
        recordFeedMessages(log, listOf(message(1, "hi", subagentId = "s1")), 2)
        assertTrue(log.messageAt.isEmpty())
    }

    @Test
    fun `feeds sessionTurns two turns with the person's bubble between`() {
        val log = emptyTurnLog()
        recordFeedMessages(log, emptyList(), start)
        recordTurnSlot(log, TURN_STATE_STARTED, start, start)
        recordTurnSlot(log, TURN_STATE_ENDED, start, 4_000)
        val feed = listOf(message(1, "stack it", at = 5_000))
        recordFeedMessages(log, feed, 5_000)
        recordTurnSlot(log, TURN_STATE_STARTED, 5_100, 5_100)
        val results = """[
            {"topic":"Summary","label":null,"attachmentId":null,"text":"first","at":3000},
            {"topic":"Reviews","label":null,"attachmentId":null,"text":"second","at":6000}
        ]"""
        val turns = sessionTurns(results, turnEventsOf(log, feed, start))
        assertTrue(turns.perTurn)
        assertEquals(2, turns.turns.size)
        assertNull(turns.turns[0].message)
        assertEquals(4_000L, turns.turns[0].endedAt)
        assertEquals("first", turns.turns[0].reply)
        assertEquals("stack it", turns.turns[1].message?.text)
        assertEquals(5_100L, turns.turns[1].startedAt)
        assertNull(turns.turns[1].endedAt)
        assertEquals(
            listOf("second"),
            turns.turns[1].items.map { (it as ThreadItem.Text).text },
        )
    }

    // web user-message-bubble "the caption drops missing parts".
    @Test
    fun `the bubble caption drops missing parts`() {
        val utc = java.time.ZoneOffset.UTC
        val at = 1_760_000_000_000L
        assertEquals("Danny · 08:53 · from mint", userMessageCaption("Danny", at, "mint", utc))
        assertEquals("08:53", userMessageCaption(" ", at, null, utc))
        assertEquals("Danny", userMessageCaption("Danny", null, "", utc))
    }
}
