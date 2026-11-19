package com.exponential.app.data.call

import at.exponential.net.ffi.EnetCallFinish
import at.exponential.net.ffi.EnetCallHandshake
import at.exponential.net.ffi.EnetCallRole
import at.exponential.net.ffi.EnetCallSeal
import at.exponential.net.ffi.EnetException
import at.exponential.net.ffi.EnetOpenedAudio
import at.exponential.net.ffi.callSealPad
import com.exponential.app.domain.VoiceCall
import java.nio.ByteBuffer
import java.util.Base64
import java.util.UUID
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/** The chat a call is keyed with, as `sessions.sendTarget {agentId}` names it (null root = none yet). */
data class CallKeyTarget(val rootId: String?, val epoch: Int?, val rotationDue: Boolean = false)

/** Where a caller gets the chat's key: this install's key holder. */
interface CallKeySource {
    /** Waits (bounded) for a key holder that is still starting; false = no claim, today's wire. */
    suspend fun ready(): Boolean
    suspend fun target(agentId: String): CallKeyTarget
    /** The tree secret of (root, epoch): the keyring, else ONE unthrottled pull. */
    suspend fun secret(rootId: String, epoch: Int): ByteArray?
    /** The highest epoch this install holds for the root. */
    fun highestEpoch(rootId: String): Int?
}

/**
 * #EXP-1941: the CALLER's half of a sealed call (the twin of the web's
 * `seal.ts`; the sealer is the Rust core through exponential-net-ffi, the
 * steps are [VoiceCall.Seal.callerStep]). ONE gate holds the microphone:
 * [sealUplink] seals only through a [EnetCallSeal], which exists only after
 * `finish` opened the host's tag. Frames are opened here, in the socket's
 * arrival order, before anything else reads them. Safe on the capture thread
 * and the socket's reader at once.
 */
