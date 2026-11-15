package com.exponential.app.domain

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.intOrNull

/**
 * NEW-69 (voice V5): caller mode on phones. A port of the TS reference
 * `packages/etalk/src/voice-call.ts` (desktop `domain::voice_call`,
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
        const val CALLING = "Calling…"
        const val GETTING_READY = "Getting ready…"
        const val NO_SOUND = "Can't hear your microphone"
        const val LISTENING = "Listening"
        const val THINKING = "Thinking"
        const val SPEAKING = "Speaking"
        const val MUTED = "Microphone muted"
        const val MUTE = "Mute"
        const val UNMUTE = "Unmute"
        const val SPEAKER = "Speaker"
        const val SPEAKER_OFF = "Speaker off"
        const val END_CALL = "End call"
        const val CANCEL = "Cancel"
        /** EXP-1681: the phone surface's minimize button. */
        const val MINIMIZE = "Minimize"
        /** EXP-1857: the second status line while a call falls back ([View.via]). */
        const val VIA = "Voice on {device}"
        const val QUEUED = "{count} queued · {text}"

        /** What the device says once answered, by language (`en` the fallback). */
        val greeting: Map<String, String> = linkedMapOf("en" to "Hello?", "de" to "Hallo?")

        object Reasons {
            /** NEW-99: the member has no voice Assistant yet. */
            const val NO_AGENT = "Set up your Assistant in Settings → Agents"
            const val NO_VOICE_DEVICE = "Run your Assistant on a desktop in Settings → Assistant"
            const val OFFLINE = "{device} is offline"
            const val NOT_HERE = "Calls run on {device}"
            const val PREPARING = "{device} is getting its voice ready"
            /** NEW-99: the voice device's `speech` row reads error. */
            const val VOICE_FAILED = "{device} couldn't load its voice: restart Exponential there"
            const val NO_GPU = "{device} can't speak: no usable GPU"
            const val BUSY = "{device} is in another call"
            /** #NEW-57: a meeting runs on the voice device. */
            const val MEETING = "{device} is transcribing a meeting"
            /** EXP-1857: the member has no desktop at all ([callTarget]). */
            const val NO_DESKTOP = "Open Exponential on one of your desktops to call"
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
            const val UNAVAILABLE = "{device} couldn't get its voice ready"

            val byCode: Map<String, String> = linkedMapOf(
                "no_answer" to NO_ANSWER,
                "device_offline" to DEVICE_OFFLINE,
                "no_microphone" to NO_MICROPHONE,
                "not_granted" to NOT_GRANTED,
                "dropped" to DROPPED,
                "busy" to BUSY,
                "unsupported" to UNSUPPORTED,
                "meeting" to MEETING,
                "unavailable" to UNAVAILABLE,
            )
        }

        /** The telephone's label / the notification title for [agent]. */
        fun call(agent: String): String = CALL.replace("{agent}", agent)
    }

    object Icons {
        /** The telephone's icon concept (`ExpIcons.voiceCall`). */
        const val CALL = "voice-call"
        const val MUTE = "call-mute"
        const val UNMUTE = "call-unmute"
        const val SPEAKER = "call-speaker"
        const val SPEAKER_OFF = "call-speaker-off"
        const val END = "call-end"
        const val MINIMIZE = "ui-chevron-down"
    }

    object TestIds {
        const val CALL = "agent-chat-call"
        const val SURFACE = "voice-call"
        const val MUTE = "voice-call-mute"
        const val END = "voice-call-end"
        const val SPEAKER = "voice-call-speaker"
        const val PANEL = "voice-call-panel"
        const val PILL = "voice-call-pill"
        const val TIMER = "voice-call-timer"
        const val MINIMIZE = "voice-call-minimize"
        /** EXP-1857: the fallback's device line ([View.via]). */
        const val VIA = "voice-call-via"
    }

    object Constants {
        /** EXP-1423: a ring's earliest answer is at least this long ([callRingMs]). */
        const val MIN_RING_MS = 3_000
        /** EXP-1423: up to this much more, drawn once per call by the host. */
        const val RING_JITTER_MS = 6_000
        /** EXP-1423: the ring tone's cadence ([callRingToneOn]): on, then off. */
        const val RING_TONE_MS = 2_000
        const val RING_GAP_MS = 4_000
        /** EXP-1423: the ring tone's two frequencies, sounding together. */
        const val RING_LOW_HZ = 440
        const val RING_HIGH_HZ = 480
        /** A voice still not ready after this fails the call `unavailable`. */
        const val MAX_RING_MS = 300_000
        /** No microphone frame reached this level during the ring = `noSound`. */
        const val NO_SOUND_LEVEL = 0.1
        const val RING_TIMEOUT_MS = 10_000
        const val MIN_SILENCE_MS = 700
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
        /** EXP-1858: a server's speech worker exits after this long without a call. */
        const val SERVER_IDLE_MS = 600_000
    }

    /** The floating call pill (`pill`): the desktop's, EXP-1681 the phones' minimized call. */
    object Pill {
        const val WIDTH = 320
        const val HEIGHT = 56
        const val MARGIN_PX = 24
        /** The phone pill's default bottom edge above the safe area's. */
        const val PHONE_BOTTOM_PX = 96
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
    enum class Speech(val wire: String) { Ok("ok"), Preparing("preparing"), NoGpu("noGpu"), Failed("failed") }

    data class AvailabilityInput(
        /** NEW-99: the member has a voice Assistant (`voice.get` agentId). */
        val agent: Boolean,
        val platform: Platform,
        /** The voice device's name; null = none picked. */
        val device: String?,
        val online: Boolean,
        /** THIS client is the voice device. */
        val here: Boolean,
        /** null = no `speech` row yet. */
        val speech: Speech?,
        /** A call this client does not run already runs on the voice device. */
        val busy: Boolean,
        /** #NEW-57: a meeting runs on the voice device (`meetingStartedAt` set). */
        val meeting: Boolean = false,
    )

    data class Availability(val enabled: Boolean, val reason: String?)

    /** Whether the telephone is enabled, and the reason when it is not. S-3:
     *  a voice that is still downloading rings until it is ready. */
    fun callAvailability(input: AvailabilityInput): Availability {
        fun off(reason: String) = Availability(enabled = false, reason = reason.replace("{device}", input.device ?: ""))
        val r = Copy.Reasons
        return when {
            !input.agent -> off(r.NO_AGENT)
            input.device == null -> off(r.NO_VOICE_DEVICE)
            !input.online -> off(r.OFFLINE)
            input.meeting -> off(r.MEETING)
            input.platform == Platform.Desktop && !input.here -> off(r.NOT_HERE)
            input.speech == null -> off(r.PREPARING)
            input.speech == Speech.Failed -> off(r.VOICE_FAILED)
            input.speech == Speech.NoGpu -> off(r.NO_GPU)
            input.busy -> off(r.BUSY)
            else -> Availability(enabled = true, reason = null)
        }
    }

    /** EXP-1857: one of the member's own desktops as [callTarget] reads it. */
    data class TargetDevice(
        val id: String,
        val name: String,
        val online: Boolean,
        /** Its doctor row `speech` through [callSpeechOf] (null = none yet). */
        val speech: Speech?,
        /** A meeting runs on it. */
        val meeting: Boolean = false,
    )

    data class TargetInput(
        val platform: Platform,
        /** THIS client's own desktop (null on phones and web). */
        val here: String?,
        /** The called agent's Runs on device. */
        val agentDevice: String?,
        /** The member's own desktops, newest `lastSeenAt` first. */
        val desktops: List<TargetDevice>,
        /** A call this client does not run already runs on the target. */
        val busy: Boolean,
        /**
         * EXP-1858: the member's own servers (CLI daemons): a candidate only
         * as the agent's device, never here nor a fallback.
         */
        val servers: List<TargetDevice> = emptyList(),
    )

    data class Target(
        val enabled: Boolean,
        val reason: String?,
        /** The desktop whose speech runs the call (null = disabled). */
        val device: String?,
        val name: String?,
        /** The target is not the agent's own device. */
        val fallback: Boolean,
        /** The target is this client's own desktop: the IDE runs the call. */
        val local: Boolean,
    ) {
        val availability: Availability get() = Availability(enabled, reason)
    }

    /** A desktop that can run a call now (a downloading voice rings until ready). */
    fun speaks(device: TargetDevice): Boolean =
        device.online && !device.meeting && (device.speech == Speech.Ok || device.speech == Speech.Preparing)

    /**
     * EXP-1857 (`targetCases`, TS `callTarget`): where a call's speech runs.
     * The first desktop that speaks of: the agent's device (one of the
     * member's own desktops, EXP-1858: or servers), the desktop you're at, the
     * other desktops newest first. None speaks = disabled with the first
     * candidate's reason.
     */
    fun callTarget(input: TargetInput): Target {
        fun byId(id: String?) = id?.let { want -> input.desktops.firstOrNull { it.id == want } }
        val agentDevice = byId(input.agentDevice)
            ?: input.agentDevice?.let { want -> input.servers.firstOrNull { it.id == want } }
        val candidates = listOfNotNull(agentDevice, byId(input.here)) + input.desktops
        val target = candidates.firstOrNull(::speaks)
        fun off(reason: String?) = Target(enabled = false, reason = reason, device = null, name = null, fallback = false, local = false)
        if (target == null) {
            val first = candidates.firstOrNull() ?: return off(Copy.Reasons.NO_DESKTOP)
            return off(
                callAvailability(
                    AvailabilityInput(
                        agent = true,
                        platform = input.platform,
                        device = first.name,
                        online = first.online,
                        here = true,
                        speech = first.speech,
                        busy = false,
                        meeting = first.meeting,
                    ),
                ).reason,
            )
        }
        if (input.busy) return off(Copy.Reasons.BUSY.replace("{device}", target.name))
        return Target(
            enabled = true,
            reason = null,
            device = target.id,
            name = target.name,
            fallback = target.id != input.agentDevice,
            local = target.id == input.here,
        )
    }

    /**
     * The voice device's `speech` (group `voice`) doctor row: ok → ok,
     * missing (no usable GPU) → noGpu, error (NEW-99: the voice failed to
     * load) → failed, absent → null, anything else → preparing.
     */
    fun callSpeech(doctor: DeviceDoctor?): Speech? =
        callSpeechOf(doctor?.items?.firstOrNull { it.key == "speech" && it.group == "voice" }?.state)

    /** NEW-99 (`speechCases`, TS `callSpeechOf`): a doctor row state → [Speech]; null = no row. */
    fun callSpeechOf(state: String?): Speech? = when (state) {
        null -> null
        "ok" -> Speech.Ok
        "missing" -> Speech.NoGpu
        "error" -> Speech.Failed
        else -> Speech.Preparing
    }

    enum class Phase { Hidden, Ringing, Listening, Thinking, Speaking }

    data class State(
        val phase: Phase,
        val muted: Boolean,
        /** The loudspeaker (false = the earpiece). */
        val speaker: Boolean,
        val device: String,
        /** EXP-1857: the device is not the agent's own ([Copy.VIA] shows it). */
        val fallback: Boolean,
        /** The greeting the device says once answered. */
        val greeting: String,
        /** The ring's earliest answer ([callRingMs]) passed. */
        val rang: Boolean,
        /** The worker confirmed it listens (`CallListening`). */
        val ready: Boolean,
        /** The microphone stayed silent during the ring. */
        val noSound: Boolean,
        /** The last utterance, quoted (what Thinking shows). */
        val said: String,
        val line: String,
        val toast: String?,
    )

    val CALL_IDLE = State(
        phase = Phase.Hidden,
        muted = false,
        speaker = true,
        device = "",
        fallback = false,
        greeting = "",
        rang = false,
        ready = false,
        noSound = false,
        said = "",
        line = "",
        toast = null,
    )

    /** The greeting in [language] (English for an unknown one). */
    fun callGreeting(language: String?): String =
        Copy.greeting[language ?: "en"] ?: Copy.greeting.getValue("en")

    /** One `flowCases` event (`t` on the wire). */
    sealed interface Event {
        data class Start(val device: String, val language: String? = null, val fallback: Boolean = false) : Event
        data object Rang : Event
        data object Ready : Event
        data object NoSound : Event
        data class Partial(val text: String) : Event
        data class Said(val text: String) : Event
        data object SpeechStarted : Event
        data class Queued(val count: Int, val text: String) : Event
        data class ReplyStarted(val text: String) : Event
        data object ReplyFinished : Event
        data object Mute : Event
        data object Unmute : Event
        data class Speaker(val on: Boolean) : Event
        data object End : Event
        data class HungUp(val reason: String?) : Event
        data class Failed(val code: String) : Event

        /**
         * The events a HOST may send in a `call_event` frame; the caller
         * owns start, mute, unmute, speaker, end and hungUp itself. A host
         * that could not get a call ready sends `failed {code}` before its
         * `call_end` (the caller fails with that code's toast).
         */
        val isHostEvent: Boolean
            get() = when (this) {
                Rang, Ready, NoSound, SpeechStarted, ReplyFinished,
                is Partial, is Said, is Queued, is ReplyStarted, is Failed -> true
                else -> false
            }

        companion object {
            /** A wire event (`{t, …}`); null for an unknown `t`. */
            fun of(json: JsonObject): Event? {
                fun str(key: String): String = (json[key] as? JsonPrimitive)?.takeIf { it.isString }?.content ?: ""
                return when (str("t")) {
                    "start" -> Start(
                        str("device"),
                        str("language").ifEmpty { null },
                        (json["fallback"] as? JsonPrimitive)?.booleanOrNull ?: false,
                    )
                    "rang" -> Rang
                    "ready" -> Ready
                    "noSound" -> NoSound
                    "partial" -> Partial(str("text"))
                    "said" -> Said(str("text"))
                    "speechStarted" -> SpeechStarted
                    "queued" -> Queued((json["count"] as? JsonPrimitive)?.intOrNull ?: 0, str("text"))
                    "replyStarted" -> ReplyStarted(str("text"))
                    "replyFinished" -> ReplyFinished
                    "mute" -> Mute
                    "unmute" -> Unmute
                    "speaker" -> Speaker((json["on"] as? JsonPrimitive)?.booleanOrNull ?: true)
                    "end" -> End
                    "hungUp" -> HungUp(str("reason").ifEmpty { null })
                    "failed" -> Failed(str("code"))
                    else -> null
                }
            }
        }
    }

    private fun quoted(text: String): String = "“$text”"

    /** Rang and ready: the device speaks its greeting (not an agent turn). */
    private fun answered(state: State): State =
        state.copy(phase = Phase.Speaking, line = state.greeting, toast = null)

    /** One step of the call (the same machine on the voice device and a caller). */
    fun callStep(state: State, event: Event): State {
        fun at(phase: Phase, line: String, said: String = state.said, noSound: Boolean = state.noSound) =
            state.copy(phase = phase, line = line, toast = null, said = said, noSound = noSound)
        if (event is Event.Start) {
            return CALL_IDLE.copy(
                phase = Phase.Ringing,
                device = event.device,
                fallback = event.fallback,
                greeting = callGreeting(event.language),
            )
        }
        if (state.phase == Phase.Hidden) return state
        val ringing = state.phase == Phase.Ringing
        return when (event) {
            is Event.Start -> state
            Event.Rang -> when {
                !ringing -> state
                state.ready -> answered(state.copy(rang = true))
                else -> state.copy(rang = true)
            }
            Event.Ready -> when {
                !ringing -> state
                state.rang -> answered(state.copy(ready = true))
                else -> state.copy(ready = true)
            }
            Event.NoSound -> state.copy(noSound = true)
            is Event.Partial -> {
                val text = event.text.trim()
                if (state.phase == Phase.Listening && text.isNotEmpty()) at(Phase.Listening, quoted(text), noSound = false) else state
            }
            is Event.Said -> {
                val text = event.text.trim()
                if (ringing || text.isEmpty()) state else at(Phase.Thinking, quoted(text), said = quoted(text), noSound = false)
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
            is Event.ReplyStarted -> if (ringing) state else at(Phase.Speaking, event.text.trim())
            Event.ReplyFinished -> if (state.phase == Phase.Speaking) at(Phase.Listening, "") else state
            Event.Mute -> state.copy(muted = true)
            Event.Unmute -> state.copy(muted = false)
            is Event.Speaker -> state.copy(speaker = event.on)
            Event.End -> CALL_IDLE
            is Event.HungUp -> if (event.reason.isNullOrEmpty()) CALL_IDLE else callStep(state, Event.Failed(HangUpRules.FAILED_CODE))
            is Event.Failed -> {
                val toast = (Copy.Failed.byCode[event.code] ?: Copy.Failed.DROPPED).replace("{device}", state.device)
                CALL_IDLE.copy(toast = toast)
            }
        }
    }

    /** What the avatar's ring shows (`ringing` a halo, `speaking` the VoiceWave). */
    enum class Activity(val wire: String) {
        Ringing("ringing"),
        Listening("listening"),
        Thinking("thinking"),
        Speaking("speaking"),
        Muted("muted"),
        ;

        /** The state word, the avatar's accessibility value. */
        val label: String get() = when (this) {
            Ringing -> Copy.CALLING
            Listening -> Copy.LISTENING
            Thinking -> Copy.THINKING
            Speaking -> Copy.SPEAKING
            Muted -> Copy.MUTED
        }
    }

    /** One round call button (`call-controls`). */
    enum class Button(val wire: String) { Mute("mute"), Speaker("speaker"), End("end") }

    data class View(
        val activity: Activity,
        /** The line under the name; null = the call's timer ([callTimer]). */
        val status: String?,
        /** EXP-1857: the device that speaks while the call falls back (a
         *  second status line, [Copy.VIA]); null otherwise. */
        val via: String?,
        val caption: String,
        val muted: Boolean,
        val speaker: Boolean,
        /** The round buttons, in order. */
        val buttons: List<Button>,
        /** The end button's label (shown under it while ringing). */
        val end: String,
    )

    /** What the call surface shows; null = no surface. */
    fun callView(state: State, platform: Platform = Platform.Phone): View? {
        if (state.phase == Phase.Hidden) return null
        val via = if (state.fallback) Copy.VIA.replace("{device}", state.device) else null
        if (state.phase == Phase.Ringing) {
            return View(
                activity = Activity.Ringing,
                status = if (state.rang) Copy.GETTING_READY else Copy.CALLING,
                via = via,
                caption = "",
                muted = state.muted,
                speaker = state.speaker,
                buttons = listOf(Button.End),
                end = Copy.CANCEL,
            )
        }
        val buttons = if (platform == Platform.Phone) listOf(Button.Mute, Button.Speaker, Button.End) else listOf(Button.Mute, Button.End)
        fun inCall(activity: Activity, caption: String) = View(
            activity = activity,
            status = null,
            via = via,
            caption = caption,
            muted = state.muted,
            speaker = state.speaker,
            buttons = buttons,
            end = Copy.END_CALL,
        )
        return when (state.phase) {
            Phase.Listening -> when {
                state.muted -> inCall(Activity.Muted, Copy.MUTED)
                else -> inCall(Activity.Listening, state.line.ifEmpty { if (state.noSound) Copy.NO_SOUND else "" })
            }
            Phase.Thinking -> inCall(Activity.Thinking, state.line)
            Phase.Speaking -> inCall(Activity.Speaking, state.line)
            Phase.Hidden, Phase.Ringing -> null
        }
    }

    /**
     * EXP-1423: the earliest answer of a ring, ms after it started:
     * `minRingMs` + floor(clamp(random, 0, 1) × `ringJitterMs`). The HOST
     * draws [random] once per call; a caller never draws.
     */
    fun callRingMs(random: Double): Int {
        val unit = if (random.isNaN()) 0.0 else random.coerceIn(0.0, 1.0)
        return Constants.MIN_RING_MS + kotlin.math.floor(unit * Constants.RING_JITTER_MS).toInt()
    }

    /**
     * EXP-1423: when the ring ends (answered), ms after it started: the later
     * of the earliest answer and the warm-up's `ready`; null = never answered
     * (no ready, or one at or past `maxRingMs`: failed `unavailable`).
     */
    fun callAnswerMs(random: Double, readyMs: Int?): Int? {
        if (readyMs == null || readyMs >= Constants.MAX_RING_MS) return null
        return maxOf(callRingMs(random), readyMs)
    }

    /** EXP-1423: whether the ring tone sounds [ms] after the ring started (`ringToneMs` on, `ringGapMs` off, looping). */
    fun callRingToneOn(ms: Long): Boolean =
        ms >= 0 && ms % (Constants.RING_TONE_MS + Constants.RING_GAP_MS) < Constants.RING_TONE_MS

    /** The call's timer since it was answered: mm:ss, h:mm:ss from an hour. */
    fun callTimer(ms: Long): String {
        val total = maxOf(0L, ms / 1000)
        val hours = total / 3600
        val minutes = (total % 3600) / 60
        val seconds = total % 60
        fun pad(n: Long) = n.toString().padStart(2, '0')
        return if (hours > 0) "$hours:${pad(minutes)}:${pad(seconds)}" else "${pad(minutes)}:${pad(seconds)}"
    }

    /** One styleguide row of the surface (`surfaceSpecimens`). */
    data class SurfaceSpecimen(
        val name: String,
        val agent: String,
        /** The ticking timer's text; null = [View.status] shows. */
        val timer: String?,
        val level: Double,
        val bands: List<Double>,
        val view: View,
    )

    private val PHONE_BUTTONS = listOf(Button.Mute, Button.Speaker, Button.End)

    val surfaceSpecimens: List<SurfaceSpecimen> = listOf(
        SurfaceSpecimen(
            "ringing", "Assistant", null, 0.0, emptyList(),
            View(Activity.Ringing, "Calling…", null, "", muted = false, speaker = true, buttons = listOf(Button.End), end = "Cancel"),
        ),
        SurfaceSpecimen(
            "getting ready", "Assistant", null, 0.0, emptyList(),
            View(Activity.Ringing, "Getting ready…", null, "", muted = false, speaker = true, buttons = listOf(Button.End), end = "Cancel"),
        ),
        SurfaceSpecimen(
            "in call, speaking", "Assistant", "04:12", 0.5, listOf(0.7, 1.0, 0.5, 0.8),
            View(
                Activity.Speaking, null, null, "Batch 4 is rebasing; the other two are merged. Want me to merge the last one when it's green?",
                muted = false, speaker = true, buttons = PHONE_BUTTONS, end = "End call",
            ),
        ),
        SurfaceSpecimen(
            "in call, listening", "Assistant", "04:20", 0.7, listOf(0.9, 0.6, 1.0, 0.5),
            View(Activity.Listening, null, null, "“merge it when it's green”", muted = false, speaker = true, buttons = PHONE_BUTTONS, end = "End call"),
        ),
        SurfaceSpecimen(
            "muted", "Assistant", "04:31", 0.0, emptyList(),
            View(Activity.Muted, null, null, "Microphone muted", muted = true, speaker = true, buttons = PHONE_BUTTONS, end = "End call"),
        ),
        SurfaceSpecimen(
            "in call, falling back", "Assistant", "04:20", 0.7, listOf(0.9, 0.6, 1.0, 0.5),
            View(
                Activity.Listening, null, "Voice on mint", "“merge it when it's green”",
                muted = false, speaker = true, buttons = PHONE_BUTTONS, end = "End call",
            ),
        ),
    )
}
