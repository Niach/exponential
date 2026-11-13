package com.exponential.app.data.api

import com.exponential.app.data.db.ComposerDraftEntity
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * #FEED-219 (fixture `composer-draft.json`): `composerDrafts.upsert` /
 * `composerDrafts.delete`. The server owns `user_id` and the timestamps and
 * refuses an empty text (that is a delete, [com.exponential.app.domain.ComposerDraft.write]).
 */
@Serializable
data class UpsertComposerDraftInput(
    @SerialName("teamId") val teamId: String,
    val key: String,
    val text: String,
)

@Serializable
data class UpsertComposerDraftResult(val draft: ComposerDraftEntity)

@Serializable
data class ComposerDraftKeyInput(val key: String)

@Singleton
class ComposerDraftsApi @Inject constructor(private val trpc: TrpcClient) {

    suspend fun upsert(accountId: String, input: UpsertComposerDraftInput): ComposerDraftEntity =
        trpc.mutation(
            accountId,
            path = "composerDrafts.upsert",
            input = input,
            inputSerializer = UpsertComposerDraftInput.serializer(),
            outputSerializer = UpsertComposerDraftResult.serializer(),
        ).draft

    suspend fun delete(accountId: String, key: String) {
        trpc.mutationUnit(
            accountId,
            path = "composerDrafts.delete",
            input = ComposerDraftKeyInput(key),
            inputSerializer = ComposerDraftKeyInput.serializer(),
        )
    }
}
