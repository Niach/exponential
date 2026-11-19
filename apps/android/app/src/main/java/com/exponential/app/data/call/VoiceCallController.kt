package com.exponential.app.data.call

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.os.SystemClock
import android.util.Log
import androidx.core.content.ContextCompat
import com.exponential.app.BuildConfig
import com.exponential.app.data.api.STEER_CLIENT
import com.exponential.app.data.api.VoiceApi
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.api.SessionsApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.enet.EnetNodeManager
import com.exponential.app.data.sealed.SEAL_HOLDER_WAIT_MS
import com.exponential.app.data.sealed.SealedKeys
import com.exponential.app.data.steer.SteerTestHooks
import com.exponential.app.data.steer.withRelayAuthority
import com.exponential.app.domain.VoiceCall
import dagger.hilt.android.qualifiers.ApplicationContext
import io.ktor.client.HttpClient
import io.ktor.client.plugins.websocket.DefaultClientWebSocketSession
import io.ktor.client.plugins.websocket.webSocketSession
import io.ktor.websocket.Frame
import io.ktor.websocket.close
import io.ktor.websocket.readBytes
import io.ktor.websocket.readText
import javax.inject.Inject
import javax.inject.Named
import javax.inject.Singleton
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.flow.updateAndGet
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put

/**
 * NEW-69 (voice V5): THE phone's call — one at a time, app-wide, so it
 * outlives the chat that started it. `fixtures/voice-call.json` TRANSPORT:
 * `voice.startCall({agentId, platform: "phone"})` mints the caller ticket,
 * the ticket URL is dialled as-is on the steer client, the relay rings the
 * voice device and both get `call_ready`. S-3: the call RINGS first: a local
 * ringback plays from the press until the host answers (its `rang` + `ready`
 * `call_event`s; `call_ready` alone answers nothing). Text frames drive
 * [VoiceCall.callStep] (`call_event` carries the host's flow events,
 * speechStarted also flushes the playback); audio rides binary frames
 * ([VoiceCallAudio]). A failure ends the call with the state's toast; a
 * refused `startCall` ends it with the server's message.
 *
 * [VoiceCallService] holds the microphone foreground service from the tap
 * on (#EXP-1942: the app is in front then, so Android allows it), so a call
 * keeps running, and answers, with the screen locked or another app in
 * front (a service Android refuses fails the call `dropped`); everything is
 * released on every end. Signing out or
 * switching accounts ends the call.
 *
 * #EXP-1941: A SEALED CALL. With a key holder the caller claims `seal`; when
 * `voice.startCall` answers `seal: true` the call speaks the sealed wire
 * ([CallSealLine]): the caller's hello after `call_ready`, the host's hello
 * with its tag, then every microphone packet as ONE sealed frame of one size
 * with mute inside (no `call_mute`), the host's events and audio opened
 * before anything reads them. The capture thread seals only through a
 * `CallSeal`, which exists only after the host's tag opened. Every seal
 * failure reads "The call dropped"; a code about this phone's own device is
 * never taken from the wire.
 */
