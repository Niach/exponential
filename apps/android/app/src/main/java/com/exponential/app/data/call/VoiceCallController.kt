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
import com.exponential.app.data.auth.AuthRepository
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
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
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
 * [VoiceCallService] holds the microphone foreground service from
 * `call_ready` on, so a call keeps running with the screen locked or another
 * app in front; everything is released on every end. Signing out or
 * switching accounts ends the call.
 */
@Singleton
class VoiceCallController @Inject constructor(
    @ApplicationContext private val context: Context,
    private val auth: AuthRepository,
    private val voiceApi: VoiceApi,
    @Named(STEER_CLIENT) private val client: HttpClient,
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
     * Call [agentId] ([agentName] for the copy) on the voice device named
     * [device]; [language] picks the greeting the device answers with.
     */
    fun start(
        agentId: String,
        agentName: String,
        device: String,
        language: String? = null,
        agentIcon: String? = null,
        agentColor: String? = null,
    ) {
        if (current != null || _ui.value.state.phase != VoiceCall.Phase.Hidden) return
        _ui.update { it.copy(agent = agentName, agentIcon = agentIcon, agentColor = agentColor, toast = null, answeredAt = null) }
        step(VoiceCall.Event.Start(device, language))
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
        audio.ring()
        run.job = scope.launch { connect(run) }
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

    /** Mute / unmute: the uplink carries silence (the relay's idle keep-alive), replies still play. */
    fun toggleMute() {
        val run = current ?: return
        val muted = !_ui.value.state.muted
        run.audio?.muted = muted
        run.session?.outgoing?.trySend(Frame.Text(controlFrame("call_mute") { put("muted", muted) }))
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
        val ticket = try {
            voiceApi.startCall(accountId, run.agentId)
        } catch (t: Throwable) {
            if (t is CancellationException) throw t
            // A refusal names its reason (the availability copy): toast it as-is.
            if (current !== run) return
            _ui.update { it.copy(state = VoiceCall.CALL_IDLE, toast = trpcErrorMessage(t, VoiceCall.Copy.Failed.DROPPED)) }
            return finish(run)
        }
        val session = try {
            client.webSocketSession(urlString = dialUrl(ticket.url))
        } catch (t: Throwable) {
            if (t is CancellationException) throw t
            Log.w(TAG, "call dial failed: ${t.javaClass.simpleName}")
            return fail(run, "dropped")
        }
        if (current !== run) {
            runCatching { session.close() }
            return
        }
        run.session = session
        try {
            for (frame in session.incoming) {
                if (current !== run) break
                when (frame) {
                    is Frame.Text -> if (!onText(run, frame.readText())) break
                    is Frame.Binary -> {
                        val data = frame.readBytes()
                        if (data.size > 3 && data[0].toInt() == VoiceCall.Wire.KIND_DOWN) {
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

    /** One control frame; false = the call is over (stop reading). */
    private fun onText(run: Run, text: String): Boolean {
        val msg = runCatching { Json.parseToJsonElement(text).jsonObject }.getOrNull() ?: return true
        when (msg.str("t")) {
            "call_ready" -> {
                // S-3: the pair is up; the flow keeps ringing until the
                // host's `rang` + `ready` answer it. The microphone opens
                // now (the host's mic check reads its level during the ring).
                // The microphone service starts with the microphone (never
                // for a call refused before it rang).
                VoiceCallService.start(context, VoiceCall.Copy.call(_ui.value.agent))
                startAudio(run)
            }
            "call_event" -> {
                val event = (msg["event"] as? JsonObject)?.let(VoiceCall.Event::of) ?: return true
                // Only the host's flow events; the call's own lifecycle stays local.
                if (event !in HOST_EVENTS && event::class !in HOST_EVENT_CLASSES) return true
                if (event == VoiceCall.Event.SpeechStarted) run.audio?.flushPlayback()
                step(event)
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
                fail(run, msg.str("code") ?: "dropped")
                return false
            }
            // The room closed without a `call_end` first.
            "bye" -> {
                fail(run, msg.str("outcome") ?: "dropped")
                return false
            }
        }
        return true
    }

    private fun newAudio(run: Run) = VoiceCallAudio(
        context = context,
        onUplink = { frame -> if (current === run) run.session?.outgoing?.trySend(Frame.Binary(true, frame)) },
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

    private fun fail(run: Run, code: String) {
        if (current !== run) return
        run.settled = true
        // Tell the host, best-effort: the relay closes the room either way.
        run.session?.outgoing?.trySend(Frame.Text(controlFrame("call_end") { put("reason", "dropped") }))
        step(VoiceCall.Event.Failed(code))
        finish(run)
    }

    /** Release the run: audio, socket, the foreground service. */
    private fun finish(run: Run) {
        if (current !== run) return
        current = null
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
        VoiceCallService.stop(context)
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
        val HOST_EVENTS = setOf(
            VoiceCall.Event.Rang,
            VoiceCall.Event.Ready,
            VoiceCall.Event.NoSound,
            VoiceCall.Event.SpeechStarted,
            VoiceCall.Event.ReplyFinished,
        )
        val HOST_EVENT_CLASSES = setOf(
            VoiceCall.Event.Partial::class,
            VoiceCall.Event.Said::class,
            VoiceCall.Event.Queued::class,
            VoiceCall.Event.ReplyStarted::class,
        )

        fun controlFrame(t: String, extra: kotlinx.serialization.json.JsonObjectBuilder.() -> Unit = {}): String =
            buildJsonObject {
                put("t", t)
                extra()
            }.toString()

        fun JsonObject.str(key: String): String? = (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content
    }
}
