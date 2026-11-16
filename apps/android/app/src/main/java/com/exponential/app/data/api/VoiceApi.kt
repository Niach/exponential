package com.exponential.app.data.api

import com.exponential.app.domain.Voice
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject

// Mirrors apps/web/src/lib/trpc/voice.ts (NEW-67): the member's ONE voice
// settings row, server-only like the notification prefs (no shape).
// S-3: the server still answers the legacy `speakReplies` / `pushToTalk` to
// old clients; the injected Json is `ignoreUnknownKeys`, so this build reads
// past them, and `Voice.Change` never writes either (push to talk = a bound
// `pttShortcut`, speak-back always on).
@Serializable
private object VoiceEmptyInput

/** NEW-69: `voice.startCall`'s input (a phone never names a deviceId). */
@Serializable
private data class StartCallInput(val agentId: String, val platform: String)

/** NEW-57: `voice.startMeeting`'s input: the consent dialog's Start, always `true`. */
@Serializable
private data class StartMeetingInput(val consent: Boolean)

/** #EXP-1859: `voice.wakeAgent`'s input (a team agent's Wake word row). */
@Serializable
private data class WakeAgentInput(val agentId: String, val on: Boolean)

/** NEW-69: the caller's relay ticket; [url] is dialled as-is. */
@Serializable
data class StartCallResult(
    val callId: String,
    val ticket: String,
    val url: String,
    val relayUrl: String? = null,
    /** EXP-1857: the target desktop the server picked (absent from an older server). */
    val deviceId: String? = null,
    /** EXP-1857: its name (the call's `{device}`). */
    val device: String? = null,
    /** EXP-1857: the target is not the agent's own desktop. */
    val fallback: Boolean? = null,
)

@Singleton
class VoiceApi @Inject constructor(private val trpc: TrpcClient) {

    /** The member's settings; the fixture's defaults when no row exists. */
    suspend fun get(accountId: String): Voice.Settings =
        trpc.query(
            accountId,
            path = "voice.get",
            input = VoiceEmptyInput,
            inputSerializer = VoiceEmptyInput.serializer(),
            outputSerializer = Voice.Settings.serializer(),
        )

    /** A partial write; the server applies `clearOnNone` and returns the row. */
    suspend fun update(accountId: String, change: Voice.Change): Voice.Settings =
        trpc.mutation(
            accountId,
            path = "voice.update",
            input = change.toJson(),
            inputSerializer = JsonObject.serializer(),
            outputSerializer = Voice.Settings.serializer(),
        )

    /**
     * NEW-69: start a call with [agentId]; EXP-1857 the server picks the
     * target (`fixtures/voice-call.json` `callTarget`: the agent's desktop or
     * the fallback) and refuses with its reason.
     */
    suspend fun startCall(accountId: String, agentId: String, platform: String = "phone"): StartCallResult =
        trpc.mutation(
            accountId,
            path = "voice.startCall",
            input = StartCallInput(agentId = agentId, platform = platform),
            inputSerializer = StartCallInput.serializer(),
            outputSerializer = StartCallResult.serializer(),
        )

    /**
     * NEW-57: THE consent dialog's Start (`meeting.json`); the server refuses
     * without a voice device and an agent. Answers the row.
     */
    suspend fun startMeeting(accountId: String): Voice.Settings =
        trpc.mutation(
            accountId,
            path = "voice.startMeeting",
            input = StartMeetingInput(consent = true),
            inputSerializer = StartMeetingInput.serializer(),
            outputSerializer = Voice.Settings.serializer(),
        )

    /** NEW-57: Meeting mode off, at once (no dialog). Answers the row. */
    suspend fun stopMeeting(accountId: String): Voice.Settings =
        trpc.mutation(
            accountId,
            path = "voice.stopMeeting",
            input = VoiceEmptyInput,
            inputSerializer = VoiceEmptyInput.serializer(),
            outputSerializer = Voice.Settings.serializer(),
        )

    /**
     * #EXP-1859: a team agent's Wake word row flipped (`AgentWake.toggle` on
     * the server, one write); refusals = BAD_REQUEST with the copy,
     * NOT_FOUND. Answers the row.
     */
    suspend fun wakeAgent(accountId: String, agentId: String, on: Boolean): Voice.Settings =
        trpc.mutation(
            accountId,
            path = "voice.wakeAgent",
            input = WakeAgentInput(agentId = agentId, on = on),
            inputSerializer = WakeAgentInput.serializer(),
            outputSerializer = Voice.Settings.serializer(),
        )
}