internal class CallSealLine(
    private val callId: String,
    private val agentId: String,
    /** The host's steer device id, as `startCall` answered it. */
    private val deviceId: String,
    private val keys: CallKeySource,
    target: CallKeyTarget,
    private val scope: CoroutineScope,
    private val sendBinary: (ByteArray) -> Unit,
    /** `call_end {reason}`. */
    private val end: (String) -> Unit,
    /** The call is over with this toast code. */
    private val fail: (String) -> Unit,
) {
    private val lock = Any()
    @Volatile private var state = VoiceCall.Seal.IDLE
    private var target = target
    private val handshake = EnetCallHandshake(EnetCallRole.CALLER, target.rootId, if (target.rootId == null) 0u else (target.epoch ?: 0).toUInt())
    private var hostHello: ByteArray? = null
    private var secret: ByteArray? = null
    @Volatile private var seal: EnetCallSeal? = null
    private var helloJob: Job? = null
    private var timerJob: Job? = null
    private var keyJob: Job? = null

    /** Frames dropped (not well-formed, did not open) and packets sent as an empty frame. */
    @Volatile var dropped = 0
        private set

    init {
        step(VoiceCall.Seal.Event.Start(seal = true))
    }

    fun step(event: VoiceCall.Seal.Event): Unit = synchronized(lock) {
        val stepped = VoiceCall.Seal.callerStep(state, event)
        state = stepped.state
        for (action in stepped.actions) act(action)
    }

    private fun act(action: String) {
        val argument = action.substringAfter(':', "")
        when (action.substringBefore(':')) {
            "timer" -> timerJob = scope.launch {
                delay(VoiceCall.Seal.HANDSHAKE_MS)
                step(VoiceCall.Seal.Event.Timeout("handshake"))
            }
            "hello" -> {
                val frame = byteArrayOf(VoiceCall.Wire.KIND_UP.toByte(), VoiceCall.Seal.TYPE_HELLO.toByte()) + handshake.hello()
                sendBinary(frame)
                helloJob = scope.launch {
                    while (isActive) {
                        delay(VoiceCall.Seal.HELLO_EVERY_MS)
                        sendBinary(frame)
                    }
                }
            }
            "helloStop" -> helloJob?.cancel()
            "key" -> keyJob = scope.launch { resolveKey() }
            "finish" -> finish()
            "count" -> dropped += 1
            "end" -> end(argument)
            "fail" -> fail(argument)
            "abandon" -> close()
        }
    }

    /** The host's hello was taken: its root and epoch against the caller's own answer (asked again once), then the secret, pulled until it is there. */
    private suspend fun resolveKey() {
        val hello = parseHostHello(hostHello ?: return) ?: return step(VoiceCall.Seal.Event.NoKey)
        fun agrees(t: CallKeyTarget) = t.rootId?.lowercase() == hello.first && t.epoch == hello.second && !t.rotationDue
        try {
            if (!agrees(target)) {
                val again = keys.target(agentId)
                synchronized(lock) { target = again }
                if (!agrees(again)) return step(VoiceCall.Seal.Event.NoKey)
            }
            // No rollback: never an epoch below the highest this install holds.
            val highest = keys.highestEpoch(hello.first)
            if (highest != null && hello.second < highest) {
                android.util.Log.w(TAG, "epoch_below_held root=${hello.first} named=${hello.second} held=$highest")
                return step(VoiceCall.Seal.Event.NoKey)
            }
            while (true) {
                val found = keys.secret(hello.first, hello.second)
                if (found != null) {
                    synchronized(lock) { secret = found }
                    return step(VoiceCall.Seal.Event.Key)
                }
                delay(VoiceCall.Seal.PULL_EVERY_MS)
            }
        } catch (err: kotlinx.coroutines.CancellationException) {
            throw err
        } catch (_: Throwable) {
            step(VoiceCall.Seal.Event.NoKey)
        }
    }

    /** Once: it refuses a hello that names another root or epoch than the caller's own answer and opens the tag; a refusal has used it up. */
    private fun finish() {
        val key = secret ?: return
        secret = null
        seal = try {
            handshake.finish(EnetCallFinish(key, callId, agentId, deviceId, hostHello!!, target.rootId?.lowercase(), (target.epoch ?: 0).toUInt(), callSealPad()))
        } catch (err: EnetException) {
            android.util.Log.w(TAG, "the handshake failed: ${err.message}")
            null
        } finally {
            key.fill(0)
        }
        step(VoiceCall.Seal.Event.Finished(seal != null))
    }

    fun ready() = step(VoiceCall.Seal.Event.Ready)

    /** A `call_event` of the host that is not `failed`: its hello, a sealed event (answers the control frame inside, else null) or clear content. */
    fun event(t: String?, data: String?): String? {
        when (t) {
            "hello" -> {
                val hello = data?.let { runCatching { Base64.getUrlDecoder().decode(it) }.getOrNull() }
                val ok = hello != null && parseHostHello(hello) != null
                synchronized(lock) {
                    if (ok && state.phase == VoiceCall.Seal.Phase.Offered) hostHello = hello
                    step(VoiceCall.Seal.Event.Hello(ok))
                }
            }
            "sealed" -> {
                val text = runCatching { seal?.openControl(data ?: "") }.getOrNull()
                step(VoiceCall.Seal.Event.Sealed(text != null))
                return text
            }
            else -> step(VoiceCall.Seal.Event.Clear)
        }
        return null
    }

    /** A binary frame: the opened audio, or null (dropped, or the call ended). */
    fun frame(bytes: ByteArray): EnetOpenedAudio? {
        val opened = runCatching { seal?.openAudio(bytes) }.getOrNull()
        step(VoiceCall.Seal.Event.Frame(opened != null))
        return opened
    }

    /** One uplink frame of the capture thread (`[1, seq hi, seq lo] + packet`) as ONE sealed frame, mute inside; null before the host's proof. A packet above the step leaves as an empty frame with its seq. */
    fun sealUplink(frame: ByteArray, muted: Boolean): ByteArray? {
        val sealer = seal ?: return null
        if (state.phase != VoiceCall.Seal.Phase.Sealed || frame.size < 3) return null
        val seq = (((frame[1].toInt() and 0xff) shl 8) or (frame[2].toInt() and 0xff)).toUShort()
        val flags: UByte = if (muted) 1u else 0u
        return try {
            sealer.sealAudio(seq, flags, frame.copyOfRange(3, frame.size))
        } catch (err: EnetException.Crypto) {
            if (!err.reason.endsWith("too_large")) {
                step(VoiceCall.Seal.Event.Sealed(false))
                return null
            }
            dropped += 1
            runCatching { sealer.sealAudio(seq, flags, ByteArray(0)) }.getOrNull()
        }
    }

    /** The call is over: the timers stop, the key source is abandoned. */
    fun close(): Unit = synchronized(lock) {
        helloJob?.cancel()
        timerJob?.cancel()
        keyJob?.cancel()
        secret?.fill(0)
        secret = null
        seal = null
        state = VoiceCall.Seal.State(VoiceCall.Seal.Phase.Over)
    }

    private companion object {
        const val TAG = "VoiceCall"

        /** A well-formed host hello (length, version, role): its root (lowercase) and epoch. */
        fun parseHostHello(hello: ByteArray): Pair<String, Int>? {
            if (hello.size != VoiceCall.Seal.HOST_HELLO_BYTES || hello[0].toInt() != 1 || hello[1].toInt() != VoiceCall.Seal.ROLE_HOST) return null
            val buffer = ByteBuffer.wrap(hello, 2, 20)
            val epoch = buffer.int.toLong() and 0xffffffffL
            if (epoch > Int.MAX_VALUE) return null
            return UUID(buffer.long, buffer.long).toString() to epoch.toInt()
        }
    }
}

/**
 * #EXP-1941: the capture thread's ONE gate. Today's call ([line] null) sends
 * the clear frame; a sealed call sends only what the sealer made of it, and
 * nothing before the host's proof opened.
 */
internal fun uplinkFrame(line: CallSealLine?, frame: ByteArray, muted: Boolean): ByteArray? =
    if (line == null) frame else line.sealUplink(frame, muted)
