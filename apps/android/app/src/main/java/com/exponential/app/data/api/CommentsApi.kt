package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class CreateCommentInput(
    @SerialName("issueId") val issueId: String,
    val body: String,
    // EXP-554: the attachments to link to the new comment. The shared Json has
    // `explicitNulls = false`, so null is OMITTED from the wire body rather
    // than sent as `null` — older servers keep parsing the input.
    @SerialName("attachmentIds") val attachmentIds: List<String>? = null,
)

@Serializable
data class UpdateCommentInput(
    val id: String,
    val body: String,
    // Omitted = attachments untouched (what the MCP tools rely on). An array is
    // the FULL desired set: rows missing from it are hard-deleted server-side.
    @SerialName("attachmentIds") val attachmentIds: List<String>? = null,
)

@Serializable
data class DeleteCommentInput(val id: String)

@Singleton
class CommentsApi @Inject constructor(private val trpc: TrpcClient) {

    suspend fun create(
        accountId: String,
        issueId: String,
        text: String,
        attachmentIds: List<String>? = null,
    ) {
        trpc.mutationUnit(
            accountId,
            path = "comments.create",
            input = CreateCommentInput(issueId, text, attachmentIds),
            inputSerializer = CreateCommentInput.serializer(),
        )
    }

    suspend fun update(
        accountId: String,
        id: String,
        text: String,
        attachmentIds: List<String>? = null,
    ) {
        trpc.mutationUnit(
            accountId,
            path = "comments.update",
            input = UpdateCommentInput(id, text, attachmentIds),
            inputSerializer = UpdateCommentInput.serializer(),
        )
    }

    suspend fun delete(accountId: String, id: String) {
        trpc.mutationUnit(
            accountId,
            path = "comments.delete",
            input = DeleteCommentInput(id),
            inputSerializer = DeleteCommentInput.serializer(),
        )
    }
}

// Extract the `{ "text": "..." }` field from a JSONB comment body stored as
// stringified JSON by Electric. Mirrors getCommentBodyText in the web app.
fun getCommentBodyText(body: String?): String {
    if (body.isNullOrBlank()) return ""
    return try {
        val element = kotlinx.serialization.json.Json.parseToJsonElement(body)
        val obj = element as? kotlinx.serialization.json.JsonObject
        val text = obj?.get("text") as? kotlinx.serialization.json.JsonPrimitive
        text?.content ?: body
    } catch (_: Throwable) {
        body
    }
}
