package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

// Mirrors apps/web/src/lib/trpc/pins.ts (EXP-778) — the ONE pins endpoint.
// `toggle` pins the target when absent and unpins it when present; the row
// itself arrives (or leaves) through the synced `pins` shape, so the answer
// only tells the caller which way it flipped.
@Serializable
private data class PinToggleInput(
    @SerialName("teamId") val teamId: String,
    // Contract `pinKind`: issue | session | action (DomainContract.pinKind*).
    @SerialName("kind") val kind: String,
    @SerialName("targetId") val targetId: String,
)

/** `pins.toggle`'s answer (the txId is unused here, ignoreUnknownKeys drops it). */
@Serializable
data class PinToggleResult(
    @SerialName("pinned") val pinned: Boolean,
)

@Singleton
class PinsApi @Inject constructor(private val trpc: TrpcClient) {

    suspend fun toggle(accountId: String, teamId: String, kind: String, targetId: String): PinToggleResult =
        trpc.mutation(
            accountId,
            path = "pins.toggle",
            input = PinToggleInput(teamId = teamId, kind = kind, targetId = targetId),
            inputSerializer = PinToggleInput.serializer(),
            outputSerializer = PinToggleResult.serializer(),
        )
}
