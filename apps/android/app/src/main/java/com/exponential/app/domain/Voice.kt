package com.exponential.app.domain

import java.text.Collator
import java.util.Locale
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject

/**
 * NEW-67 (voice plan v3, V1): THE Settings → Voice section. A port of the TS
 * reference `packages/domain-contract/src/voice.ts` (desktop `domain::voice`,
 * iOS `Voice.swift`), byte-locked by `fixtures/voice.json` (`copy`,
 * `testIds`, `shortcutGrammar`, `languages`, `limits`, `defaults`,
 * `languageNote`, the option builders and S-3's `shortcutCanonical` /
 * `shortcutLabel` / `capture*` / `wakeDetail`, replayed by `VoiceTest`). The phones only show and edit the member's setting: speaking
 * is desktop-only, so `shouldSpeak` / `spokenText` are not mirrored here. The
 * fixture's `$comment` holds the rules.
 */
object Voice {

    object Copy {
        const val GROUP = "Voice"
        const val VOICE_DEVICE = "Voice device"
        const val VOICE_DEVICE_NONE = "None"
        const val AGENT = "Agent"
        const val AGENT_NONE = "None"
        const val LANGUAGE = "Language"
        const val LANGUAGE_NOTE = "Needs a GPU on {device}"
        const val VOICE_FILE = "Voice file"
        const val VOICE_FILE_PLACEHOLDER = "Built-in"
        const val VOICE_TRANSCRIPT = "Transcript"
        const val VOICE_TRANSCRIPT_PLACEHOLDER = "What the clip says"
        const val TRAY_HEADER = "On {device} (tray)"
        const val PUSH_TO_TALK = "Push to talk"
        const val WAKE_WORD = "Wake word"
        const val WAKE_PHRASE = "Wake phrase"
        const val WAKE_PHRASE_PLACEHOLDER = "Hey {agent}"
        const val WAKE_PHRASE_TOO_SHORT = "The wake phrase needs at least 6 letters"
        const val TRAY_OPEN = "Open Exponential"
        const val TRAY_CALL = "Call {agent}"
        const val TRAY_MUTE = "Mute microphone"
        const val TRAY_SETTINGS = "Voice settings…"
        const val TRAY_QUIT = "Quit"
        const val DOWNLOADING = "Downloading voice…"
        const val DOWNLOADING_PROGRESS = "Downloading voice… {received} of {total}"
        const val DOWNLOAD_FAILED = "Couldn't download the voice"
        const val UNAVAILABLE = "The voice stopped working"
        const val SPEAKING = "Speaking"
        const val STOP = "Stop"
        const val LINK = "link"
        const val ELLIPSIS = "…"
        const val SHORTCUT_NONE = "None"
        const val SHORTCUT_RECORD = "Press any key or combination…"
        const val SHORTCUT_CLEAR = "Clear"
        const val TRAY_END_CALL = "End call"
    }

    /** The `pttCopy` lines the phones show (S-3). */
    object PttCopy {
        const val SHORTCUT_NEEDS_MODIFIER = "Hold Ctrl, Alt, Shift or Super with that key"

        /** The Wake word row's trailing detail by [WakeState] (`pttCopy.wakeDetail`). */
        object WakeDetail {
            const val LISTENING = "Listening"
            const val MUTED = "Microphone muted"
            const val NO_AGENT = "Pick an agent"
            const val DOWNLOADING = "Downloading speech recognition…"
            const val MIC_BUSY = "A call is using the microphone"
            const val MEETING = "A meeting is using the microphone"
            const val NO_MICROPHONE = "No microphone"
            const val NOT_GRANTED = "Microphone not allowed"
            const val UNAVAILABLE = "Speech recognition unavailable"
            const val OFFLINE = "{device} is offline"
            const val STARTING = "Starting…"
        }
    }

