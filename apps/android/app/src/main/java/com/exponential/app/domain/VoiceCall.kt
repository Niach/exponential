package com.exponential.app.domain

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.intOrNull

/**
 * NEW-69 (voice V5): caller mode on phones. A port of the TS reference
 * `packages/domain-contract/src/voice-call.ts` (desktop `domain::voice_call`,
 * iOS `VoiceCall.swift`), byte-locked by `fixtures/voice-call.json` (`copy`,
 * `testIds`, `icons`, `constants`, `wire`, `surfaceSpecimens` and every
 * `availabilityCases` / `flowCases` row, replayed by `VoiceCallTest`). The
 * fixture's `$comment` holds the rules: the telephone in the agent chat's
 * header, the pure availability, the call machine the phone drives from the
 * host's `call_event` frames, the relay transport and the audio wire.
 */
object VoiceCall {

    object Copy {
        const val CALL = "Call {agent}"
        const val CONNECTING = "Connecting"
        const val LISTENING = "Listening"
        const val THINKING = "Thinking"
        const val SPEAKING = "Speaking"
        const val MUTED = "Microphone muted"
        const val MUTED_LINE = "Unmute to talk"
        const val MUTE = "Mute"
        const val UNMUTE = "Unmute"
        const val END_CALL = "End call"
        const val QUEUED = "{count} queued · {text}"

        object Reasons {
            const val NO_VOICE_DEVICE = "Pick a voice device in Settings → Voice"
            const val SPEAK_OFF = "Turn on Speak replies in Settings → Voice"
            const val OFFLINE = "{device} is offline"
            const val NOT_HERE = "Calls run on {device}"
            const val PREPARING = "{device} is getting its voice ready"
            const val NO_GPU = "{device} can't speak: no usable GPU"
            const val BUSY = "{device} is in another call"
            /** #NEW-57: a meeting runs on the voice device. */
            const val MEETING = "{device} is transcribing a meeting"
        }

        /** A failure's toast by its code; an unknown code reads [Failed.DROPPED]. */
        object Failed {
            const val NO_ANSWER = "{device} didn't answer"
            const val DEVICE_OFFLINE = "{device} is offline"
            const val NO_MICROPHONE = "No microphone"
            const val NOT_GRANTED = "Microphone not allowed"
            const val DROPPED = "The call dropped"
            const val BUSY = "{device} is in another call"
            const val UNSUPPORTED = "This browser can't make calls"
            const val MEETING = "{device} is transcribing a meeting"

            val byCode: Map<String, String> = linkedMapOf(
                "no_answer" to NO_ANSWER,
                "device_offline" to DEVICE_OFFLINE,
                "no_microphone" to NO_MICROPHONE,
                "not_granted" to NOT_GRANTED,
                "dropped" to DROPPED,
                "busy" to BUSY,
                "unsupported" to UNSUPPORTED,
            "meeting" to MEETING,
            )
        }

        /** The telephone's label / the notification title for [agent]. */
        fun call(agent: String): String = CALL.replace("{agent}", agent)
    }

    object Icons {
        /** The telephone's icon concept (`ExpIcons.voiceCall`). */
        const val CALL = "voice-call"
    }

    object TestIds {
        const val CALL = "agent-chat-call"
        const val SURFACE = "voice-call"
        const val MUTE = "voice-call-mute"
        const val END = "voice-call-end"
    }

    object Constants {
        const val RING_TIMEOUT_MS = 10_000
        const val MIN_SILENCE_MS = 250
        const val BARGE_IN_MS = 250
        const val FRAME_MS = 20
        const val UP_RATE = 16_000
        const val DOWN_RATE = 24_000
        const val UP_KBPS = 24
        const val DOWN_KBPS = 32
        const val JITTER_MS = 40
        const val MAX_FRAMES_PER_SECOND = 60
        const val MAX_CALL_MS = 3_600_000
        const val IDLE_TIMEOUT_MS = 30_000
    }

    /**
     * `hangUpRules`: a `call_end` steps `HungUp(reason)`; ANY reason (`idle`,
     * `max_length`, `dropped`, or one this build doesn't know) fails the call
     * as [FAILED_CODE], only a reason-less one ends silently.
     */
    object HangUpRules {
        const val FAILED_CODE = "dropped"
    }

    /**
     * The uplink while muted sends SILENCE, never nothing: the relay ends a
     * call after [Constants.IDLE_TIMEOUT_MS] without a frame and the host
     * only talks back while speaking, so a muted caller keeps the call alive
     * with one encoded silent frame per [Constants.FRAME_MS].
     */
    object Uplink {
        const val FRAME_SAMPLES = Constants.UP_RATE * Constants.FRAME_MS / 1000

        /** What the encoder gets for one captured frame of [count] samples: silence while muted. */
        fun samples(frame: ShortArray, count: Int, muted: Boolean): ShortArray =
            if (muted) ShortArray(count) else frame
    }

