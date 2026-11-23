package com.exponential.app.ui.session

import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.steer.SyncedTailMessage
import com.exponential.app.data.steer.syncedTailItems
import com.exponential.app.data.steer.tailFeedRow
import com.exponential.app.domain.AgentFeedItem
import com.exponential.app.domain.AgentFeedRow
import com.exponential.app.domain.MessageDelivery
import com.exponential.app.domain.TranscriptGrouping
import com.exponential.app.ui.chat.sentInputLayouts
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * #EXP-1990 (transcript-grouping.json MEMBER INPUTS): who wrote a line in a
 * team agent's group chat: the raw feed's member inputs, the synced tail and
 * the sent inputs under the no-feed placeholder.
 */
class GroupChatFeedTest {
    private fun user(id: String, name: String) = UserEntity(id = id, name = name, email = "$id@acme.dev", createdAt = "", updatedAt = "")

    private val senders = FeedSenders(
        chat = TranscriptGrouping.Chat(sessionId = "g1", title = "Release", agentId = "ag1", originSessionId = null),
        origin = null,
        onOpen = null,
        viewerId = "u-me",
        members = mapOf("u-mira" to user("u-mira", "Mira Chen")),
    )

    private fun input(id: Long, text: String) = AgentFeedRow.Single(AgentFeedItem.UserMessage(id = id, text = text, at = 1_000L * id))

    @Test
    fun `a member input is its member's bubble, the viewer's own stays own`() {
        val rows = listOf(
            input(1, TranscriptGrouping.memberInput("Alex", "u-me", "what is open?")),
            input(2, TranscriptGrouping.memberInput("Mira C.", "u-mira", "the changelog")),
            input(3, TranscriptGrouping.memberInput("Gone", "u-gone", "bye")),
            input(4, "a bare prompt"),
        )
        val layouts = feedTranscriptLayout(rows, SessionCardSlots.None, senders, agent = null, fromStart = true)
        assertEquals(TranscriptGrouping.Side.Own, layouts.getValue(1).layout.side)
        val mira = layouts.getValue(2).layout
        assertEquals(TranscriptGrouping.Side.Other, mira.side)
        assertEquals(TranscriptGrouping.SenderKind.Member, mira.sender?.kind)
        assertEquals("u-mira", mira.sender?.id)
        // The synced member's name wins over the line's.
        assertEquals("Mira Chen", mira.sender?.name)
        assertEquals("Mira Chen", senders.memberOf(mira.sender!!)?.name)
        // A member this install does not sync keeps the line's name.
        assertEquals("Gone", layouts.getValue(3).layout.sender?.name)
        assertEquals(TranscriptGrouping.Side.Own, layouts.getValue(4).layout.side)
    }

    @Test
    fun `without a viewer nobody's member input is taken for the viewer's`() {
        val rows = listOf(input(1, TranscriptGrouping.memberInput("Alex", "u-me", "hello")))
        val layouts = feedTranscriptLayout(rows, SessionCardSlots.None, senders.copy(viewerId = null), agent = null, fromStart = true)
        assertEquals(TranscriptGrouping.Side.Other, layouts.getValue(1).layout.side)
    }

    private fun synced(id: String, authorId: String?, body: String) =
        SyncedTailMessage(id, "user", "2026-11-23T10:00:00.000Z", "2026-11-23T10:00:00.000Z", 2_000_000L, body, authorId)

    @Test
    fun `the synced tail words a teammate's input as theirs`() {
        val feed = listOf(AgentFeedItem.Narration(id = 1, text = "Working on it.", at = 1_000_000L))
        val items = syncedTailItems(
            feed,
            live = false,
            messages = listOf(synced("m1", "u-me", "mine"), synced("m2", "u-mira", "hers"), synced("m3", null, "unknown")),
            viewerId = "u-me",
            memberName = { id -> "Mira Chen".takeIf { id == "u-mira" } },
        ).map { (it as AgentFeedItem.UserMessage).text }
        assertEquals(listOf("mine", "Message from Mira Chen (member u-mira): hers", "unknown"), items)
    }

    @Test
    fun `a member input in the feed holds its synced row by the body`() {
        val row = AgentFeedItem.UserMessage(id = 1, text = "Message from Mira Chen (member u-mira): hers", at = 5L).tailFeedRow()
        assertEquals("hers", row?.text)
        assertEquals("plain", AgentFeedItem.UserMessage(id = 2, text = "plain", at = 5L).tailFeedRow()?.text)
    }

    private fun bubble(key: String, authorId: String?) = MessageDelivery.Bubble(
        key = key, text = key, createdAt = "2026-11-23T10:00:00.000Z", state = MessageDelivery.State.Read,
        stored = true, clientId = null, messageId = key, authorId = authorId,
    )

    @Test
    fun `sent inputs group by their author, the viewer's own as before`() {
        // Every bubble the viewer's (or unnamed): the own tail, untouched.
        assertNull(sentInputLayouts(listOf(bubble("a", "u-me"), bubble("b", null)), "u-me") { null })
        assertNull(sentInputLayouts(listOf(bubble("a", "u-mira")), null) { null })
        val layouts = sentInputLayouts(
            listOf(bubble("a", "u-me"), bubble("b", "u-mira"), bubble("c", "u-mira"), bubble("d", "u-gone")),
            "u-me",
        ) { id -> "Mira Chen".takeIf { id == "u-mira" } }!!
        assertEquals(TranscriptGrouping.Side.Own, layouts.getValue("a").side)
        val first = layouts.getValue("b")
        assertEquals(TranscriptGrouping.Side.Other, first.side)
        assertEquals("Mira Chen", first.sender?.name)
        assertEquals(true, first.avatar && first.name && first.tail)
        // The group's second bubble: no second avatar, name or tail.
        val second = layouts.getValue("c")
        assertEquals(false, second.avatar || second.name || second.tail)
        assertEquals(TranscriptGrouping.Copy.FORMER_MEMBER, layouts.getValue("d").sender?.name)
    }
}