    object TestIds {
        const val PAGE = "settings-voice"
        const val VOICE_DEVICE = "settings-voice-device"
        const val AGENT = "settings-voice-agent"
        const val LANGUAGE = "settings-voice-language"
        const val VOICE_FILE = "settings-voice-file"
        const val VOICE_TRANSCRIPT = "settings-voice-transcript"
        const val SHORTCUT = "settings-voice-shortcut"
        const val WAKE_WORD = "settings-voice-wake-word"
        const val WAKE_PHRASE = "settings-voice-wake-phrase"
        const val SHORTCUT_CLEAR = "settings-voice-shortcut-clear"
    }

    const val MAX_CHARS = 600

    /**
     * S-3: a push-to-talk binding is any key or mouse combination
     * (`shortcutGrammar`): zero or more [MODS] in this order joined by `+`,
     * then exactly one key. Empty = no key bound = push to talk off.
     */
    object ShortcutGrammar {
        val MODS: List<String> = listOf("ctrl", "alt", "shift", "super")
        val NAMED: List<String> = listOf(
            "space", "enter", "tab", "backspace", "delete", "insert", "home", "end", "pageup", "pagedown",
            "up", "down", "left", "right", "minus", "equal", "bracketleft", "bracketright", "backslash",
            "semicolon", "quote", "comma", "period", "slash", "backquote", "pause", "scrolllock", "printscreen",
        )
        val BARE_KEYS: List<String> = listOf("f1-f24", "insert", "pause", "scrolllock", "printscreen", "mouse3-mouse9")
        const val MOUSE_MIN = 3
        const val MOUSE_MAX = 9
        const val FUNCTION_MIN = 1
        const val FUNCTION_MAX = 24
        const val MAX_LENGTH = 32
        const val SEPARATOR = " + "
        val LABELS: Map<String, String> = linkedMapOf(
            "ctrl" to "Ctrl", "alt" to "Alt", "shift" to "Shift", "super" to "Super",
            "space" to "Space", "enter" to "Enter", "tab" to "Tab", "backspace" to "Backspace",
            "delete" to "Delete", "insert" to "Insert", "home" to "Home", "end" to "End",
            "pageup" to "Page Up", "pagedown" to "Page Down", "up" to "Up", "down" to "Down",
            "left" to "Left", "right" to "Right", "minus" to "-", "equal" to "=",
            "bracketleft" to "[", "bracketright" to "]", "backslash" to "\\", "semicolon" to ";",
            "quote" to "'", "comma" to ",", "period" to ".", "slash" to "/", "backquote" to "`",
            "pause" to "Pause", "scrolllock" to "Scroll Lock", "printscreen" to "Print Screen",
            "mouse" to "Mouse button {n}",
        )
    }

    private val LETTER_OR_DIGIT = Regex("^[a-z0-9]$")
    private val FUNCTION_KEY = Regex("^f([1-9][0-9]?)$")
    private val MOUSE_KEY = Regex("^mouse([0-9])$")

    /** The grammar's key (not a modifier): a letter, digit, f1-f24, a named key or mouse3-mouse9. */
    private fun isShortcutKey(part: String): Boolean {
        val g = ShortcutGrammar
        if (LETTER_OR_DIGIT.matches(part)) return true
        FUNCTION_KEY.matchEntire(part)?.let { val n = it.groupValues[1].toInt(); return n in g.FUNCTION_MIN..g.FUNCTION_MAX }
        MOUSE_KEY.matchEntire(part)?.let { val n = it.groupValues[1].toInt(); return n in g.MOUSE_MIN..g.MOUSE_MAX }
        return part in g.NAMED
    }

    /** A key that may be bound without a modifier (`bareKeys`). */
    private fun isBareKey(part: String): Boolean =
        Regex("^f[0-9]+$").matches(part) || MOUSE_KEY.matches(part) || part in listOf("insert", "pause", "scrolllock", "printscreen")

    /** S-3: the canonical text of a binding, or null when the grammar refuses it; empty stays empty (no key = off). */
    fun shortcutCanonical(text: String): String? {
        if (text.isEmpty()) return ""
        val parts = text.split("+").toMutableList()
        val key = parts.removeAt(parts.size - 1)
        if (!isShortcutKey(key)) return null
        if (parts.any { it !in ShortcutGrammar.MODS } || parts.toSet().size != parts.size) return null
        if (parts.isEmpty() && !isBareKey(key)) return null
        val value = (ShortcutGrammar.MODS.filter { it in parts } + key).joinToString("+")
        return if (value.length > ShortcutGrammar.MAX_LENGTH) null else value
    }