    object Wire {
        /** Byte 0 of a binary frame: caller → host audio. */
        const val KIND_UP = 1
        /** Byte 0 of a binary frame: host → caller audio. */
        const val KIND_DOWN = 2
        const val MAX_FRAME_BYTES = 1500
        /** The JSON text frames (`wire.control`'s keys). */
        val control: List<String> = listOf("call_ring", "call_ready", "call_event", "call_mute", "call_end")

        /** One uplink/downlink frame: kind, the u16 big-endian [seq], the packet. */
        fun frame(kind: Int, seq: Int, packet: ByteArray): ByteArray {
            val out = ByteArray(3 + packet.size)
            out[0] = kind.toByte()
            out[1] = ((seq shr 8) and 0xff).toByte()
            out[2] = (seq and 0xff).toByte()
            packet.copyInto(out, destinationOffset = 3)
            return out
        }
    }

    enum class Platform(val wire: String) { Desktop("desktop"), Phone("phone"), Web("web") }

    /** The voice device's doctor row `speech` as a caller reads it. */
    enum class Speech(val wire: String) { Ok("ok"), Preparing("preparing"), NoGpu("noGpu") }

    data class AvailabilityInput(
        val platform: Platform,
        /** The voice device's name; null = none picked. */
        val device: String?,
        val speakReplies: Boolean,
        val online: Boolean,
        /** THIS client is the voice device. */
        val here: Boolean,
        /** null = no `speech` row yet. */
        val speech: Speech?,
        /** A call already runs on the voice device. */
        val busy: Boolean,
        /** #NEW-57: a meeting runs on the voice device (`meetingStartedAt` set). */
        val meeting: Boolean = false,
    )

    data class Availability(val enabled: Boolean, val reason: String?)

    /** Whether the telephone is enabled, and the reason when it is not. */
    fun callAvailability(input: AvailabilityInput): Availability {
        fun off(reason: String) = Availability(enabled = false, reason = reason.replace("{device}", input.device ?: ""))
        val r = Copy.Reasons
        return when {
            input.device == null -> off(r.NO_VOICE_DEVICE)
            !input.speakReplies -> off(r.SPEAK_OFF)
            !input.online -> off(r.OFFLINE)
            input.meeting -> off(r.MEETING)
            input.platform == Platform.Desktop && !input.here -> off(r.NOT_HERE)
            input.speech == null || input.speech == Speech.Preparing -> off(r.PREPARING)
            input.speech == Speech.NoGpu -> off(r.NO_GPU)
            input.busy -> off(r.BUSY)
            else -> Availability(enabled = true, reason = null)
        }
    }

    /**
     * The voice device's `speech` (group `voice`) doctor row: ok → ok,
     * missing (no usable GPU) → noGpu, absent → null, anything else → preparing
     * (web `callSpeechOf`).
     */
    fun callSpeech(doctor: DeviceDoctor?): Speech? {
        val row = doctor?.items?.firstOrNull { it.key == "speech" && it.group == "voice" } ?: return null
        return when (row.state) {
            "ok" -> Speech.Ok
            "missing" -> Speech.NoGpu
            else -> Speech.Preparing
        }
    }

    enum class Phase { Hidden, Connecting, Listening, Thinking, Speaking }

    data class State(
        val phase: Phase,
        val muted: Boolean,
        val device: String,
        /** The last utterance, quoted (what Thinking shows). */
        val said: String,
        val line: String,
        val toast: String?,
    )

    val CALL_IDLE = State(phase = Phase.Hidden, muted = false, device = "", said = "", line = "", toast = null)

    /** One `flowCases` event (`t` on the wire). */
    sealed interface Event {
        data class Start(val device: String) : Event
        data object Connected : Event
        data class Partial(val text: String) : Event
        data class Said(val text: String) : Event
        data object SpeechStarted : Event
        data class Queued(val count: Int, val text: String) : Event
        data class ReplyStarted(val text: String) : Event
        data object ReplyFinished : Event
        data object Mute : Event
        data object Unmute : Event
        data object End : Event
        data class HungUp(val reason: String?) : Event
        data class Failed(val code: String) : Event

        companion object {
            /** A wire event (`{t, …}`); null for an unknown `t`. */
            fun of(json: JsonObject): Event? {
                fun str(key: String): String = (json[key] as? JsonPrimitive)?.takeIf { it.isString }?.content ?: ""
                return when (str("t")) {
                    "start" -> Start(str("device"))
                    "connected" -> Connected
                    "partial" -> Partial(str("text"))
                    "said" -> Said(str("text"))
                    "speechStarted" -> SpeechStarted
                    "queued" -> Queued((json["count"] as? JsonPrimitive)?.intOrNull ?: 0, str("text"))
                    "replyStarted" -> ReplyStarted(str("text"))
                    "replyFinished" -> ReplyFinished
                    "mute" -> Mute
                    "unmute" -> Unmute
                    "end" -> End
                    "hungUp" -> HungUp(str("reason").ifEmpty { null })
                    "failed" -> Failed(str("code"))
                    else -> null
                }
            }
        }
    }

