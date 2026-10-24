package com.exponential.app.ui.thread

import com.exponential.app.domain.Chats
import com.exponential.app.domain.SessionTree
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Test

/** #EXP-1380 (chats.json WINDOW): the painted window + the sessions a chat's
 *  cards name outside its tree. */
class TranscriptWindowTest {

    private fun rows(n: Int) = (0 until n).map { SessionTree.ThreadRow.Divider("r$it", "new", "") }

    @Test
    fun aShortChatPaintsEveryRow() {
        assertEquals(0, transcriptWindowStart(rows(40), null))
    }

    @Test
    fun aLongChatPaintsItsNewestWindow() {
        assertEquals(1500 - Chats.Transcript.WINDOW, transcriptWindowStart(rows(1500), null))
    }

    @Test
    fun aPinnedStartKeepsEveryArrivalAndLoadEarlierWidens() {
        val before = rows(1500)
        val pinned = before[transcriptWindowStart(before, null)].id
        // An arrival never shrinks the window: the pinned start stays.
        val after = rows(1501)
        assertEquals(1400, transcriptWindowStart(after, pinned))
        // Load earlier = the row `windowStep` above the start.
        val wider = after[1400 - Chats.Transcript.WINDOW_STEP].id
        assertEquals(1300, transcriptWindowStart(after, wider))
        // A start that left the transcript falls back to the newest window.
        assertEquals(1501 - Chats.Transcript.WINDOW, transcriptWindowStart(after, "gone"))
    }

    private fun message(id: String, card: String?) = SessionTree.Message(
        id = id,
        sessionId = "root",
        authorKind = "agent",
        authorId = null,
        body = "",
        card = card?.let { Json.parseToJsonElement(it).jsonObject },
        answeredAt = null,
        consumedAt = null,
        createdAt = "",
    )

    @Test
    fun referencedSessionsAreTheCardsSessionIdsAndTheirItems() {
        val ids = referencedSessionIds(
            listOf(
                message("m1", null),
                message("m2", """{"kind":"thread_started","sessionId":"t2"}"""),
                message("m3", """{"kind":"suggestions","items":[{"id":"a","sessionId":"c1"},{"id":"b"}]}"""),
                message("m4", """{"kind":"report","sessionId":"c1"}"""),
            ),
        )
        assertEquals(listOf("c1", "t2"), ids)
    }
}
