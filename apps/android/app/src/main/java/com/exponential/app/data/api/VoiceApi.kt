package com.exponential.app.data.api

import com.exponential.app.domain.Voice
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject

// Mirrors apps/web/src/lib/trpc/voice.ts (NEW-67): the member's ONE voice
// settings row, server-only like the notification prefs (no shape).
@Serializable
private object VoiceEmptyInput

/** NEW-69: `voice.startCall`'s input (a phone never names a deviceId). */
@Serializable
private data class StartCallInput(val agentId: String, val platform: String)

/** NEW-69: the caller's relay ticket; [url] is dialled as-is. */
@Serializable
data class StartCallResult(
    val callId: String,
    val ticket: String,
    val url: String,
    val relayUrl: String? = null,
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
     * NEW-69: start a call with [agentId] streamed to the member's voice
     * device; the server refuses with the availability reason
     * (`fixtures/voice-call.json` `callAvailability`).
     */
    suspend fun startCall(accountId: String, agentId: String, platform: String = "phone"): StartCallResult =
        trpc.mutation(
            accountId,
            path = "voice.startCall",
            input = StartCallInput(agentId = agentId, platform = platform),
            inputSerializer = StartCallInput.serializer(),
            outputSerializer = StartCallResult.serializer(),
        )
}