    private fun partLabel(part: String): String {
        MOUSE_KEY.matchEntire(part)?.let { return ShortcutGrammar.LABELS.getValue("mouse").replace("{n}", it.groupValues[1]) }
        return ShortcutGrammar.LABELS[part] ?: part.uppercase(Locale.ROOT)
    }

    /** S-3: a binding's label (`Ctrl + Shift + K`); empty reads None. */
    fun shortcutLabel(value: String): String {
        if (value.isEmpty()) return Copy.SHORTCUT_NONE
        return value.split("+").joinToString(ShortcutGrammar.SEPARATOR) { partLabel(it) }
    }

    /** S-3: the push-to-talk recorder's state (`captureCases`; the phones never record, ported for parity). */
    data class CaptureState(
        val status: CaptureStatus,
        /** The binding (the one before recording until a key records). */
        val value: String,
        /** The modifiers held now, in canonical order. */
        val held: List<String>,
        val error: String?,
    )

    enum class CaptureStatus(val wire: String) { Recording("recording"), Done("done"), Cancelled("cancelled") }

    sealed interface CaptureEvent {
        data class Down(val key: String) : CaptureEvent
        data class Up(val key: String) : CaptureEvent
        data class Mouse(val button: Int) : CaptureEvent
    }

    fun captureStart(value: String): CaptureState = CaptureState(CaptureStatus.Recording, value, emptyList(), null)

    /** S-3: one step of the recorder (`captureCases`). */
    fun captureStep(state: CaptureState, event: CaptureEvent): CaptureState {
        if (state.status != CaptureStatus.Recording) return state
        val mods = ShortcutGrammar.MODS
        fun record(key: String): CaptureState {
            val value = shortcutCanonical((state.held + key).joinToString("+"))
                ?: return state.copy(error = PttCopy.SHORTCUT_NEEDS_MODIFIER)
            return CaptureState(CaptureStatus.Done, value, emptyList(), null)
        }
        val key = when (event) {
            is CaptureEvent.Mouse -> return if (event.button < ShortcutGrammar.MOUSE_MIN) state else record("mouse${event.button}")
            is CaptureEvent.Down -> event.key
            is CaptureEvent.Up -> event.key
        }
        if (key in mods) {
            val held = if (event is CaptureEvent.Down) state.held + key else state.held.filter { it != key }
            return state.copy(held = mods.filter { it in held })
        }
        if (event is CaptureEvent.Up) return state
        if (state.held.isEmpty() && key == "escape") return state.copy(status = CaptureStatus.Cancelled, held = emptyList())
        if (state.held.isEmpty() && (key == "backspace" || key == "delete")) {
            return CaptureState(CaptureStatus.Done, "", emptyList(), null)
        }
        if (key == "escape") return state.copy(error = PttCopy.SHORTCUT_NEEDS_MODIFIER)
        if (!isShortcutKey(key)) return state
        return record(key)
    }

    /** S-3: the held modifiers while recording (`Ctrl + …`; empty = none). */
    fun capturePending(state: CaptureState): String {
        if (state.status != CaptureStatus.Recording || state.held.isEmpty()) return ""
        return state.held.joinToString(ShortcutGrammar.SEPARATOR) { partLabel(it) } + ShortcutGrammar.SEPARATOR + "…"
    }

    /** S-3: why the wake word is or is not listening (the voice device's worker / doctor row `wake`). */
    enum class WakeState(val wire: String, val detail: String) {
        Listening("listening", PttCopy.WakeDetail.LISTENING),
        Muted("muted", PttCopy.WakeDetail.MUTED),
        NoAgent("noAgent", PttCopy.WakeDetail.NO_AGENT),
        Downloading("downloading", PttCopy.WakeDetail.DOWNLOADING),
        MicBusy("micBusy", PttCopy.WakeDetail.MIC_BUSY),
        Meeting("meeting", PttCopy.WakeDetail.MEETING),
        NoMicrophone("noMicrophone", PttCopy.WakeDetail.NO_MICROPHONE),
        NotGranted("notGranted", PttCopy.WakeDetail.NOT_GRANTED),
        Unavailable("unavailable", PttCopy.WakeDetail.UNAVAILABLE),
        Offline("offline", PttCopy.WakeDetail.OFFLINE),
        Starting("starting", PttCopy.WakeDetail.STARTING),
    }

