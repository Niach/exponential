package com.exponential.app.data.api

import com.exponential.app.data.db.IssueDraftEntity
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer

/**
 * `issueDrafts.upsert` (EXP-878). The id is minted CLIENT-side by the create
 * screen, so the same draft row is written over and over (open → close → open)
 * without ever forking; the server owns `user_id` and the timestamps.
 *
 * `labelIds` is ALWAYS sent (never omitted): the shared Json drops nulls, so a
 * nullable list could never clear the selection — an empty list is the honest
 * "no labels" write.
 */
@Serializable
data class UpsertIssueDraftInput(
    val id: String,
    @SerialName("teamId") val teamId: String,
    @SerialName("boardId") val boardId: String,
    val title: String,
    val description: String,
    @SerialName("statusId") val statusId: String? = null,
    val priority: String? = null,
    @SerialName("assigneeId") val assigneeId: String? = null,
    @SerialName("labelIds") val labelIds: List<String> = emptyList(),
    /** `YYYY-MM-DD`, or omitted for no due date. */
    @SerialName("dueDate") val dueDate: String? = null,
)

@Serializable
data class UpsertIssueDraftResult(val draft: IssueDraftEntity)

@Serializable
data class IssueDraftIdInput(val id: String)

/**
 * One attachment already uploaded against a draft. The draft-owned rows are
 * deliberately NOT in the attachments shape (it is scoped to
 * `issue_id IS NOT NULL`), so re-opening a draft reads them back over tRPC
 * instead of from Room.
 */
@Serializable
data class IssueDraftAttachment(
    val id: String,
    val filename: String,
    @SerialName("contentType") val contentType: String = "application/octet-stream",
    @SerialName("sizeBytes") val sizeBytes: Long = 0,
    val url: String = "",
    @SerialName("createdAt") val createdAt: String = "",
)

@Singleton
class IssueDraftsApi @Inject constructor(private val trpc: TrpcClient) {

    suspend fun upsert(accountId: String, input: UpsertIssueDraftInput): IssueDraftEntity =
        trpc.mutation(
            accountId,
            path = "issueDrafts.upsert",
            input = input,
            inputSerializer = UpsertIssueDraftInput.serializer(),
            outputSerializer = UpsertIssueDraftResult.serializer(),
        ).draft

    suspend fun delete(accountId: String, id: String) {
        trpc.mutationUnit(
            accountId,
            path = "issueDrafts.delete",
            input = IssueDraftIdInput(id),
            inputSerializer = IssueDraftIdInput.serializer(),
        )
    }

    /** The draft's own attachments (owner-only) — the Files section on re-open. */
    suspend fun listAttachments(accountId: String, id: String): List<IssueDraftAttachment> =
        trpc.query(
            accountId,
            path = "issueDrafts.listAttachments",
            input = IssueDraftIdInput(id),
            inputSerializer = IssueDraftIdInput.serializer(),
            outputSerializer = ListSerializer(IssueDraftAttachment.serializer()),
        )
}
