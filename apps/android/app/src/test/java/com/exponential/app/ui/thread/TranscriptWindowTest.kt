package com.exponential.app.ui.thread

import com.exponential.app.domain.Chats
import com.exponential.app.domain.SessionTree
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Test

/** #EXP-1380 (chats.json WINDOW): the waiting cards the window never cuts
 *  off (the rule itself replays `windowCases` in ChatsTest) + the sessions a
 *  chat's cards name outside its tree. */
class TranscriptWindowTest {

    private fun plan(id: String, decision: String?) = SessionTree.ThreadRow.Plan(id = id, sessionId = "root", plan = "", decision = decision)

    private fun question(id: String, answered: Boolean) = SessionTree.ThreadRow.Question(
        id = id,
        sessionId = "root",
        question = "",
        options = emptyList(),
        custom = true,
        answer = null,
        answered = answered,
    )

    @Test
    fun theWaitingCardsAreTheUndecidedPlansAndUnansweredQuestions() {
        val rows = listOf(
            SessionTree.ThreadRow.Divider("d", "new", ""),
            plan("p1", decision = "approved"),
            question("q1", answered = false),
            plan("p2", decision = null),
            question("q2", answered = true),
        )
        assertEquals(listOf(2, 3), transcriptWaitingIndexes(rows))
    }

    @Test
    fun aWaitingCardOlderThanTheWindowIsNeverCutOff() {
        val rows = (0 until 250).map { i ->
            if (i == 30) question("q", answered = false) else SessionTree.ThreadRow.Divider("r$i", "new", "")
        }
        assertEquals(30, Chats.transcriptWindowStart(rows.size, transcriptWaitingIndexes(rows), null, 0))
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