    data class WakeDetailContext(
        val wakeWord: Boolean,
        /** An agent is picked. */
        val agent: Boolean,
        /** The voice device is online. */
        val online: Boolean,
        /** The voice device's name. */
        val device: String,
        /** What the worker reported; null = nothing yet. */
        val state: WakeState?,
    )

    /** S-3: the Wake word row's trailing detail ×4 (null = none). */
    fun wakeDetail(context: WakeDetailContext): String? {
        val words = PttCopy.WakeDetail
        if (!context.wakeWord) return null
        if (!context.agent) return words.NO_AGENT
        if (!context.online) return words.OFFLINE.replace("{device}", context.device)
        return (context.state?.detail ?: words.STARTING).replace("{device}", context.device)
    }

    /**
     * S-3: the Wake word row's detail on a phone: the voice device's doctor
     * row `wake` (group voice) verbatim when it reports one, else
     * [wakeDetail] without a worker state (no agent / offline / starting).
     */
    fun wakeRowDetail(doctor: DeviceDoctor?, context: WakeDetailContext): String? {
        if (!context.wakeWord) return null
        if (context.agent && context.online) {
            val row = doctor?.items?.firstOrNull { it.key == "wake" && it.group == "voice" }
            row?.detail?.takeIf { it.isNotBlank() }?.let { return it }
        }
        return wakeDetail(context)
    }

    /** #NEW-73: one language replies can be spoken in ([value] is stored). */
    data class Language(val value: String, val label: String)

    val languages: List<Language> = listOf(
        Language(value = "en", label = "English"),
        Language(value = "de", label = "Deutsch"),
    )

    /** #NEW-73: the input rows' character limits (`limits`). */
    object Limits {
        const val VOICE_FILE = 512
        const val VOICE_TRANSCRIPT = 500
        /** The wake phrase's cap (`limits.wakePhrase`, `voice.update`'s `oneLine`). */
        const val WAKE_PHRASE = 64
        /** #NEW-56: a stored wake phrase's minimum LETTER count (`limits.wakePhraseMinLetters`). */
        const val WAKE_PHRASE_MIN_LETTERS = 6
    }

    /** The member's ONE settings row (`voice.get`, `voice.update`'s result). */
    @Serializable
    data class Settings(
        @SerialName("voiceDeviceId") val voiceDeviceId: String? = null,
        @SerialName("agentId") val agentId: String? = null,
        /** S-3: the push-to-talk binding (`shortcutGrammar`); empty = off. */
        @SerialName("pttShortcut") val pttShortcut: String = "",
        @SerialName("wakeWord") val wakeWord: Boolean = false,
        @SerialName("wakePhrase") val wakePhrase: String = "",
        @SerialName("language") val language: String = "en",
        @SerialName("voiceFile") val voiceFile: String = "",
        @SerialName("voiceTranscript") val voiceTranscript: String = "",
        /** NEW-57 (`meeting.json`): `off` | `all` | `include` | `exclude`. */
        @SerialName("desktopAudio") val desktopAudio: String = "off",
        /** NEW-57: the apps `include` picks or `exclude` leaves out. */
        @SerialName("desktopAudioApps") val desktopAudioApps: List<AudioApp> = emptyList(),
        /** NEW-57: ISO; null = no meeting (written only by `voice.startMeeting` / `voice.stopMeeting`). */
        @SerialName("meetingStartedAt") val meetingStartedAt: String? = null,
        /** #FEED-163: a meeting stops after this many hours (`Meeting.AutoStop.HOURS_VALUES`). */
        @SerialName("meetingAutoStopHours") val meetingAutoStopHours: Int = Meeting.AutoStop.DEFAULT_HOURS,
    )

