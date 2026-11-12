package com.exponential.app.ui.chat

import com.exponential.app.data.db.SessionEntity
import com.exponential.app.domain.Chats
import com.exponential.app.domain.SessionTree
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Test

/** results-tab.json + session-screen.json `count`: the Results tab and its
 *  count fold the tree's CARD rows ([ChatViewModel.resultCards]), never the
 *  chat's loaded window (#EXP-1380), so an old result never vanishes. */
class ChatResultsCardsTest {

    private val root = SessionEntity(id = "root", rootId = "root", title = "Chat")

    private fun createdAt(i: Int) = "2026-11-01T00:%02d:%02d.000Z".format(i / 60, i % 60)

    private fun text(i: Int) = SessionTree.Message(
        id = "m%04d".format(i),
        sessionId = "root",
        authorKind = "agent",
        authorId = null,
        body = "line $i",
        card = null,
        answeredAt = null,
        consumedAt = null,
        createdAt = createdAt(i),
        authorSessionId = "root",
    )

    private fun result(i: Int) = text(i).copy(
        body = "",
        card = Json.parseToJsonElement(
            """{"kind":"surface","title":"Shot $i","result":true,"pictures":[{"label":"web shot","attachmentId":"a$i"}]}""",
        ).jsonObject,
    )

    @Test
    fun aResultOlderThanTheLoadedWindowStillCounts() {
        val all = (0 until 600).map { if (it == 3) result(it) else text(it) }
        val limit = 200
        val window = Chats.newestMessagesPage(all.takeLast(limit), emptyList(), limit).rows
        // The window lost it: folding the loaded rows would count nothing.
        assertEquals(0, chatResultsCount("root", listOf(root), window))
        // The card rows (root_id + author_session_id + card) keep it.
        val cards = all.filter { it.authorSessionId != null && it.card != null }
        assertEquals(1, chatResultsCount("root", listOf(root), cards))
        val inputs = resultsTabInputs(listOf(root), cards)
        assertEquals(listOf("m0003"), inputs.second.map { it.id })
    }
}