@Singleton
class VoiceCallController @Inject constructor(
    @ApplicationContext private val context: Context,
    private val auth: AuthRepository,
    private val voiceApi: VoiceApi,
    @Named(STEER_CLIENT) private val client: HttpClient,
    private val nodes: dagger.Lazy<EnetNodeManager>,
    private val sessions: dagger.Lazy<SessionsApi>,
    private val databases: DatabaseHolder,
) {
    /** The call as the surface reads it; [toast] is raised once ([consumeToast]). */
    data class Ui(
        val state: VoiceCall.State = VoiceCall.CALL_IDLE,
        /** The agent's name (the notification's `Call {agent}`). */
        val agent: String = "",
        /** S-3: the agent's mark on the call's avatar (null = the Exponential mark). */
        val agentIcon: String? = null,
        val agentColor: String? = null,
        val toast: String? = null,
        /** When the host answered (elapsedRealtime ms): the call's timer; null while ringing. */
        val answeredAt: Long? = null,
        /** EXP-1681: the call screen is minimized into the floating pill. */
        val minimized: Boolean = false,
        /** FEED-272: the tab on top of the call (a call opens on Voice). */
        val tab: VoiceCall.Tab = VoiceCall.Tabs.INITIAL,
        /** The called agent's definition id (its chat is the Chat tab's). */
        val agentId: String? = null,
    ) {
        val view: VoiceCall.View? get() = VoiceCall.callView(state, VoiceCall.Platform.Phone)
    }

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val _ui = MutableStateFlow(Ui())
    val ui: StateFlow<Ui> = _ui.asStateFlow()

    /** The wave's level: the microphone while listening, the reply while speaking. */
    private val _level = MutableStateFlow(0f)
    val level: StateFlow<Float> = _level.asStateFlow()

    /** One running call; every callback checks it is still [current]. */
    private inner class Run(val agentId: String, val accountId: String?) {
        var job: Job? = null
        /** S-3: fails a ring that outlasts `maxRingMs`. */
        var ringJob: Job? = null
        var session: DefaultClientWebSocketSession? = null
        var audio: VoiceCallAudio? = null
        /** call_end / error arrived, or End call was pressed: a close is no drop. */
        @Volatile var settled = false
        /** EXP-1929: the relay beats (`call_alive`), and when it last sent anything. */
        @Volatile var alive = false
        @Volatile var heardAt = SystemClock.elapsedRealtime()
        var liveJob: Job? = null
        /** #EXP-1941: the sealed line of a call the server marked sealed, else null (today's wire). */
        @Volatile var line: CallSealLine? = null
    }

    @Volatile private var current: Run? = null

    init {
        // A call belongs to the account that started it: sign-out or an
        // account switch ends it.
        scope.launch {
            auth.activeAccountId.collect { id ->
                val run = current ?: return@collect
                if (run.accountId != id) end()
            }
        }
    }

    private fun step(event: VoiceCall.Event) {
        val ui = _ui.updateAndGet { ui ->
            val next = VoiceCall.callStep(ui.state, event)
            val answeredAt = when (next.phase) {
                VoiceCall.Phase.Hidden, VoiceCall.Phase.Ringing -> null
                else -> ui.answeredAt ?: SystemClock.elapsedRealtime()
            }
            ui.copy(state = next, toast = next.toast ?: ui.toast, answeredAt = answeredAt)
        }
        // The ringback lasts while the call rings (answered, ended or failed stops it).
        if (ui.state.phase != VoiceCall.Phase.Ringing) current?.audio?.stopRing()
    }

    fun consumeToast() {
        _ui.update { it.copy(toast = null) }
    }

    fun hasMicPermission(): Boolean =
        ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED

    /** The RECORD_AUDIO request was refused: the call ends at once (`not_granted`). */
    fun permissionDenied(device: String, language: String? = null) {
        if (current != null) return
        step(VoiceCall.Event.Start(device, language))
        step(VoiceCall.Event.Failed("not_granted"))
    }

    /**
     * Call [agentId] ([agentName] for the copy) on the desktop named
     * [device] (EXP-1857 `callTarget`); [language] = the agent's voice
     * language; [fallback] = that
     * desktop is not the agent's own (the surface's `via` line names it).
     */
    fun start(
        agentId: String,
        agentName: String,
        device: String,
        language: String? = null,
        agentIcon: String? = null,
        agentColor: String? = null,
        fallback: Boolean = false,
    ) {
        if (current != null || _ui.value.state.phase != VoiceCall.Phase.Hidden) return
        // A new call always opens full screen.
        _ui.update { it.copy(agent = agentName, agentIcon = agentIcon, agentColor = agentColor, toast = null, answeredAt = null, minimized = false, tab = VoiceCall.Tabs.INITIAL, agentId = agentId) }
        step(VoiceCall.Event.Start(device, language, fallback))
        if (!hasMicPermission()) return step(VoiceCall.Event.Failed("not_granted"))
        // No Opus encoder below API 29 (MediaCodec), or no microphone at all.
        if (!VoiceCallAudio.isSupported(context)) return step(VoiceCall.Event.Failed("no_microphone"))
        val run = Run(agentId, auth.activeAccountId.value)
        current = run
        // S-3: the caller rings at once, on the call's own routing (focus
        // refused = another call holds the audio = dropped).
        val audio = newAudio(run)
        run.audio = audio
        audio.speaker = _ui.value.state.speaker
        if (!audio.prepare()) return fail(run, "dropped")
        // #EXP-1942: the microphone service is held from the tap, the one
        // moment the app is surely in front, so the host may answer while
        // another app is. A start Android refuses fails the call (#EXP-1940).
        val begun = beginCall(
            hold = {
                VoiceCallService.start(context, VoiceCall.Copy.call(agentName)) {
                    scope.launch { if (current === run) fail(run, "dropped") }
                }
            },
            ring = audio::ring,
            dial = { run.job = scope.launch { connect(run) } },
        )
        if (!begun) return fail(run, "dropped")
        scope.launch { pumpLevel(run) }
        run.ringJob = scope.launch { ringTimeout(run) }
    }

    /** A voice still not ready after `maxRingMs` fails the call `unavailable`. */
    private suspend fun ringTimeout(run: Run) {
        delay(VoiceCall.Constants.MAX_RING_MS.toLong())
        if (current === run && _ui.value.state.phase == VoiceCall.Phase.Ringing) fail(run, "unavailable")
    }

    /** End call: `call_end` to the relay, then close. */
    fun end() {
        val run = current ?: return step(VoiceCall.Event.End)
        run.settled = true
        run.session?.outgoing?.trySend(Frame.Text(controlFrame("call_end")))
        step(VoiceCall.Event.End)
        finish(run)
    }

    /** EXP-1681: Minimize (the chevron, back): the screen goes, the floating pill shows, the call goes on. */
    fun minimize() {
        if (_ui.value.state.phase == VoiceCall.Phase.Hidden) return
        _ui.update { it.copy(minimized = true) }
    }

    /** A tap on the pill: the call screen again (on Voice). */
    fun expand() {
        _ui.update { it.copy(minimized = false, tab = VoiceCall.Tab.Voice) }
    }

    /** FEED-272: Chat / Voice; never ends, mutes or re-dials the call. */
    fun showTab(tab: VoiceCall.Tab) {
        if (_ui.value.state.phase == VoiceCall.Phase.Hidden) return
        _ui.update { it.copy(tab = tab, minimized = false) }
    }

    /** FEED-272: the agent chats on screen now (each registers while composed). */
    private val _chatsShown = MutableStateFlow<Map<String, Int>>(emptyMap())
    val chatsShown: StateFlow<Map<String, Int>> = _chatsShown.asStateFlow()

    fun chatShown(agentId: String, shown: Boolean) {
        val left = _chatsShown.updateAndGet { map ->
            val n = (map[agentId] ?: 0) + if (shown) 1 else -1
            if (n <= 0) map - agentId else map + (agentId to n)
        }
        // Leaving the call's chat on its Chat tab minimizes the call into the pill.
        val ui = _ui.value
        if (!shown && !left.containsKey(agentId) && ui.agentId == agentId && ui.tab == VoiceCall.Tab.Chat) minimize()
    }

    /** Mute / unmute: the uplink carries silence (the relay's idle keep-alive), replies still play. */
    fun toggleMute() {
        val run = current ?: return
        val muted = !_ui.value.state.muted
        run.audio?.muted = muted
        // A sealed call sends no `call_mute`: mute rides inside every sealed frame.
        if (run.line == null) run.session?.outgoing?.trySend(Frame.Text(controlFrame("call_mute") { put("muted", muted) }))
        step(if (muted) VoiceCall.Event.Mute else VoiceCall.Event.Unmute)
    }

    /** S-3: the loudspeaker vs the earpiece, device-local (the host never hears of it). */
    fun toggleSpeaker() {
        val on = !_ui.value.state.speaker
        current?.audio?.speaker = on
        step(VoiceCall.Event.Speaker(on))
    }

    private suspend fun connect(run: Run) {
        val accountId = run.accountId ?: return fail(run, "dropped")
        val keys = callKeys(accountId)
        var target = CallKeyTarget(null, null)
        val ticket = try {
            // A key holder still starting is waited for, then the caller claims.
            val claim = runCatching { keys.ready() }.getOrDefault(false)
            if (current !== run) return
            coroutineScope {
                val asked = async { if (claim) runCatching { keys.target(run.agentId) }.getOrDefault(target) else target }
                voiceApi.startCall(accountId, run.agentId, seal = claim).also {
                    target = asked.await()
                    // Told sealed without the claim or the ids: no clear call instead.
                    if (it.seal == true && (!claim || it.deviceId == null)) error("a sealed call this caller cannot seal")
                }
            }
        } catch (t: Throwable) {
            if (t is CancellationException) throw t
            // A refusal names its reason (the availability copy): toast it as-is.
            if (current !== run) return
            _ui.update { it.copy(state = VoiceCall.CALL_IDLE, toast = trpcErrorMessage(t, VoiceCall.Copy.Failed.DROPPED)) }
            return finish(run)
        }
        // EXP-1857: the server picked the target itself; its answer names it
        // (an older server answers neither field: keep ours).
        if (current === run && (ticket.device != null || ticket.fallback != null)) {
            _ui.update { ui ->
                if (ui.state.phase == VoiceCall.Phase.Hidden) {
                    ui
                } else {
                    ui.copy(state = ui.state.copy(device = ticket.device ?: ui.state.device, fallback = ticket.fallback ?: ui.state.fallback))
                }
            }
        }
        if (current !== run) return
        val session = try {
            dialCall({ client.webSocketSession(urlString = dialUrl(ticket.url)) }, { current === run }) { it.close() }
        } catch (t: Throwable) {
            if (t is CancellationException) throw t
            Log.w(TAG, "call dial failed: ${t.javaClass.simpleName}")
            return fail(run, "dropped")
        } ?: return
        run.session = session
        if (ticket.seal == true) {
            run.line = try {
                CallSealLine(
                    callId = ticket.callId, agentId = run.agentId, deviceId = ticket.deviceId!!, keys = keys, target = target, scope = scope,
                    sendBinary = { frame -> if (current === run) session.outgoing.trySend(Frame.Binary(true, frame)) },
                    end = { reason -> session.outgoing.trySend(Frame.Text(controlFrame("call_end") { put("reason", reason) })) },
                    fail = { code -> if (current === run) fail(run, code, tell = false) },
                )
            } catch (t: Throwable) {
                return fail(run, "dropped")
            }
        }
        run.heardAt = SystemClock.elapsedRealtime()
        run.liveJob = scope.launch { watchLiveness(run) }
        try {
            for (frame in session.incoming) {
                if (current !== run) break
                run.heardAt = SystemClock.elapsedRealtime()
                when (frame) {
                    is Frame.Text -> if (!onText(run, frame.readText())) break
                    is Frame.Binary -> {
                        val data = frame.readBytes()
                        val line = run.line
                        if (line != null) {
                            // Opened here, in arrival order; a frame with no packet reaches no decoder.
                            line.frame(data)?.packet?.takeIf { it.isNotEmpty() }?.let { run.audio?.pushDownlink(it) }
                        } else if (data.size > 3 && data[0].toInt() == VoiceCall.Wire.KIND_DOWN) {
                            run.audio?.pushDownlink(data.copyOfRange(3, data.size))
                        }
                    }
                    else -> Unit
                }
            }
        } catch (t: Throwable) {
            if (t is CancellationException && !run.settled) throw t
        }
        // The socket closed or failed without call_end / error: dropped.
        if (current === run && !run.settled) fail(run, "dropped") else if (current === run) finish(run)
    }

    /** EXP-1929: once the relay beat, a path that carries nothing for `idleTimeoutMs` is dead: dropped. */
    private suspend fun watchLiveness(run: Run) {
        while (current === run) {
            delay(LIVENESS_TICK_MS)
            if (current === run && VoiceCall.callPathDead(run.alive, SystemClock.elapsedRealtime() - run.heardAt)) {
                Log.i(TAG, "call path went silent")
                return fail(run, "dropped")
            }
        }
    }

    /** One control frame; false = the call is over (stop reading). */
    private fun onText(run: Run, text: String): Boolean {
        val msg = runCatching { Json.parseToJsonElement(text).jsonObject }.getOrNull() ?: return true
        when (msg.str("t")) {
            "call_ready" -> {
                // S-3: the pair is up; the flow keeps ringing until the
                // host's `rang` + `ready` answer it. The microphone opens
                // now (the host's mic check reads its level during the ring),
                // under the microphone service held since the tap
                // (#EXP-1942), whichever app is in front.
                startAudio(run)
                run.line?.ready()
            }
            "call_event" -> {
                val raw = msg["event"] as? JsonObject ?: return true
                val line = run.line
                if (line != null && raw.str("t") != "failed") {
                    // A sealed call: the host's hello, or ONE control frame of today sealed; clear content ends it.
                    val inner = line.event(raw.str("t"), raw.str("data")) ?: return current === run
                    val opened = runCatching { Json.parseToJsonElement(inner).jsonObject }.getOrNull()
                    if (opened?.str("t") != "call_event") return true
                    return hostEvent(run, opened["event"] as? JsonObject ?: return true)
                }
                return hostEvent(run, raw)
            }
            "call_end" -> {
                // hangUpRules: a reason drops the call with its toast, none ends it.
                run.settled = true
                step(VoiceCall.Event.HungUp(msg.str("reason")))
                finish(run)
                return false
            }
            "error" -> {
                run.settled = true
                fail(run, VoiceCall.Seal.wireCode(msg.str("code") ?: "dropped"))
                return false
            }
            "call_alive" -> run.alive = true
            // The room closed without a `call_end` first.
            "bye" -> {
                fail(run, VoiceCall.Seal.wireCode(msg.str("outcome") ?: "dropped"))
                return false
            }
        }
        return true
    }

    /** One flow event of the host (clear in today's call, opened in a sealed one); false = the call is over. */
    private fun hostEvent(run: Run, raw: JsonObject): Boolean {
        val event = VoiceCall.Event.of(raw) ?: return true
        // Only the host's flow events; the call's own lifecycle stays local.
        if (!event.isHostEvent) return true
        // The host could not get the call ready: it fails the call
        // with its code before its `call_end` (ignored: the run is over).
        if (event is VoiceCall.Event.Failed) {
            run.settled = true
            step(VoiceCall.Event.Failed(VoiceCall.Seal.wireCode(event.code)))
            finish(run)
            return false
        }
        if (event == VoiceCall.Event.SpeechStarted) run.audio?.flushPlayback()
        step(event)
        return true
    }

    /** #EXP-1941: the chat's key of a sealed call, from this install's key holder for [accountId]. */
    private fun callKeys(accountId: String) = object : CallKeySource {
        // Waited for only when a call CAN be sealed: one of the member's
        // synced devices advertises the cap. Until a host does, a phone
        // without a holder dials at once, as it always did.
        override suspend fun ready(): Boolean {
            val manager = nodes.get()
            if (manager.awaitHolder(accountId, 0L) != null) return true
            val sealable = databases.database(accountId).deviceDao().observeAll().first().any { it.caps?.contains("\"${VoiceCall.Seal.CAP}\"") == true }
            return sealable && manager.awaitHolder(accountId, SEAL_HOLDER_WAIT_MS) != null
        }
        override suspend fun target(agentId: String): CallKeyTarget =
            sessions.get().sendTarget(accountId, agentId, null).let { CallKeyTarget(it.rootId, it.epoch, it.rotationDue) }
        // Not throttled: a call waits on this pull, and it nudges the devices that hold the key.
        override suspend fun secret(rootId: String, epoch: Int): ByteArray? =
            SealedKeys.of(rootId)[epoch]?.copyOf() ?: nodes.get().pullTreeKey(accountId, rootId, epoch)
        override fun highestEpoch(rootId: String): Int? = SealedKeys.of(rootId).keys.maxOrNull()
    }

    private fun newAudio(run: Run) = VoiceCallAudio(
        context = context,
        // The capture thread's ONE gate: a sealed call's frame leaves only through the sealer.
        onUplink = { frame ->
            if (current === run) uplinkFrame(run.line, frame, _ui.value.state.muted)?.let { run.session?.outgoing?.trySend(Frame.Binary(true, it)) }
        },
        onFailure = { code -> scope.launch { if (current === run) fail(run, code) } },
    )

    private fun startAudio(run: Run) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) return
        val audio = run.audio ?: newAudio(run).also { run.audio = it }
        audio.speaker = _ui.value.state.speaker
        audio.muted = _ui.value.state.muted
        audio.sending = true
        try {
            audio.start()
        } catch (t: Throwable) {
            Log.w(TAG, "call audio did not start", t)
            fail(run, "dropped")
        }
    }

    private fun fail(run: Run, code: String, tell: Boolean = true) {
        if (current !== run) return
        run.settled = true
        // Tell the host, best-effort: the relay closes the room either way.
        if (tell) run.session?.outgoing?.trySend(Frame.Text(controlFrame("call_end") { put("reason", "dropped") }))
        step(VoiceCall.Event.Failed(code))
        finish(run)
    }

    /** Release the run: audio, socket, the foreground service. */
    private fun finish(run: Run) {
        if (current !== run) return
        current = null
        run.line?.close()
        run.line = null
        run.audio?.release()
        run.audio = null
        _level.value = 0f
        val session = run.session
        run.session = null
        scope.launch {
            withContext(NonCancellable) { runCatching { session?.close() } }
            session?.cancel()
            run.job?.cancel()
        }
        run.ringJob?.cancel()
        run.liveJob?.cancel()
        VoiceCallService.stop()
    }

    /** The wave's level at ~30 fps while [run] lasts. */
    private suspend fun pumpLevel(run: Run) {
        while (scope.isActive && current === run) {
            val audio = run.audio
            _level.value = when (_ui.value.view?.activity) {
                VoiceCall.Activity.Listening -> audio?.micLevel ?: 0f
                VoiceCall.Activity.Speaking -> audio?.playLevel ?: 0f
                else -> 0f
            }
            delay(LEVEL_TICK_MS)
        }
        _level.value = 0f
    }

    private fun dialUrl(url: String): String =
        SteerTestHooks.relayAuthority
            ?.takeIf { BuildConfig.DEBUG }
            ?.let { withRelayAuthority(url, it) }
            ?: url

    private companion object {
        const val TAG = "VoiceCall"
        const val LEVEL_TICK_MS = 33L
        const val LIVENESS_TICK_MS = 1_000L
        fun controlFrame(t: String, extra: kotlinx.serialization.json.JsonObjectBuilder.() -> Unit = {}): String =
            buildJsonObject {
                put("t", t)
                extra()
            }.toString()

        fun JsonObject.str(key: String): String? = (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content
    }
}

/**
 * #EXP-1940: the dial of a call. Ktor finishes a WebSocket handshake on the
 * client's own coroutine, so a cancelled caller would leave a connected
 * socket nobody holds (the relay rings the desktop for it). The wait
 * therefore outlives a cancel, and a socket that connects once the call is
 * no longer [wanted] is [close]d here; null = that happened.
 */
internal suspend fun <S : Any> dialCall(dial: suspend () -> S, wanted: () -> Boolean, close: suspend (S) -> Unit): S? =
    withContext(NonCancellable) {
        val session = dial()
        if (wanted()) {
            session
        } else {
            runCatching { close(session) }
            null
        }
    }

/**
 * #EXP-1942: a call's start, in order: the microphone service is held
 * BEFORE the call rings and dials; false = Android refused it, nothing rang
 * and nothing was dialled (the caller fails the call `dropped`).
 */
internal fun beginCall(hold: () -> Boolean, ring: () -> Unit, dial: () -> Unit): Boolean {
    if (!hold()) return false
    ring()
    dial()
    return true
}
