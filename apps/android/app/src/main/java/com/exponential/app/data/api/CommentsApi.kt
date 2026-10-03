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
    // EXP-741: the top-level comment this one replies to. Omitted when null,
    // exactly like attachmentIds.
    @SerialName("parentId") val parentId: String? = null,
    // SLOP-4: `reporter` = the server emails the comment to the widget
    // reporter (accepted only top-level, on an issue whose submission has a
    // reporter email). Omitted when null — a plain team comment, which older
    // servers without the field parse unchanged.
    @SerialName("audience") val audience: String? = null,
)

/**
 * SLOP-4: what `comments.create` answers beyond the row. [reporterEmailed] =
 * true (mailed), false (saved, no transport / failed), null (a team comment,
 * or an older server without the field — nothing to toast).
 */
@Serializable
data class CreateCommentResult(
    @SerialName("reporterEmailed") val reporterEmailed: Boolean? = null,
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
        parentId: String? = null,
        audience: String? = null,
    ): CreateCommentResult =
        trpc.mutation(
            accountId,
            path = "comments.create",
            input = CreateCommentInput(issueId, text, attachmentIds, parentId, audience),
            inputSerializer = CreateCommentInput.serializer(),
            outputSerializer = CreateCommentResult.serializer(),
        )

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
