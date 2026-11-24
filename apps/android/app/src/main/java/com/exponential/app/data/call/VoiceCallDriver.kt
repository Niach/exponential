package com.exponential.app.data.call

import com.exponential.app.data.api.StartCallResult
import com.exponential.app.domain.VoiceCall
import com.exponential.app.domain.VoiceCall.Caller.Effect
import com.exponential.app.domain.VoiceCall.Caller.Frame
import com.exponential.app.domain.VoiceCall.Caller.Input
import com.exponential.app.domain.VoiceCall.Caller.Outgoing
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject

/** One dialled socket (ktor's WebSocket session in the app). */
interface CallSocket {
    /** A control frame. */
    fun send(text: String)
    /** The call's last control frame: the socket closes after it. */
    fun sendLast(text: String)
    fun sendBinary(frame: ByteArray)
    /** Reads every frame, in arrival order, until the socket goes. */
    suspend fun receive(onText: (String) -> Unit, onBinary: (ByteArray) -> Unit, onOther: () -> Unit)
    fun close()
}

/** A sealed call's line ([CallSealLine]). */
interface CallSeal {
    fun ready()
    /** A host `call_event` that is not `failed`: the control frame it opened, else null. */
    fun event(t: String?, data: String?): String?
    /** A binary frame's packet; null = dropped. */
    fun frame(bytes: ByteArray): ByteArray?
    /** THE capture path's gate: null = the frame does not leave. */
    fun uplink(frame: ByteArray, muted: Boolean): ByteArray?
    fun close()
}

/** Something that runs later and can be called off. */
fun interface CallTimer {
    fun cancel()
}

/** What a call needs from Android. */
interface CallPlatform {
    /**
     * Before the call rings (#EXP-1942): the grant, a microphone, the audio
     * focus, the microphone service; [answer] once (a refusal with its code).
     * [report] = an own failure Android tells later (the service refused,
     * the audio failed).
     */
    fun hold(speaker: Boolean, report: (code: String) -> Unit, answer: (ok: Boolean, code: String?) -> Unit)
    fun ring()
    fun stopRing()
    fun setSpeaker(on: Boolean)
    fun setMuted(muted: Boolean)
    /** Barge-in: the buffered reply goes. */
    fun flush()
    /** The microphone and the playback; throws when they do not start. */
    fun startAudio(muted: Boolean, speaker: Boolean)
    /** One down packet for the player. */
    fun play(packet: ByteArray)
    fun toast(text: String)
    /** Everything Android holds for the call: the audio, the microphone service. */
    fun release()
    suspend fun keysReady(): Boolean
    suspend fun keyTarget(agentId: String): CallKeyTarget
    suspend fun startCall(agentId: String, seal: Boolean): StartCallResult
    /** A refused `startCall` in this platform's words. */
    fun refusal(error: Throwable): String
    /** EXP-1940: a socket that connects once [wanted] says no is closed there (null). */
    suspend fun dial(url: String, wanted: () -> Boolean): CallSocket?
    /** null = this build cannot seal. */
    fun makeSeal(ticket: StartCallResult, agentId: String, target: CallKeyTarget, socket: CallSocket, end: (String) -> Unit, fail: (String) -> Unit): CallSeal?
    fun after(ms: Long, body: () -> Unit): CallTimer
    fun every(ms: Long, body: () -> Unit): CallTimer
    fun nowMs(): Long
    fun log(message: String)
}

/**
 * #EXP-1993: the phone's call as the app runs it. [VoiceCall.Caller]
 * decides (voice-call.json `caller`); the driver does each effect through
 * [CallPlatform] and the call's [CallSocket] / [CallSeal], in order, turns
 * what they report into the machine's inputs and drops whatever a call
 * reports after its end (the generation). [VoiceCallController] is the
 * Android glue behind the ports; `VoiceCallDriverTest` replays every
 * `caller` case through this driver with fakes and asserts the calls it
 * makes. Inputs from any thread are stepped one at a time, effects in the
 * order the machine gave them.
 */
class VoiceCallDriver(private val platform: CallPlatform, private val scope: CoroutineScope) {
    private val lock = Any()

    /** The caller's session: the flow and its stage. */
    @Volatile var caller: VoiceCall.Caller.State = VoiceCall.Caller.IDLE
        private set

    /** Every state the machine steps to (under the driver's lock). */
    var onState: (VoiceCall.Caller.State) -> Unit = {}

