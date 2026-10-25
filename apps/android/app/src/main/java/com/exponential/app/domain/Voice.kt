package com.exponential.app.domain

import java.text.Collator
import java.util.Locale

/**
 * NEW-53 exponentialTalk: the voice settings (`fixtures/voice.json`, TS
 * `packages/domain-contract/src/voice.ts`). The phones mirror `copy`,
 * `testIds` and the Assistant picker's options; only the desktop speaks in
 * this slice, so `shouldSpeak` / `spokenText` are not mirrored here.
 */
object Voice {

    object Copy {
        const val GROUP = "Voice"
        const val ASSISTANT = "Assistant"
        const val ASSISTANT_NONE = "None"
        const val SPEAK_REPLIES = "Speak replies"
        const val DOWNLOADING = "Downloading voice…"
        const val DOWNLOAD_FAILED = "Couldn't download the voice"
        const val UNAVAILABLE = "The voice stopped working"
        const val SPEAKING = "Speaking"
        const val STOP = "Stop"
        const val LINK = "link"
        const val ELLIPSIS = "…"
    }

    object TestIds {
        const val GROUP = "device-voice"
        const val ASSISTANT = "device-voice-assistant"
        const val SPEAK_REPLIES = "device-voice-speak-replies"
    }

    const val MAX_CHARS = 600

    /** One agent the picker can offer; [team] = its team's name. */
    data class AgentLike(val id: String, val name: String, val team: String)

    /** One Assistant option; `id == null` = no assistant. */
    data class AssistantOption(val id: String?, val label: String)

    /**
     * The Assistant picker's options: None first, then every agent labelled
     * by its name (`{name} · {team}` when two agents share a name), sorted by
     * label the way the TS reference's `localeCompare(…, "en")` sorts.
     */
    fun assistantOptions(agents: List<AgentLike>): List<AssistantOption> {
        val counts = agents.groupingBy { it.name }.eachCount()
        val collator = Collator.getInstance(Locale.ENGLISH)
        val options = agents
            .map { agent ->
                AssistantOption(
                    id = agent.id,
                    label = if ((counts[agent.name] ?: 0) > 1) "${agent.name} · ${agent.team}" else agent.name,
                )
            }
            .sortedWith { a, b -> collator.compare(a.label, b.label) }
        return listOf(AssistantOption(id = null, label = Copy.ASSISTANT_NONE)) + options
    }
}
