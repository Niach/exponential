package com.exponential.app.domain

import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** #EXP-1380 (TS `useNewestMessages`): a chat loads its newest messages,
 *  Load earlier widens by `windowStep`, waiting cards load however old. */
class ChatMessageLimitTest {
    private fun message(i: Int) = SessionTree.Message(
        id = "m%05d".format(i),
        sessionId = "s",
        authorKind = "agent",
        authorId = null,
        body = "row $i",
        card = null,
        answeredAt = null,
        consumedAt = null,
        createdAt = "2026-11-12T%08d".format(i),
    )

    /** What `observeNewestByRoot` returns: the newest [limit], oldest first. */
    private fun newest(all: List<SessionTree.Message>, limit: Int) =
        all.sortedWith(compareBy({ it.createdAt }, { it.id })).takeLast(limit)

    @Test
    fun theLimitIsTheWindowPlusOneStepAndLoadEarlierWidensByAStep() {
        assertEquals(Chats.Transcript.WINDOW + Chats.Transcript.WINDOW_STEP, Chats.CHAT_MESSAGE_LIMIT)
        assertEquals(Chats.CHAT_MESSAGE_LIMIT + Chats.Transcript.WINDOW_STEP, Chats.widenedMessageLimit(Chats.CHAT_MESSAGE_LIMIT))
        runBlocking {
            val key = "chat-limit-test"
            assertEquals(Chats.CHAT_MESSAGE_LIMIT, Chats.messageLimit(key).first())
            Chats.loadEarlierMessages(key)
            Chats.loadEarlierMessages(key)
            assertEquals(Chats.CHAT_MESSAGE_LIMIT + 2 * Chats.Transcript.WINDOW_STEP, Chats.messageLimit(key).first())
            // Per chat: another key is untouched.
            assertEquals(Chats.CHAT_MESSAGE_LIMIT, Chats.messageLimit("$key-other").first())
        }
    }

    @Test
    fun aLargeTreeLoadsOnlyItsNewestPagePlusEveryWaitingCard() {
        val all = (0 until 8_492).map(::message)
        val limit = Chats.CHAT_MESSAGE_LIMIT
        val loaded = newest(all, limit)
        val waiting = listOf(all[3], all[8_491])
        val page = Chats.newestMessagesPage(loaded, waiting, limit)
        assertEquals(limit + 1, page.rows.size)
        assertTrue(page.more)
        // The old waiting card joins in time order; the loaded one is not doubled.
        assertEquals(all[3].id, page.rows.first().id)
        assertEquals(all.last().id, page.rows.last().id)
        assertEquals(page.rows.map { it.id }.distinct(), page.rows.map { it.id })
        // A short tree loads whole: nothing more.
        val short = Chats.newestMessagesPage(all.take(50), emptyList(), limit)
        assertFalse(short.more)
        assertEquals(50, short.rows.size)
    }

    @Test
    fun theWaitingIdsAreEveryDigestsWaitingAndWaitsOnceEachSorted() {
        fun digest(text: String): JsonObject = Json.parseToJsonElement(text).jsonObject
        val ids = Chats.digestWaitingIds(
            listOf(
                digest("""{"waiting":{"id":"b"},"waits":[{"id":"c"},{"id":"b"}]}"""),
                null,
                digest("""{"waits":[{"id":"a"},{"nope":1}]}"""),
                digest("""{"first":{"id":"z"}}"""),
            ),
        )
        assertEquals(listOf("a", "b", "c"), ids)
    }
}
