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
import com.exponential.app.data.api.StartCallResult
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
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
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
 *
 * #EXP-1993: all of that is [VoiceCallDriver] (on [VoiceCall.Caller],
 * voice-call.json `caller`, replayed with iOS's); this class is its Android
 * glue: the grant, [VoiceCallAudio], [VoiceCallService], ktor, the toast.
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

    /** Who calls, set by [start] / [permissionDenied] for the hold and the ticket. */
    private class Calling(val agentName: String, val accountId: String?, val denied: Boolean = false)

    @Volatile private var calling: Calling? = null
    /** The call's audio: made by the hold (the ring and the routing first), released by every end. */
    @Volatile private var audio: VoiceCallAudio? = null

    /** #EXP-1993: WHAT happens is [VoiceCallDriver] on [VoiceCall.Caller]; this class is its Android glue. */
    private val driver = VoiceCallDriver(AndroidCallPlatform(), scope).also { it.onState = ::stepped }

    init {
        // A call belongs to the account that started it: sign-out or an
        // account switch ends it.
        scope.launch {
            auth.activeAccountId.collect { id ->
                if (driver.caller.stage != VoiceCall.Caller.Stage.Idle && calling?.accountId != id) end()
            }
        }
    }

    /** Every state the driver steps to: the surface's, with the timer's zero. */
    private fun stepped(state: VoiceCall.Caller.State) {
        _ui.update { ui ->
            val answeredAt = when (state.call.phase) {
                VoiceCall.Phase.Hidden, VoiceCall.Phase.Ringing -> null
                else -> ui.answeredAt ?: SystemClock.elapsedRealtime()
            }
            ui.copy(state = state.call, answeredAt = answeredAt)
        }
    }

    fun consumeToast() {
        _ui.update { it.copy(toast = null) }
    }

    fun hasMicPermission(): Boolean =
        ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED

    /** The RECORD_AUDIO request was refused: the call ends at once (`not_granted`). */
    fun permissionDenied(device: String, language: String? = null) {
        if (driver.caller.stage != VoiceCall.Caller.Stage.Idle) return
        calling = Calling(agentName = "", accountId = auth.activeAccountId.value, denied = true)
        driver.start("", device, language, fallback = false)
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
        if (driver.caller.stage != VoiceCall.Caller.Stage.Idle || _ui.value.state.phase != VoiceCall.Phase.Hidden) return
        // A new call always opens full screen.
        _ui.update { it.copy(agent = agentName, agentIcon = agentIcon, agentColor = agentColor, toast = null, answeredAt = null, minimized = false, tab = VoiceCall.Tabs.INITIAL, agentId = agentId) }
        calling = Calling(agentName, auth.activeAccountId.value)
        driver.start(agentId, device, language, fallback)
    }

    /** End call: `call_end` to the relay, then close. */
    fun end() = driver.end()

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
    fun toggleMute() = driver.toggleMute()

    /** S-3: the loudspeaker vs the earpiece, device-local (the host never hears of it). */
    fun toggleSpeaker() = driver.toggleSpeaker()

    /** [VoiceCallDriver]'s ports on Android. */
    private inner class AndroidCallPlatform : CallPlatform {
        override fun hold(speaker: Boolean, report: (code: String) -> Unit, answer: (ok: Boolean, code: String?) -> Unit) {
            val call = calling ?: return answer(false, "dropped")
            if (call.denied || !hasMicPermission()) return answer(false, "not_granted")
            // No Opus encoder below API 29 (MediaCodec), or no microphone at all.
            if (!VoiceCallAudio.isSupported(context)) return answer(false, "no_microphone")
            // S-3: the caller rings at once, on the call's own routing (focus
            // refused = another call holds the audio = dropped).
            val audio = VoiceCallAudio(
                context = context,
                // The capture thread's ONE gate: a sealed call's frame leaves only through the sealer.
                onUplink = driver::uplink,
                onFailure = { code -> scope.launch { report(code) } },
            )
            this@VoiceCallController.audio = audio
            audio.speaker = speaker
            if (!audio.prepare()) return answer(false, "dropped")
            // #EXP-1942: the microphone service is held from the tap, the one
            // moment the app is surely in front, so the host may answer while
            // another app is. A start Android refuses fails the call (#EXP-1940).
            if (!VoiceCallService.start(context, VoiceCall.Copy.call(call.agentName)) { scope.launch { report("dropped") } }) {
                return answer(false, "dropped")
            }
            scope.launch { pumpLevel(audio) }
            answer(true, null)
        }

        override fun ring() {
            audio?.ring()
        }

        override fun stopRing() {
            audio?.stopRing()
        }

        override fun setSpeaker(on: Boolean) {
            audio?.speaker = on
        }

        override fun setMuted(muted: Boolean) {
            audio?.muted = muted
        }

        override fun flush() {
            audio?.flushPlayback()
        }

        override fun startAudio(muted: Boolean, speaker: Boolean) {
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) return
            // The pair is up; the flow keeps ringing until the host's `rang` +
            // `ready` answer it. The microphone opens now (the host's mic check
            // reads its level during the ring), under the microphone service
            // held since the tap (#EXP-1942), whichever app is in front.
            val audio = audio ?: return
            audio.speaker = speaker
            audio.muted = muted
            audio.sending = true
            audio.start()
        }

        override fun play(packet: ByteArray) {
            audio?.pushDownlink(packet)
        }

        override fun toast(text: String) {
            _ui.update { it.copy(toast = text) }
        }

        override fun release() {
            audio?.release()
            audio = null
            _level.value = 0f
            VoiceCallService.stop()
        }

        override suspend fun keysReady(): Boolean = keys()?.ready() ?: false

        override suspend fun keyTarget(agentId: String): CallKeyTarget = keys()?.target(agentId) ?: CallKeyTarget(null, null)

        override suspend fun startCall(agentId: String, seal: Boolean): StartCallResult {
            val accountId = calling?.accountId ?: error("no account")
            return voiceApi.startCall(accountId, agentId, seal = seal)
        }

        override fun refusal(error: Throwable): String = trpcErrorMessage(error, VoiceCall.Copy.Failed.DROPPED)

        override suspend fun dial(url: String, wanted: () -> Boolean): CallSocket? =
            dialCall({ client.webSocketSession(urlString = dialUrl(url)) }, wanted) { it.close() }?.let { KtorCallSocket(it) }

        override fun makeSeal(ticket: StartCallResult, agentId: String, target: CallKeyTarget, socket: CallSocket, end: (String) -> Unit, fail: (String) -> Unit): CallSeal? {
            val keys = keys() ?: return null
            val line = CallSealLine(
                callId = ticket.callId, agentId = agentId, deviceId = ticket.deviceId ?: return null, keys = keys, target = target, scope = scope,
                sendBinary = socket::sendBinary, end = end, fail = fail,
            )
            return object : CallSeal {
                override fun ready() = line.ready()
                override fun event(t: String?, data: String?): String? = line.event(t, data)
                override fun frame(bytes: ByteArray): ByteArray? = line.frame(bytes)?.packet
                override fun uplink(frame: ByteArray, muted: Boolean): ByteArray? = uplinkFrame(line, frame, muted)
                override fun close() = line.close()
            }
        }

        override fun after(ms: Long, body: () -> Unit): CallTimer {
            val job = scope.launch {
                delay(ms)
                body()
            }
            return CallTimer { job.cancel() }
        }

        override fun every(ms: Long, body: () -> Unit): CallTimer {
            val job = scope.launch {
                while (isActive) {
                    delay(ms)
                    body()
                }
            }
            return CallTimer { job.cancel() }
        }

        override fun nowMs(): Long = SystemClock.elapsedRealtime()

        override fun log(message: String) {
            Log.i(TAG, message)
        }
    }

    /** One call's ktor WebSocket session. */
    private inner class KtorCallSocket(private val session: DefaultClientWebSocketSession) : CallSocket {
        override fun send(text: String) {
            session.outgoing.trySend(Frame.Text(text))
        }

        override fun sendLast(text: String) {
            send(text)
            close()
        }

        override fun sendBinary(frame: ByteArray) {
            session.outgoing.trySend(Frame.Binary(true, frame))
        }

        override suspend fun receive(onText: (String) -> Unit, onBinary: (ByteArray) -> Unit, onOther: () -> Unit) {
            for (frame in session.incoming) {
                when (frame) {
                    is Frame.Text -> onText(frame.readText())
                    is Frame.Binary -> onBinary(frame.readBytes())
                    else -> onOther()
                }
            }
        }

        override fun close() {
            scope.launch {
                withContext(NonCancellable) { runCatching { session.close() } }
                session.cancel()
            }
        }
    }

    /** #EXP-1941: the chat's key of a sealed call, from this install's key holder for the caller's account. */
    private fun keys(): CallKeySource? = calling?.accountId?.let(::callKeys)

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
            // #EXP-1950 (install-trust.json `callCases`): the keyring's key, by the rule.
            nodes.get().callTreeKey(accountId, rootId, epoch)
        override fun highestEpoch(rootId: String): Int? = SealedKeys.of(rootId).keys.maxOrNull()
    }

    /** The wave's level at ~30 fps while [audio] is the call's. */
    private suspend fun pumpLevel(audio: VoiceCallAudio) {
        while (scope.isActive && this.audio === audio) {
            _level.value = when (_ui.value.view?.activity) {
                VoiceCall.Activity.Listening -> audio.micLevel
                VoiceCall.Activity.Speaking -> audio.playLevel
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