    /** The row a member without one reads (`defaults`). */
    val defaults = Settings()

    /** A picked id for the device or the agent; `Pick(null)` = None. */
    data class Pick(val id: String?)

    /**
     * A partial write (`voice.update`): a null field = the key is absent
     * (kept); [voiceDeviceId] / [agentId] = `Pick(null)` for None.
     */
    data class Change(
        val voiceDeviceId: Pick? = null,
        val agentId: Pick? = null,
        val pttShortcut: String? = null,
        val wakeWord: Boolean? = null,
        val wakePhrase: String? = null,
        val language: String? = null,
        val voiceFile: String? = null,
        val voiceTranscript: String? = null,
        /** NEW-57: the Desktop audio row's value (`Meeting.DesktopAudio.wire`). */
        val desktopAudio: String? = null,
        val desktopAudioApps: List<AudioApp>? = null,
        /** #FEED-163: the Stop after row's hours. */
        val meetingAutoStopHours: Int? = null,
    ) {
        /** The wire input: None travels as an explicit `null`; an untouched
         *  field writes no key. */
        fun toJson(): JsonObject = buildJsonObject {
            voiceDeviceId?.let { put("voiceDeviceId", it.id?.let { id -> JsonPrimitive(id) } ?: JsonNull) }
            agentId?.let { put("agentId", it.id?.let { id -> JsonPrimitive(id) } ?: JsonNull) }
            pttShortcut?.let { put("pttShortcut", JsonPrimitive(it)) }
            wakeWord?.let { put("wakeWord", JsonPrimitive(it)) }
            wakePhrase?.let { put("wakePhrase", JsonPrimitive(it)) }
            language?.let { put("language", JsonPrimitive(it)) }
            voiceFile?.let { put("voiceFile", JsonPrimitive(it)) }
            voiceTranscript?.let { put("voiceTranscript", JsonPrimitive(it)) }
            desktopAudio?.let { put("desktopAudio", JsonPrimitive(it)) }
            desktopAudioApps?.let { apps ->
                put(
                    "desktopAudioApps",
                    JsonArray(apps.map { buildJsonObject { put("id", JsonPrimitive(it.id)); put("name", JsonPrimitive(it.name)) } }),
                )
            }
            meetingAutoStopHours?.let { put("meetingAutoStopHours", JsonPrimitive(it)) }
        }
    }

    /**
     * Apply a change; choosing None for the device or the agent clears the
     * wake word, so picking again never turns the always-on microphone on;
     * #NEW-57: and stops a running meeting. S-3: a bound key stays bound.
     */
    fun clearOnNone(before: Settings, change: Change): Settings {
        var after = before.copy(
            voiceDeviceId = if (change.voiceDeviceId != null) change.voiceDeviceId.id else before.voiceDeviceId,
            agentId = if (change.agentId != null) change.agentId.id else before.agentId,
            pttShortcut = change.pttShortcut ?: before.pttShortcut,
            wakeWord = change.wakeWord ?: before.wakeWord,
            wakePhrase = change.wakePhrase ?: before.wakePhrase,
            language = change.language ?: before.language,
            voiceFile = change.voiceFile ?: before.voiceFile,
            voiceTranscript = change.voiceTranscript ?: before.voiceTranscript,
            desktopAudio = change.desktopAudio ?: before.desktopAudio,
            desktopAudioApps = change.desktopAudioApps ?: before.desktopAudioApps,
            meetingAutoStopHours = change.meetingAutoStopHours ?: before.meetingAutoStopHours,
        )
        // NEW-57: None for the device or the agent also stops a meeting.
        if (after.voiceDeviceId == null || after.agentId == null) {
            after = after.copy(wakeWord = false, meetingStartedAt = null)
        }
        return after
    }

    /** One picker option; `id == null` = None. */
    data class Option(val id: String?, val label: String)

    /** One device the member owns ([kind] = contract `deviceKind`). */
    data class DeviceLike(val deviceId: String, val name: String, val kind: String)