    private var generation = 0
    private val inbox = ArrayDeque<Input>()
    private var stepping = false
    private var agentId: String? = null
    private var ticket: StartCallResult? = null
    private var target = CallKeyTarget(null, null)
    private var askJob: Job? = null
    private var dialJob: Job? = null
    private var ringTimer: CallTimer? = null
    private var watchTimer: CallTimer? = null
    @Volatile private var socket: CallSocket? = null
    /** The socket the microphone's frames go to: set once the line (if any) exists. */
    @Volatile private var uplinkSocket: CallSocket? = null
    @Volatile private var seal: CallSeal? = null
    /** EXP-1929: the relay beats (`call_alive`), and when it last sent anything. */
    @Volatile private var alive = false
    @Volatile private var heardAt = 0L

    // #FEED-268: the call's log lines (logcat tag `VoiceCall`): its id, the
    // stages it passed, counts and the reason it ended. Never a word of the
    // call, never a name.
    private var startedAt = 0L
    private var answered = false
    @Volatile private var logId: String? = null
    @Volatile private var upFrames = 0L
    @Volatile private var downFrames = 0L
    @Volatile private var lastDownAt = 0L

    private fun say(message: String) = platform.log("call ${logId ?: "(no id yet)"}: $message")

    private fun sinceStart(): Long = platform.nowMs() - startedAt

    /** What an input is in the log (null = not a line: captions, beats, mute, speaker). */
    private fun logged(input: Input): String? = when (input) {
        is Input.Start -> "started (fallback ${input.fallback})"
        is Input.Held -> if (input.ok) "the microphone is held" else "the microphone was refused (${input.code ?: "dropped"})"
        is Input.Ticket -> null
        is Input.Refused -> "startCall refused: ${input.message}"
        is Input.Received -> when (val frame = input.frame) {
            Frame.Ready -> "call_ready: the host joined after ${sinceStart()} ms"
            is Frame.End -> "call_end from the other side (reason ${frame.reason ?: "none: it hung up"})"
            is Frame.Error -> "the relay refused the call (${frame.code})"
            is Frame.Bye -> "bye from the relay (${frame.outcome})"
            is Frame.HostEvent -> when (val event = frame.event) {
                VoiceCall.Event.Rang -> "the host rang"
                VoiceCall.Event.Ready -> "the host's voice is ready"
                VoiceCall.Event.NoSound -> "the host hears no sound from this microphone"
                is VoiceCall.Event.Failed -> "the host failed the call (${event.code})"
                VoiceCall.Event.End, is VoiceCall.Event.HungUp -> "the host ended the call"
                else -> null
            }
            else -> null
        }
        Input.Closed -> "the socket closed without call_end"
        is Input.Failed -> "failed on this phone (${input.code})"
        Input.RingTimeout -> "the longest ring passed"
        Input.End -> "End call here (or the account changed)"
        else -> null
    }

    /** One step's lines: what happened, the answer, and ONE line for the end. */
    private fun log(input: Input, before: VoiceCall.Caller.State, stepped: VoiceCall.Caller.Stepped) {
        if (stepped.state == before && stepped.effects.isEmpty()) return
        if (input is Input.Start) {
            startedAt = platform.nowMs()
            answered = false
            logId = null
            upFrames = 0
            downFrames = 0
            lastDownAt = 0
        }
        val note = logged(input)
        val released = Effect.Release in stepped.effects
        if (released) {
            val lastDown = if (downFrames == 0L) "never" else "${platform.nowMs() - lastDownAt} ms ago"
            say(
                "ended: ${note ?: input::class.simpleName} stage=${before.stage} phase=${before.call.phase} answered=$answered " +
                    "lasted=${sinceStart()} ms sealed=${before.sealed} up_frames=$upFrames down_frames=$downFrames last_host_audio=$lastDown",
            )
            return
        }
        note?.let(::say)
        if (!answered && before.call.phase == VoiceCall.Phase.Ringing && stepped.state.call.phase != VoiceCall.Phase.Ringing && before.stage != VoiceCall.Caller.Stage.Idle) {
            answered = true
            say("answered after ${sinceStart()} ms")
        }
    }

    /** The tap: call [agentId] on the desktop named [device] (EXP-1857 `callTarget`). */
    fun start(agentId: String, device: String, language: String?, fallback: Boolean) {
        synchronized(lock) {
            if (caller.stage != VoiceCall.Caller.Stage.Idle) return
            generation += 1
            this.agentId = agentId
            send(Input.Start(device, language, fallback))
        }
    }

    /** End call (also an account switch). */
    fun end() = send(Input.End)

    fun toggleMute() = send(Input.Mute)

