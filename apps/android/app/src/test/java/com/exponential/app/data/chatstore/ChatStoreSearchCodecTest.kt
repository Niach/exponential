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
}