    /** One agent the picker can offer; [team] = its team's name. */
    data class AgentLike(val id: String, val name: String, val team: String)

    /** Sorted by label the way the TS reference's `localeCompare(…, "en")` sorts. */
    private fun sortedByLabel(options: List<Option>): List<Option> {
        val collator = Collator.getInstance(Locale.ENGLISH)
        return options.sortedWith { a, b -> collator.compare(a.label, b.label) }
    }

    /**
     * The Voice device picker's options: None first, then the member's
     * desktops by name (phones and servers are never a voice device).
     */
    fun voiceDeviceOptions(devices: List<DeviceLike>): List<Option> {
        val options = devices
            .filter { it.kind == "desktop" }
            .map { Option(id = it.deviceId, label = it.name) }
        return listOf(Option(id = null, label = Copy.VOICE_DEVICE_NONE)) + sortedByLabel(options)
    }

    /**
     * The Agent picker's options: None first, then every agent labelled by
     * its name (`{name} · {team}` when two agents share a name).
     */
    fun agentOptions(agents: List<AgentLike>): List<Option> {
        val counts = agents.groupingBy { it.name }.eachCount()
        val options = agents.map { agent ->
            Option(
                id = agent.id,
                label = if ((counts[agent.name] ?: 0) > 1) "${agent.name} · ${agent.team}" else agent.name,
            )
        }
        return listOf(Option(id = null, label = Copy.AGENT_NONE)) + sortedByLabel(options)
    }

    /** The tray group's header for the voice device. */
    fun trayHeader(deviceName: String): String = Copy.TRAY_HEADER.replace("{device}", deviceName)

    /** The wake phrase's placeholder (and its value when empty). */
    fun wakePhrasePlaceholder(agentName: String): String =
        Copy.WAKE_PHRASE_PLACEHOLDER.replace("{agent}", agentName)

    /**
     * #NEW-75: THE rule of the page's one-line inputs, as you type
     * (`voiceTextInput`): each run of whitespace controls (tab, LF, VT, FF,
     * CR, NEL) becomes one space (#FEED-160), every other control character
     * (Cc: C0, DEL, C1) is dropped, then the value is cut to at most [max]
     * UTF-16 code units (the server's zod count), whole code points only (a
     * surrogate pair straddling the cap is dropped whole). Trimming waits for the save.
     */
    private val SPACE_CONTROLS = Regex("[\\t\\n\\u000B\\f\\r\\u0085]+")

    fun textInput(value: String, max: Int): String {
        val clean = value.replace(SPACE_CONTROLS, " ").filterNot { it.isISOControl() }
        if (clean.length <= max) return clean
        val end = if (max > 0 && Character.isHighSurrogate(clean[max - 1])) max - 1 else max
        return clean.substring(0, end)
    }

    /**
     * #NEW-56: the Wake phrase field's inline error: a trimmed, non-empty
     * phrase with fewer than [Limits.WAKE_PHRASE_MIN_LETTERS] letters (spaces,
     * digits and punctuation don't count). Empty stays valid (the placeholder).
     */
    fun wakePhraseError(text: String): String? {
        val trimmed = text.trim()
        if (trimmed.isEmpty()) return null
        return if (trimmed.count { it.isLetter() } < Limits.WAKE_PHRASE_MIN_LETTERS) Copy.WAKE_PHRASE_TOO_SHORT else null
    }

    /**
     * #NEW-73: the Language row's trailing detail; every language speaks on
     * the voice device's GPU only (#NEW-74). Null without a voice device.
     */
    @Suppress("UNUSED_PARAMETER")
    fun languageNote(language: String, deviceName: String?): String? {
        if (deviceName == null) return null
        return Copy.LANGUAGE_NOTE.replace("{device}", deviceName)
    }

    /** #NEW-73: what the Language row and its menu show for a language. */
    fun languageLabel(language: String, deviceName: String?): String {
        val label = languages.firstOrNull { it.value == language }?.label ?: language
        val note = languageNote(language, deviceName) ?: return label
        return "$label · $note"
    }
}