    fun toggleSpeaker() = send(Input.Speaker)

    /** One captured up frame (the capture thread): through the sealer when sealed, else as it is. */
    fun uplink(frame: ByteArray) {
        val socket = uplinkSocket ?: return
        val seal = seal
        (if (seal == null) frame else seal.uplink(frame, caller.call.muted))?.let {
            socket.sendBinary(it)
            upFrames += 1
            if (upFrames == 1L) say("the first microphone frame went out after ${sinceStart()} ms")
        }
    }

    /** Hands [input] to [VoiceCall.Caller]; an input of an ended call ([gen]) is dropped. */
    private fun send(input: Input, gen: Int? = null): Unit = synchronized(lock) {
        if (gen != null && gen != generation) return
        inbox.addLast(input)
        if (stepping) return
        stepping = true
        try {
            while (inbox.isNotEmpty()) {
                val input = inbox.removeFirst()
                val stepped = VoiceCall.Caller.step(caller, input)
                log(input, caller, stepped)
                caller = stepped.state
                onState(stepped.state)
                stepped.effects.forEach(::perform)
            }
        } finally {
            stepping = false
        }
    }

    private fun perform(effect: Effect) {
        val gen = generation
        when (effect) {
            Effect.Hold -> platform.hold(
                caller.call.speaker,
                report = { code -> send(Input.Failed(code), gen) },
                answer = { ok, code -> send(Input.Held(ok, code), gen) },
            )
            is Effect.Ring -> {
                platform.ring()
                ringTimer = platform.after(effect.ms.toLong()) { send(Input.RingTimeout, gen) }
            }
            Effect.Ticket -> {
                val agentId = agentId ?: return
                askJob = scope.launch { ask(gen, agentId) }
            }
            Effect.Dial -> dialJob = scope.launch { dial(gen) }
            Effect.StartAudio -> try {
                platform.startAudio(caller.call.muted, caller.call.speaker)
            } catch (t: Throwable) {
                say("the call audio did not start: ${t.javaClass.simpleName}")
                send(Input.Failed("dropped"), gen)
            }
            Effect.StopRing -> {
                ringTimer?.cancel()
                ringTimer = null
                platform.stopRing()
            }
            Effect.Flush -> platform.flush()
            is Effect.Mute -> platform.setMuted(effect.muted)
            is Effect.Speaker -> platform.setSpeaker(effect.on)
            is Effect.Send -> {
                val socket = socket ?: return
                if (effect.frame is Outgoing.End) {
                    this.socket = null
                    uplinkSocket = null
                    socket.sendLast(effect.frame.json.toString())
                } else {
                    socket.send(effect.frame.json.toString())
                }
            }
            Effect.Alive -> alive = true
            Effect.Release -> teardown()
            is Effect.Toast -> platform.toast(effect.text)
        }
    }

    /** The `ticket` effect: a key holder that is still starting, then `voice.startCall`. */
    private suspend fun ask(gen: Int, agentId: String) {
        var target = CallKeyTarget(null, null)
        val ticket = try {
            // A key holder still starting is waited for, then the caller claims.
            val claim = runCatching { platform.keysReady() }.getOrDefault(false)
            if (gen != generation) return
            coroutineScope {
                val asked = async { if (claim) runCatching { platform.keyTarget(agentId) }.getOrDefault(target) else target }
                platform.startCall(agentId, claim).also {
                    target = asked.await()
                    // Told sealed without the claim or the ids: no clear call instead.
                    if (it.seal == true && (!claim || it.deviceId == null)) error("a sealed call this caller cannot seal")
                }
            }
        } catch (t: Throwable) {
            if (t is CancellationException) throw t
            // A refusal names its reason (the availability copy): toasted as-is.
            return send(Input.Refused(platform.refusal(t)), gen)
        }
        synchronized(lock) {
            if (gen != generation) return
            this.ticket = ticket
            this.target = target
            logId = ticket.callId
        }
        say("ticket after ${sinceStart()} ms: rings device ${ticket.deviceId ?: "?"} (fallback ${ticket.fallback}, sealed ${ticket.seal == true})")
        // EXP-1857: the server's answer names the device that speaks.
        send(Input.Ticket(ticket.device, ticket.fallback, sealed = ticket.seal == true), gen)
    }