    private fun quoted(text: String): String = "“$text”"

    /** One step of the call (the same machine on the voice device and a caller). */
    fun callStep(state: State, event: Event): State {
        fun at(phase: Phase, line: String, said: String = state.said) =
            state.copy(phase = phase, line = line, toast = null, said = said)
        if (event is Event.Start) return CALL_IDLE.copy(phase = Phase.Connecting, device = event.device)
        if (state.phase == Phase.Hidden) return state
        return when (event) {
            is Event.Start -> state
            Event.Connected -> if (state.phase == Phase.Connecting) at(Phase.Listening, "") else state
            is Event.Partial -> {
                val text = event.text.trim()
                if (state.phase == Phase.Listening && text.isNotEmpty()) at(Phase.Listening, quoted(text)) else state
            }
            is Event.Said -> {
                val text = event.text.trim()
                if (text.isEmpty()) state else at(Phase.Thinking, quoted(text), said = quoted(text))
            }
            Event.SpeechStarted -> if (state.phase == Phase.Speaking) at(Phase.Listening, "") else state
            is Event.Queued -> when {
                state.phase != Phase.Thinking && state.phase != Phase.Speaking -> state
                event.count <= 0 -> at(state.phase, if (state.phase == Phase.Thinking) state.said else state.line)
                else -> at(
                    state.phase,
                    Copy.QUEUED.replace("{count}", event.count.toString()).replace("{text}", quoted(event.text.trim())),
                )
            }
            is Event.ReplyStarted -> at(Phase.Speaking, event.text.trim())
            Event.ReplyFinished -> if (state.phase == Phase.Speaking) at(Phase.Listening, "") else state
            Event.Mute -> state.copy(muted = true)
            Event.Unmute -> state.copy(muted = false)
            Event.End -> CALL_IDLE
            is Event.HungUp -> if (event.reason.isNullOrEmpty()) CALL_IDLE else callStep(state, Event.Failed(HangUpRules.FAILED_CODE))
            is Event.Failed -> {
                val toast = (Copy.Failed.byCode[event.code] ?: Copy.Failed.DROPPED).replace("{device}", state.device)
                CALL_IDLE.copy(toast = toast)
            }
        }
    }

    /** The VoiceWave activity the surface draws (`VoiceWaveActivity`). */
    enum class Activity(val wire: String) { Listening("listening"), Thinking("thinking"), Speaking("speaking"), Muted("muted") }

    data class View(
        val activity: Activity,
        val state: String,
        val line: String,
        /** The mute pill's label (Mute / Unmute); End call is always beside it. */
        val mute: String,
    )

    /** What the call surface shows; null = no surface. */
    fun callView(state: State): View? {
        val mute = if (state.muted) Copy.UNMUTE else Copy.MUTE
        return when (state.phase) {
            Phase.Hidden -> null
            Phase.Connecting -> View(Activity.Thinking, Copy.CONNECTING, state.line, mute)
            Phase.Listening -> if (state.muted) {
                View(Activity.Muted, Copy.MUTED, Copy.MUTED_LINE, mute)
            } else {
                View(Activity.Listening, Copy.LISTENING, state.line, mute)
            }
            Phase.Thinking -> View(Activity.Thinking, Copy.THINKING, state.line, mute)
            Phase.Speaking -> View(Activity.Speaking, Copy.SPEAKING, state.line, mute)
        }
    }

    /** One styleguide row of the surface (`surfaceSpecimens`). */
    data class SurfaceSpecimen(
        val activity: Activity,
        val level: Double,
        val bands: List<Double>,
        val state: String,
        val line: String,
        val mute: String,
    ) {
        val view: View get() = View(activity, state, line, mute)
    }

    val surfaceSpecimens: List<SurfaceSpecimen> = listOf(
        SurfaceSpecimen(Activity.Listening, 0.7, listOf(0.9, 0.6, 1.0, 0.5), "Listening", "“what is still open on the voice board”", "Mute"),
        SurfaceSpecimen(Activity.Thinking, 0.0, emptyList(), "Thinking", "1 queued · “and assign them to me”", "Mute"),
        SurfaceSpecimen(Activity.Speaking, 0.5, listOf(0.7, 1.0, 0.5, 0.8), "Speaking", "Three issues are open: NEW-55, NEW-56 and NEW-79.", "Mute"),
        SurfaceSpecimen(Activity.Muted, 0.0, emptyList(), "Microphone muted", "Unmute to talk", "Unmute"),
    )
}
