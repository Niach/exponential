package com.exponential.app.ui.session

import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.steer.SyncedTailMessage
import com.exponential.app.data.steer.syncedTailItems
import com.exponential.app.data.steer.tailFeedRow
import com.exponential.app.domain.AgentFeedItem
import com.exponential.app.domain.AgentFeedRow
import com.exponential.app.domain.ChatExpress
import com.exponential.app.domain.MessageDelivery
import com.exponential.app.domain.TranscriptGrouping
import com.exponential.app.ui.chat.sentInputLayouts
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
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
    fun `the synced tail draws a teammate's consumed press as their action input until the feed holds it`() {
        // #EXP-2075 (steer-enet.json SYNCED TAIL ... ACTION ROWS).
        val reply = ChatExpress.ActionCard("reply", null, "n:s1:msg_01#1", "n:s1:msg_01#1", null)
        val save = ChatExpress.ActionCard("save_deal", buildJsonObject { put("rep", "Ana") }, "n:s1:msg_01#1", "n:s1:msg_01#1", null)
        val messages = listOf(
            synced("p1", "u-mira", "Ship it ").copy(hasCard = true, action = reply),
            synced("p2", "u-mira", "").copy(hasCard = true, action = save),
        )
        val narration = AgentFeedItem.Narration(id = 1, text = "Ship the first cut?", at = 1_000_000L)
        fun tail(feed: List<AgentFeedItem>) = syncedTailItems(
            feed,
            live = false,
            messages = messages,
            viewerId = "u-me",
            memberName = { id -> "Mira Chen".takeIf { id == "u-mira" } },
        ).map { (it as AgentFeedItem.UserMessage).text }
        val bodied = TranscriptGrouping.memberInput("Mira Chen", "u-mira", ChatExpress.actionInput(reply, "Ship it"))
        val bodiless = TranscriptGrouping.memberInput("Mira Chen", "u-mira", ChatExpress.actionInput(save, ""))
        assertEquals(listOf(bodied, bodiless), tail(listOf(narration)))
        // The member wording carries the `[action]` line back out.
        assertEquals(ChatExpress.parseActionInput(TranscriptGrouping.memberInputOf(bodiless)!!.body)?.card, save)
        // Bodied = their bubble; bodiless = THE marker (full, never grouped).
        val layouts = feedTranscriptLayout(listOf(input(1, bodied), input(2, bodiless)), SessionCardSlots.None, senders, agent = null, fromStart = true)
        assertEquals(TranscriptGrouping.Side.Other, layouts.getValue(1).layout.side)
        assertEquals("u-mira", layouts.getValue(1).layout.sender?.id)
        assertEquals(TranscriptGrouping.Side.Full, layouts.getValue(2).layout.side)
        // The feed holds both presses as its `[action]` inputs: none drawn twice.
        val held = listOf(
            narration,
            AgentFeedItem.UserMessage(id = 2, text = ChatExpress.actionInput(reply, "Ship it"), at = 1_500_000L),
            AgentFeedItem.UserMessage(id = 3, text = ChatExpress.actionInput(save, ""), at = 1_500_001L),
        )
        assertEquals(emptyList<String>(), tail(held))
    }

    @Test
    fun `a member input in the feed holds its synced row by the body`() {
        val row = AgentFeedItem.UserMessage(id = 1, text = "Message from Mira Chen (member u-mira): hers", at = 5L).tailFeedRow()
        assertEquals("hers", row?.text)
        assertEquals("plain", AgentFeedItem.UserMessage(id = 2, text = "plain", at = 5L).tailFeedRow()?.text)
    }

    // #EXP-2084: the feed's own echo of a child's reply holds its synced
    // row, even when the device's clock is behind the server's.
    @Test
    fun `a child's reply the feed holds as that session's input is not drawn twice`() {
        fun reply(id: String, from: String, body: String) = SyncedTailMessage(
            id, "agent", "2026-11-23T10:00:00.000Z", "2026-11-23T10:00:00.000Z", 2_000_000L, body, authorSessionId = from,
        )
        val feed = listOf(
            AgentFeedItem.UserMessage(id = 1, text = "Message from Fix the login (session s-child): The PR is open.", at = 1_000L),
            AgentFeedItem.Narration(id = 2, text = "Noted.", at = 1_000_000L),
        )
        val items = syncedTailItems(
            feed,
            live = false,
            messages = listOf(reply("r1", "s-child", "The PR is open."), reply("r2", "s-child", "CI is green."), reply("r3", "s-other", "The PR is open.")),
        ).map { (it as AgentFeedItem.Narration).messageId }
        assertEquals(listOf("synced:r2", "synced:r3"), items)
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

    @Test
    fun `a teammate's input the device has not consumed draws as theirs, never the viewer's own`() {
        fun row(id: String, authorId: String?, consumed: Boolean = false, card: String? = null, body: String = id) =
            com.exponential.app.data.db.MessageEntity(
                id = id, sessionId = "g1", authorKind = "user", authorId = authorId, body = body, card = card,
                consumedAt = if (consumed) "2026-11-23T10:00:05.000Z" else null, createdAt = "2026-11-23T10:00:00.000Z",
            )
        val rows = listOf(row("mine", "u-me"), row("hers", "u-mira"), row("done", "u-mira", consumed = true),
            row("card", "u-mira", card = """{"kind":"action"}"""), row("blank", "u-mira", body = " "), row("old", null))
        val tail = teammatePending(rows, "u-me")
        assertEquals(listOf("member-row-hers"), tail.map { it.key })
        assertEquals("u-mira", tail.single().authorId)
        assertEquals(emptyList<MessageDelivery.Bubble>(), teammatePending(rows, null))
    }
}
