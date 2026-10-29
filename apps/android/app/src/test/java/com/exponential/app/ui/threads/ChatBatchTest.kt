package com.exponential.app.ui.threads

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.fail
import org.junit.Test

/** The selection bar's batch delete (chats.json SELECTION): one call per chat. */
class ChatBatchTest {
    private val ten = (1..10).map { "chat-$it" }

    @Test
    fun `a batch of ten deletes every chat, in selection order`() = runBlocking {
        val deleted = mutableListOf<String>()
        eachChat(ten) { deleted += it }
        assertEquals(ten, deleted)
    }

    @Test
    fun `a refused chat never stops the rest and is counted`() = runBlocking {
        val refusal = IllegalStateException("FORBIDDEN")
        val deleted = mutableListOf<String>()
        try {
            eachChat(ten) { id ->
                if (id == "chat-3" || id == "chat-7") throw refusal
                deleted += id
            }
            fail("expected a PartialSelectionFailure")
        } catch (e: PartialSelectionFailure) {
            assertEquals(2, e.failed)
            assertEquals(10, e.total)
            assertSame(refusal, e.first)
        }
        assertEquals(ten - setOf("chat-3", "chat-7"), deleted)
    }

    @Test
    fun `leaving the screen mid-batch still deletes every chat`() = runBlocking {
        val deleted = mutableListOf<String>()
        val firstStarted = CompletableDeferred<Unit>()
        val release = CompletableDeferred<Unit>()
        val job = launch {
            eachChat(ten) { id ->
                if (id == "chat-1") {
                    firstStarted.complete(Unit)
                    release.await()
                }
                deleted += id
            }
        }
        firstStarted.await()
        // The ViewModel's scope is cancelled (the screen went away).
        val cancel = async { job.cancelAndJoin() }
        release.complete(Unit)
        cancel.await()
        assertEquals(ten, deleted)
    }
}
