package com.exponential.app.domain

import java.text.Collator
import java.util.Locale
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject

/**
 * NEW-67 (voice plan v3, V1): THE Settings → Voice section. A port of the TS
 * reference `packages/domain-contract/src/voice.ts` (desktop `domain::voice`,
 * iOS `Voice.swift`), byte-locked by `fixtures/voice.json` (`copy`,
 * `testIds`, `shortcuts`, `defaults` and the option builders, replayed by
 * `VoiceTest`). The phones only show and edit the member's setting: speaking
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
        const val SPEAK_REPLIES = "Speak replies"
        const val TRAY_HEADER = "On {device} (tray)"
        const val PUSH_TO_TALK = "Push to talk"
        const val SHORTCUT = "Shortcut"
        const val WAKE_WORD = "Wake word"
        const val WAKE_PHRASE = "Wake phrase"
        const val WAKE_PHRASE_PLACEHOLDER = "Hey {agent}"
        const val DOWNLOADING = "Downloading voice…"
        const val DOWNLOAD_FAILED = "Couldn't download the voice"
        const val UNAVAILABLE = "The voice stopped working"
        const val SPEAKING = "Speaking"
        const val STOP = "Stop"
        const val LINK = "link"
        const val ELLIPSIS = "…"
    }

    object TestIds {
        const val PAGE = "settings-voice"
        const val VOICE_DEVICE = "settings-voice-device"
        const val AGENT = "settings-voice-agent"
        const val SPEAK_REPLIES = "settings-voice-speak-replies"
        const val PUSH_TO_TALK = "settings-voice-push-to-talk"
        const val SHORTCUT = "settings-voice-shortcut"
        const val WAKE_WORD = "settings-voice-wake-word"
        const val WAKE_PHRASE = "settings-voice-wake-phrase"
    }

    const val MAX_CHARS = 600

    /** One push-to-talk shortcut ([value] is what is stored). */
    data class Shortcut(val value: String, val label: String)

    val shortcuts: List<Shortcut> = listOf(
        Shortcut(value = "ctrl+space", label = "Ctrl + Space"),
        Shortcut(value = "alt+space", label = "Alt + Space"),
        Shortcut(value = "ctrl+shift+space", label = "Ctrl + Shift + Space"),
        Shortcut(value = "f13", label = "F13"),
        Shortcut(value = "mouse4", label = "Mouse button 4"),
        Shortcut(value = "mouse5", label = "Mouse button 5"),
    )

    /** The member's ONE settings row (`voice.get`, `voice.update`'s result). */
    @Serializable
    data class Settings(
        @SerialName("voiceDeviceId") val voiceDeviceId: String? = null,
        @SerialName("agentId") val agentId: String? = null,
        @SerialName("speakReplies") val speakReplies: Boolean = false,
        @SerialName("pushToTalk") val pushToTalk: Boolean = false,
        @SerialName("pttShortcut") val pttShortcut: String = "ctrl+space",
        @SerialName("wakeWord") val wakeWord: Boolean = false,
        @SerialName("wakePhrase") val wakePhrase: String = "",
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
        val speakReplies: Boolean? = null,
        val pushToTalk: Boolean? = null,
        val pttShortcut: String? = null,
        val wakeWord: Boolean? = null,
        val wakePhrase: String? = null,
    ) {
        /** The wire input: None travels as an explicit `null`; an untouched
         *  field writes no key. */
        fun toJson(): JsonObject = buildJsonObject {
            voiceDeviceId?.let { put("voiceDeviceId", it.id?.let { id -> JsonPrimitive(id) } ?: JsonNull) }
            agentId?.let { put("agentId", it.id?.let { id -> JsonPrimitive(id) } ?: JsonNull) }
            speakReplies?.let { put("speakReplies", JsonPrimitive(it)) }
            pushToTalk?.let { put("pushToTalk", JsonPrimitive(it)) }
            pttShortcut?.let { put("pttShortcut", JsonPrimitive(it)) }
            wakeWord?.let { put("wakeWord", JsonPrimitive(it)) }
            wakePhrase?.let { put("wakePhrase", JsonPrimitive(it)) }
        }
    }

    /**
     * Apply a change; choosing None for the device or the agent clears every
     * switch, so picking again never turns listening or speaking on.
     */
    fun clearOnNone(before: Settings, change: Change): Settings {
        var after = before.copy(
            voiceDeviceId = if (change.voiceDeviceId != null) change.voiceDeviceId.id else before.voiceDeviceId,
            agentId = if (change.agentId != null) change.agentId.id else before.agentId,
            speakReplies = change.speakReplies ?: before.speakReplies,
            pushToTalk = change.pushToTalk ?: before.pushToTalk,
            pttShortcut = change.pttShortcut ?: before.pttShortcut,
            wakeWord = change.wakeWord ?: before.wakeWord,
            wakePhrase = change.wakePhrase ?: before.wakePhrase,
        )
        if (after.voiceDeviceId == null || after.agentId == null) {
            after = after.copy(speakReplies = false, pushToTalk = false, wakeWord = false)
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
}
