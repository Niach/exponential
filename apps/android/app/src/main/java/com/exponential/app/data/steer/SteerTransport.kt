package com.exponential.app.data.steer

import com.exponential.app.data.api.SteerApi
import com.exponential.app.data.api.SteerTicketResult
import com.exponential.app.data.auth.AuthRepository
import io.ktor.client.HttpClient
import io.ktor.client.plugins.websocket.DefaultClientWebSocketSession
import io.ktor.client.plugins.websocket.webSocketSession
import io.ktor.websocket.Frame
import io.ktor.websocket.readText
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.ReceiveChannel
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

// EXP-625: the ticket mint and the websocket itself, behind an interface, so
// [SteerConnection]'s dial/redial machinery is testable on the JVM without an
// engine. The revival bugs it grew were all in the loop, and none of them
// was reachable from a test while the loop spoke ktor directly.

/** A closed socket's close reason is already settled — this only guards
 *  against a dial that somehow left it pending. */
private const val CLOSE_REASON_TIMEOUT_MS = 1_000L

/** One open viewer socket. TEXT only: the PTY mirror is gone (EXP-249), so an
 *  old desktop's binary output frames never reach the connection. */
internal interface SteerSocket {
    val incoming: ReceiveChannel<String>
    suspend fun send(text: String)
    fun cancel()

    /** The relay's close code once the socket closed on its own, if any. */
    suspend fun closeCode(): Int?
}

internal interface SteerTransport {
    suspend fun mint(codingSessionId: String): SteerTicketResult

    /** Dial the full `ws(s)://…/ws?ticket=…` URL the mint returned. */
    suspend fun open(url: String): SteerSocket
}

internal class KtorSteerTransport(
    private val auth: AuthRepository,
    private val steerApi: SteerApi,
    private val client: HttpClient,
) : SteerTransport {

    override suspend fun mint(codingSessionId: String): SteerTicketResult {
        val accountId = auth.activeAccountId.value
            ?: throw IllegalStateException("No active account")
        return steerApi.mintViewerTicket(accountId, codingSessionId)
    }

    override suspend fun open(url: String): SteerSocket =
        KtorSteerSocket(client.webSocketSession(urlString = url))
}

private class KtorSteerSocket(private val session: DefaultClientWebSocketSession) : SteerSocket {

    private val texts = Channel<String>(Channel.UNLIMITED)

    init {
        // Pump text frames onto our own channel. The `finally` is the point:
        // the session's scope is cancelled the moment the socket dies, and a
        // channel left open there would hang the connection's frame loop for
        // good (EXP-625).
        session.launch {
            try {
                for (frame in session.incoming) {
                    if (frame is Frame.Text) texts.send(frame.readText())
                }
            } catch (t: Throwable) {
                // Whatever ended the read, the close code below says why. Never
                // rethrown: this is a child of ktor's session scope, and an
                // exception escaping here would be an unhandled coroutine
                // failure rather than a closed socket.
                if (t is CancellationException) throw t
            } finally {
                texts.close()
            }
        }
    }

    override val incoming: ReceiveChannel<String> get() = texts

    override suspend fun send(text: String) {
        session.send(Frame.Text(text))
    }

    override fun cancel() {
        runCatching { session.cancel() }
    }

    override suspend fun closeCode(): Int? = try {
        withTimeoutOrNull(CLOSE_REASON_TIMEOUT_MS) { session.closeReason.await() }?.code?.toInt()
    } catch (t: Throwable) {
        // ktor surfaces a java.util.concurrent.CancellationException off a
        // cancelled call job here: that is the RELAY hanging up, not us
        // (EXP-625). Only our own cancellation may end the dial.
        if (t is CancellationException && !currentCoroutineContext().isActive) throw t
        null
    }
}
