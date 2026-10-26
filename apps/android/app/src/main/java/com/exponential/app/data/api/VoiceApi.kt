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
}
