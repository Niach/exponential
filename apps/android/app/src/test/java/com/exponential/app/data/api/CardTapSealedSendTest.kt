package com.exponential.app.data.api

import com.exponential.app.data.sealed.NO_KEY_REFUSAL
import com.exponential.app.data.sealed.PreparedSend
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/**
 * #EXP-1958 (sealed-content.json `cardStamps` NEVER IN CLEAR): a card tap's
 * send ([sendPrepared], the path every `cardItem` send of [SessionsApi]
 * takes) leaves only as the writer sealed it; anything else is refused with
 * [NO_KEY_REFUSAL] and the transport sees ZERO requests.
 */
class CardTapSealedSendTest {
    private val item = CardItemRef("4d2c8b1e-5a7f-4e30-9b6d-2f1a0c9e8d73", "9e0f1a2b-3c4d-4e5f-8a6b-7c8d9e0f1a2b")
    private val session = "6f1c2a4e-3b7d-4c1e-9a52-0d8e7f6b5a41"
    private val reply = SessionsSendInput(sessionId = session, text = "Yes, ship it", cardItem = item)
    private val start = SessionsSendInput(parentId = session, text = "Add tests\n\nCover it", name = "Add tests", cardItem = item)
    private val envelope = "exp-sealed:v1:1:AAAA"

    private fun run(input: SessionsSendInput, prepare: suspend (Boolean) -> PreparedSend): Pair<Result<SessionsSendResult>, List<SessionsSendInput>> {
        val sent = mutableListOf<SessionsSendInput>()
        val out = runCatching {
            runBlocking { sendPrepared(input, prepare) { wire -> sent += wire; SessionsSendResult(sessionId = session) } }
        }
        return out to sent
    }

    private fun assertRefused(out: Result<SessionsSendResult>, sent: List<SessionsSendInput>) {
        assertEquals(NO_KEY_REFUSAL, out.exceptionOrNull()?.message)
        assertTrue("nothing reaches the transport: $sent", sent.isEmpty())
    }

    @Test
    fun aTapIntoAClearTreeSendsNothing() {
        // The writer resolved a clear tree: it hands the input back in clear.
        for (input in listOf(reply, start)) {
            val (out, sent) = run(input) { PreparedSend(input) }
            assertRefused(out, sent)
        }
        // A sealed text beside a clear title is refused too.
        val (out, sent) = run(start) { PreparedSend(start.copy(text = envelope)) }
        assertRefused(out, sent)
    }

    @Test
    fun aTapWithoutKeysSendsNothing() {
        for (input in listOf(reply, start)) {
            val (out, sent) = run(input) { throw TrpcException(NO_KEY_REFUSAL) }
            assertRefused(out, sent)
        }
    }

    @Test
    fun aKeyedTapSendsSealedWithItsCardItem() {
        val (out, sent) = run(reply) { PreparedSend(reply.copy(text = envelope)) }
        assertEquals(session, out.getOrThrow().sessionId)
        assertEquals(listOf(reply.copy(text = envelope)), sent)
        assertEquals(item, sent.single().cardItem)

        val sealedStart = start.copy(text = envelope, name = null, sealedName = "exp-sealed:v1:1:BBBB")
        val (startOut, startSent) = run(start) { PreparedSend(sealedStart) }
        startOut.getOrThrow()
        assertEquals(listOf(sealedStart), startSent)
    }

    @Test
    fun theComposersOwnSendIntoAClearTreeStaysPlaintext() {
        val plain = SessionsSendInput(sessionId = session, text = "in clear")
        val (out, sent) = run(plain) { PreparedSend(plain) }
        out.getOrThrow()
        if (sent != listOf(plain)) fail("$sent")
    }
}
