package com.exponential.app.ui.share

import java.util.concurrent.atomic.AtomicReference
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The share's consume-once hand-off into a composer: taken once, and its
 * delivery survives the effect that took it (taking it restarts that effect).
 */
class TakeChatShareTest {
    @Test fun `the delivery outlives the effect that took the share`() = runBlocking {
        val screen = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val waiting = AtomicReference<String?>("share")
        val attached = CompletableDeferred<List<String>>()
        val effect = launch(start = CoroutineStart.UNDISPATCHED) {
            takeChatShare(screen, { waiting.compareAndSet("share", null) }) {
                // The first image's read suspends, like readComposerPick's IO hop.
                delay(50)
                attached.complete(listOf("image-1", "image-2"))
            }
            awaitCancellation()
        }
        // Taking it nulled the pending share: the keyed effect restarts.
        effect.cancelAndJoin()
        assertEquals(listOf("image-1", "image-2"), withTimeout(2_000) { attached.await() })
        screen.cancel()
    }

    @Test fun `the old inline delivery died with its effect`() = runBlocking {
        var attached = false
        val effect = launch(start = CoroutineStart.UNDISPATCHED) {
            delay(50)
            attached = true
        }
        effect.cancelAndJoin()
        assertFalse(attached)
    }

    @Test fun `a share is taken once`() {
        val screen = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        val waiting = AtomicReference<String?>("share")
        var deliveries = 0
        val take = { waiting.compareAndSet("share", null) }
        assertTrue(takeChatShare(screen, take) { deliveries++ })
        assertFalse(takeChatShare(screen, take) { deliveries++ })
        runBlocking { delay(100) }
        assertEquals(1, deliveries)
        screen.cancel()
    }
}
