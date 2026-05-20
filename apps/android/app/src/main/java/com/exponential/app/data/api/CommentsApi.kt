package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class CommentBody(val text: String)

@Serializable
data class CreateCommentInput(
    @SerialName("issueId") val issueId: String,
    val body: CommentBody,
)

@Serializable
data class UpdateCommentInput(
    val id: String,
    val body: CommentBody,
)

@Serializable
data class DeleteCommentInput(val id: String)

@Singleton
class CommentsApi @Inject constructor(private val trpc: TrpcClient) {

    suspend fun create(issueId: String, text: String) {
        trpc.mutation(
            path = "comments.create",
            input = CreateCommentInput(issueId, CommentBody(text)),
            inputSerializer = CreateCommentInput.serializer(),
            outputSerializer = kotlinx.serialization.json.JsonElement.serializer(),
        )
    }

    suspend fun update(id: String, text: String) {
        trpc.mutation(
            path = "comments.update",
            input = UpdateCommentInput(id, CommentBody(text)),
            inputSerializer = UpdateCommentInput.serializer(),
            outputSerializer = kotlinx.serialization.json.JsonElement.serializer(),
        )
    }

    suspend fun delete(id: String) {
        trpc.mutation(
            path = "comments.delete",
            input = DeleteCommentInput(id),
            inputSerializer = DeleteCommentInput.serializer(),
            outputSerializer = kotlinx.serialization.json.JsonElement.serializer(),
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