    private suspend fun dial(gen: Int) {
        val (ticket, target, agentId) = synchronized(lock) {
            Triple(ticket ?: return, target, agentId ?: return)
        }
        val socket = try {
            platform.dial(ticket.url) { gen == generation }
        } catch (t: Throwable) {
            if (t is CancellationException) throw t
            say("the dial failed: ${t.javaClass.simpleName}")
            return send(Input.Failed("dropped"), gen)
        } ?: return
        synchronized(lock) {
            if (gen != generation) return socket.close()
            this.socket = socket
        }
        say("the relay socket is open after ${sinceStart()} ms")
        val seal = if (ticket.seal == true) {
            runCatching {
                platform.makeSeal(
                    ticket, agentId, target, socket,
                    end = { reason -> socket.send(Outgoing.End(reason).json.toString()) },
                    // The line sent its own `call_end {reason}`.
                    fail = { code -> send(Input.Failed(code, told = true), gen) },
                )
            }.getOrNull() ?: run {
                say("the call cannot be sealed here (no key holder or no device id)")
                return send(Input.Failed("dropped"), gen)
            }
        } else {
            null
        }
        synchronized(lock) {
            if (gen != generation) return seal?.close() ?: Unit
            this.seal = seal
            uplinkSocket = socket
            heardAt = platform.nowMs()
            // EXP-1929: once the relay beat, a path that carries nothing for `idleTimeoutMs` is dead.
            watchTimer = platform.every(LIVENESS_TICK_MS) {
                if (VoiceCall.callPathDead(alive, platform.nowMs() - heardAt)) {
                    say("the path went silent: nothing from the relay for ${platform.nowMs() - heardAt} ms")
                    send(Input.Failed("dropped"), gen)
                }
            }
        }
        try {
            socket.receive(
                onText = { text ->
                    heardAt = platform.nowMs()
                    onText(gen, seal, text)
                },
                onBinary = { bytes ->
                    heardAt = platform.nowMs()
                    onBinary(gen, seal, bytes)
                },
                onOther = { heardAt = platform.nowMs() },
            )
        } catch (t: Throwable) {
            if (t is CancellationException && gen == generation) throw t
            // #FEED-268: why the socket went (the class, never a ticket URL).
            if (gen == generation) say("the socket failed: ${t.javaClass.simpleName}")
        }
        // The socket went away without `call_end` or `bye` (an ended call's is dropped).
        send(Input.Closed, gen)
    }

    /** One control frame, in arrival order; a sealed call's host events are opened first. */
    private fun onText(gen: Int, seal: CallSeal?, text: String) {
        val frame = runCatching { Json.parseToJsonElement(text).jsonObject }.getOrNull() ?: return
        if (seal != null && frame.str("t") == "call_event") {
            val raw = frame["event"] as? JsonObject
            if (raw != null && raw.str("t") != "failed") {
                // The host's hello, or ONE control frame of today sealed; clear content ends it.
                val inner = seal.event(raw.str("t"), raw.str("data")) ?: return
                val opened = runCatching { Json.parseToJsonElement(inner).jsonObject }.getOrNull()
                if (opened?.str("t") != "call_event") return
                return send(Input.Received(Frame.of(opened)), gen)
            }
        }
        send(Input.Received(Frame.of(frame)), gen)
        // The microphone opened on `call_ready`; the line offers its hello now.
        if (seal != null && frame.str("t") == "call_ready" && gen == generation) seal.ready()
    }

    private fun onBinary(gen: Int, seal: CallSeal?, bytes: ByteArray) {
        if (gen != generation) return
        downFrames += 1
        lastDownAt = platform.nowMs()
        if (downFrames == 1L) say("the host's first audio frame came after ${sinceStart()} ms")
        if (seal != null) {
            // Opened here, in arrival order; a frame with no packet reaches no decoder.
            seal.frame(bytes)?.takeIf { it.isNotEmpty() }?.let(platform::play)
        } else if (bytes.size > 3 && bytes[0].toInt() == VoiceCall.Wire.KIND_DOWN) {
            platform.play(bytes.copyOfRange(3, bytes.size))
        }
    }

    /** The `release` effect: everything goes; late reports of this call turn inert. */
    private fun teardown() {
        generation += 1
        agentId = null
        ticket = null
        target = CallKeyTarget(null, null)
        ringTimer?.cancel()
        ringTimer = null
        watchTimer?.cancel()
        watchTimer = null
        alive = false
        seal?.close()
        seal = null
        uplinkSocket = null
        platform.release()
        socket?.close()
        socket = null
        askJob?.cancel()
        askJob = null
        dialJob?.cancel()
        dialJob = null
    }

    private companion object {
        const val LIVENESS_TICK_MS = 1_000L

        fun JsonObject.str(key: String): String? = (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content
    }
}
