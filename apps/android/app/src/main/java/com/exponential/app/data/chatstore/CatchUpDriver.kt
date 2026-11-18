package com.exponential.app.data.chatstore

import at.exponential.chatstore.ffi.CatchUp
import at.exponential.chatstore.ffi.CatchUpSession
import at.exponential.chatstore.ffi.CatchUpTaken
import at.exponential.chatstore.ffi.PageAccess
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/**
 * #EDE-114 (chat-store.json `catchUp` THE ASKER): the catch-up answers in
 * flight. A `history_chunk` whose `requestId` is pending is [take]n here,
 * ahead of any viewer feed (`EnetSteerSocket` and the store's own node
 * subscription both offer every chunk; the first take wins), so it never
 * reaches a session view.
 */
object CatchUpChunks {
    private val pending = ConcurrentHashMap<String, CompletableDeferred<String>>()

    fun expect(requestId: String): CompletableDeferred<String> =
        CompletableDeferred<String>().also { pending[requestId] = it }

    fun forget(requestId: String) {
        pending.remove(requestId)
    }

    fun isPending(requestId: String): Boolean = pending.containsKey(requestId)

    /** True = [frame] answered a pending ask (taken: no viewer sees it). */
    fun take(frame: JsonObject): Boolean {
        if ((frame["t"] as? JsonPrimitive)?.content != "history_chunk") return false
        val id = (frame["requestId"] as? JsonPrimitive)?.content ?: return false
        val waiting = pending.remove(id) ?: return false
        waiting.complete(frame.toString())
        return true
    }
}

/**
 * #EDE-114: a catch-up ask's frame is a device-naming `history_page`
 * (`chat-store.json` THE WIRE): never a viewer's (a viewer's page names no
 * `deviceId`).
 */
internal fun isCatchUpPageFrame(frame: JsonObject): Boolean =
    (frame["t"] as? JsonPrimitive)?.content == "history_page" && frame["deviceId"] is JsonPrimitive

/**
 * steer-enet.json ACCESS (the join's rule, the HOLDER's too): the asker's user
 * (from the session team's registry list) is the session's starter, or the
 * session is shared; no synced row = `no_such_session`.
 */
fun pageAccess(rowFound: Boolean, starterUserId: String?, shared: Boolean, askerUserId: String?): PageAccess = when {
    !rowFound -> PageAccess.NO_SUCH_SESSION
    askerUserId != null && (askerUserId == starterUserId || shared) -> PageAccess.ALLOWED
    else -> PageAccess.UNAUTHORIZED
}

/** Carries one ask's frame to a peer (in the steer envelope of its session); false = not sent. */
fun interface CatchUpCarrier {
    suspend fun send(peer: String, sessionId: String, frame: String): Boolean
}

/**
 * #EDE-114 (chat-store.json THE PHONE WALK): runs one pass of the facade's
 * sans-I/O [CatchUp]: `concurrency()` walkers, each looping
 * `nextWalk` → `nextAsk` → (wait, send, await the chunk) → `takeAnswer`
 * until the pass has nothing left. Blocking facade calls run on [io].
 * Cancelled (the OS window ended, the app left the foreground or the
 * unmetered network): [CatchUp.stop], the watermarks keep the place.
 */
class CatchUpDriver(
    private val catchUp: CatchUp,
    private val carrier: CatchUpCarrier,
    private val nowMs: () -> Long = System::currentTimeMillis,
    private val io: kotlinx.coroutines.CoroutineDispatcher = Dispatchers.IO,
    private val onTaken: (CatchUpTaken) -> Unit = {},
) {
    /** One pass over [sessions]; returns how many sessions were due. */
    suspend fun pass(sessions: List<CatchUpSession>): Int {
        val due = withContext(io) { catchUp.beginPass(sessions, nowMs()) }.toInt()
        if (due == 0) return 0
        val walkers = withContext(io) { catchUp.concurrency() }.toInt().coerceAtLeast(1)
        val timeoutMs = withContext(io) { catchUp.askTimeoutMs() }.toLong()
        try {
            coroutineScope {
                repeat(walkers) { launch { walk(timeoutMs) } }
            }
        } catch (err: CancellationException) {
            withContext(kotlinx.coroutines.NonCancellable + io) { catchUp.stop() }
            throw err
        }
        return due
    }

    private suspend fun walk(timeoutMs: Long) {
        while (true) {
            val session = withContext(io) { catchUp.nextWalk(nowMs()) } ?: return
            while (true) {
                val ask = withContext(io) { catchUp.nextAsk(session, nowMs()) } ?: break
                if (ask.waitMs > 0u) delay(ask.waitMs.toLong())
                val answer = CatchUpChunks.expect(ask.requestId)
                val chunk = try {
                    val sent = runCatching { carrier.send(ask.peer, ask.sessionId, ask.frame) }
                        .onFailure { if (it is CancellationException) throw it }
                        .getOrDefault(false)
                    if (sent) withTimeoutOrNull(timeoutMs) { answer.await() } else null
                } finally {
                    CatchUpChunks.forget(ask.requestId)
                }
                val taken = withContext(io) { catchUp.takeAnswer(ask.requestId, chunk, nowMs()) }
                if (taken != null && taken.added > 0u) onTaken(taken)
            }
        }
    }
}
