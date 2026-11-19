package com.exponential.app.data.chatstore

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** #EDE-130: `searchChat`'s answer as find in chat reads it. */
class ChatStoreSearchCodecTest {
    @Test
    fun `hits keep their session, kind, seq and message id`() {
        val hits = ChatStoreCodec.parseHits(
            """{"hits":[
              {"rootId":"r1","sessionId":"s1","deviceId":"d1","kind":"tool_output","at":5,"seq":30,"snippet":"x","matchStart":0,"matchLen":1},
              {"rootId":"r1","sessionId":"s1","kind":"message","at":4,"messageId":"m3","snippet":"x","matchStart":0,"matchLen":1},
              {"kind":"narration"}
            ],"total":3}""",
        )
        assertEquals(
            listOf(StoredChatHit("s1", "tool_output", 30, null), StoredChatHit("s1", "message", null, "m3")),
            hits,
        )
    }

    @Test
    fun `an unreadable answer is no hits`() {
        assertTrue(ChatStoreCodec.parseHits("nope").isEmpty())
        assertTrue(ChatStoreCodec.parseHits("{}").isEmpty())
    }

    @Test
    fun `searchAll's chats keep their root, count and newest hit (EDE-99)`() {
        val chats = ChatStoreCodec.parseAll(
            """{"chats":[
              {"rootId":"r1","count":4,"hit":{"rootId":"r1","sessionId":"c1","deviceId":"d1","kind":"tool_output","at":1794823200000,"seq":7,"snippet":"disk cache hit","matchStart":5,"matchLen":5}},
              {"rootId":"r2","count":1,"hit":{"rootId":"r2","sessionId":"r2","kind":"message","at":4,"messageId":"m3","snippet":"x","matchStart":0,"matchLen":1}},
              {"rootId":"r3","count":1}
            ],"total":3}""",
        )
        assertEquals(listOf("r1", "r2"), chats.map { it.rootId })
        assertEquals(4, chats[0].count)
        assertEquals(
            com.exponential.app.domain.SearchList.ChatHit("r1", "c1", "d1", "tool_output", 1794823200000, 7, null, "disk cache hit", 5, 5),
            chats[0].hit,
        )
        assertEquals("m3", chats[1].hit.messageId)
        assertTrue(ChatStoreCodec.parseAll("nope").isEmpty())
    }
}
