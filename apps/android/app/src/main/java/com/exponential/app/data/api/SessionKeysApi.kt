package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer

// EXP-1458 tRPC `sessionKeys` (fixtures/sealed-content.json KEY API): every
// call names an install the caller owns (not revoked).

@Serializable
data class TreeKeyWrap(val installId: String, val wrapped: String)

@Serializable
data class CreateSessionKeyInput(
    val rootId: String,
    val installId: String,
    val epoch: Int,
    val publicKey: String,
    val wraps: List<TreeKeyWrap>,
)

@Serializable
data class MineSessionKeysInput(val installId: String, val rootIds: List<String>? = null)

/** One wrap this install holds. */
@Serializable
data class HeldTreeKey(val rootId: String, val epoch: Int, val wrapped: String)

@Serializable
data class PendingSessionKeysInput(val installId: String, val limit: Int = PENDING_MAX)

/** A reader of a tree this install holds the current epoch of, still without its wrap. */
@Serializable
data class PendingTreeKey(
    val rootId: String,
    val epoch: Int,
    val publicKey: String,
    val targetInstallId: String,
    val targetX25519Public: String,
)

@Serializable
data class PendingWrap(val rootId: String, val epoch: Int, val targetInstallId: String, val wrapped: String)

@Serializable
data class WrapSessionKeysInput(val installId: String, val wraps: List<PendingWrap>)

@Serializable
data class WrapSessionKeysResult(val written: Int = 0, val skipped: Int = 0)

@Serializable
data class PublicKeysInput(val rootIds: List<String>)

@Serializable
data class TreePublicKey(val rootId: String, val epoch: Int, val publicKey: String)

const val PENDING_MAX = 200

@Singleton
class SessionKeysApi @Inject constructor(private val trpc: TrpcClient) {
    suspend fun create(accountId: String, input: CreateSessionKeyInput) {
        trpc.mutationUnit(accountId, path = "sessionKeys.create", input = input, inputSerializer = CreateSessionKeyInput.serializer())
    }

    suspend fun mine(accountId: String, installId: String, rootIds: List<String>? = null): List<HeldTreeKey> =
        trpc.query(
            accountId,
            path = "sessionKeys.mine",
            input = MineSessionKeysInput(installId, rootIds),
            inputSerializer = MineSessionKeysInput.serializer(),
            outputSerializer = ListSerializer(HeldTreeKey.serializer()),
        )

    suspend fun pending(accountId: String, installId: String): List<PendingTreeKey> =
        trpc.query(
            accountId,
            path = "sessionKeys.pending",
            input = PendingSessionKeysInput(installId),
            inputSerializer = PendingSessionKeysInput.serializer(),
            outputSerializer = ListSerializer(PendingTreeKey.serializer()),
        )

    suspend fun wrap(accountId: String, installId: String, wraps: List<PendingWrap>): WrapSessionKeysResult =
        trpc.mutation(
            accountId,
            path = "sessionKeys.wrap",
            input = WrapSessionKeysInput(installId, wraps),
            inputSerializer = WrapSessionKeysInput.serializer(),
            outputSerializer = WrapSessionKeysResult.serializer(),
        )

    suspend fun publicKeys(accountId: String, rootIds: List<String>): List<TreePublicKey> =
        trpc.query(
            accountId,
            path = "sessionKeys.publicKeys",
            input = PublicKeysInput(rootIds),
            inputSerializer = PublicKeysInput.serializer(),
            outputSerializer = ListSerializer(TreePublicKey.serializer()),
        )
}
