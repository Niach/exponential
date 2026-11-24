package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer

// EXP-1458 tRPC `sessionKeys` (fixtures/sealed-content.json KEY API): every
// call names an install the caller owns (not revoked).

/** #EXP-1950: [signature] = this install's over the wrap (`enet_core::wrap_sig`); null = unsigned, omitted on the wire. */
@Serializable
data class TreeKeyWrap(val installId: String, val wrapped: String, val signature: String? = null)

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
data class HeldTreeKey(
    val rootId: String,
    val epoch: Int,
    val wrapped: String,
    /** #EXP-1950: who signed it (both null = unsigned; an older server sends neither). */
    val signature: String? = null,
    val signerEndpointId: String? = null,
)

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
    /** #EXP-1950: the target's ed25519 key a wrap signature names (null from a server older than #3394: unsigned). */
    val targetEndpointId: String? = null,
)

@Serializable
data class PendingWrap(val rootId: String, val epoch: Int, val targetInstallId: String, val wrapped: String, val signature: String? = null)

/** #EXP-1950 (wrap-signature.json `resign`): a fresh signed wrap of a root's current epoch for [installId], the TARGET. */
@Serializable
data class ResignWrap(val rootId: String, val epoch: Int, val installId: String, val wrapped: String, val signature: String)

@Serializable
data class ResignInput(val installId: String, val wraps: List<ResignWrap>)

@Serializable
data class ResignResult(val replaced: Int = 0, val skipped: Int = 0)

@Serializable
data class WrapSessionKeysInput(val installId: String, val wraps: List<PendingWrap>)

@Serializable
data class WrapSessionKeysResult(val written: Int = 0, val skipped: Int = 0)

@Serializable
data class PublicKeysInput(val rootIds: List<String>)

@Serializable
data class TreePublicKey(val rootId: String, val epoch: Int, val publicKey: String)

/** #EXP-1960 `sessionKeys.requestKey`: `state` = keyed | clear | requested | noHost. */
@Serializable
data class RequestKeyInput(val rootId: String)

@Serializable
data class RequestKeyResult(val state: String, val epoch: Int? = null, val publicKey: String? = null)

/** #EXP-1951 `sessionKeys.due`: a root this install rotates; [epoch] = the one to supersede. */
@Serializable
data class DueTreeKey(val rootId: String, val epoch: Int)

const val PENDING_MAX = 200

/** `resign.maxWraps`. */
const val RESIGN_MAX = 200

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

    /** #EXP-1951 (sealed-content.json `rotation`): the roots this install rotates now. */
    suspend fun due(accountId: String, installId: String): List<DueTreeKey> =
        trpc.query(
            accountId,
            path = "sessionKeys.due",
            input = PendingSessionKeysInput(installId),
            inputSerializer = PendingSessionKeysInput.serializer(),
            outputSerializer = ListSerializer(DueTreeKey.serializer()),
        )

    /** #EXP-1950: replaces readers' UNSIGNED rows of a root's current epoch with these signed wraps. */
    suspend fun resign(accountId: String, installId: String, wraps: List<ResignWrap>): ResignResult =
        trpc.mutation(
            accountId,
            path = "sessionKeys.resign",
            input = ResignInput(installId, wraps),
            inputSerializer = ResignInput.serializer(),
            outputSerializer = ResignResult.serializer(),
        )

    /** #EXP-1960: a keyless tree this install may not key: its devices key it. */
    suspend fun requestKey(accountId: String, rootId: String): RequestKeyResult =
        trpc.mutation(
            accountId,
            path = "sessionKeys.requestKey",
            input = RequestKeyInput(rootId),
            inputSerializer = RequestKeyInput.serializer(),
            outputSerializer = RequestKeyResult.serializer(),
        )
}
