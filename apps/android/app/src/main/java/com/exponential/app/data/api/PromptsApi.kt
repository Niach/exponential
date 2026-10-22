package com.exponential.app.data.api

import com.exponential.app.data.db.PromptEntity
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

// NEW-2: saved PROMPTS replace actions (apps/web/src/lib/trpc/prompts.ts):
// a name, a curated icon and a markdown body, nothing else (no inputs, no
// composer hint, no triggers, no repository). The rows sync through the
// `prompts` shape WITHOUT the body; `prompts.get` is the only body path.
// Members read, owners write. Running one = `sessions.send({promptId, text})`
// (the composer's `/` menu).

/** One saved prompt; [body] is empty off the synced shape. */
@Serializable
data class PromptDto(
    val id: String,
    val teamId: String,
    val name: String,
    /** A curated registry icon name; null = the generic prompt glyph. */
    val icon: String? = null,
    val body: String = "",
    val sortOrder: Double = 0.0,
    val createdAt: String = "",
    val updatedAt: String = "",
)

/** `prompts.get`'s / the write echoes' answer: the full row, body included. */
@Serializable
data class PromptResult(val prompt: PromptDto)

@Serializable
data class PromptListResult(val prompts: List<PromptDto> = emptyList())

@Serializable
private data class PromptIdInput(val id: String)

@Serializable
private data class PromptTeamInput(val teamId: String)

/** The editor's fields, the create / update payload. */
data class PromptFields(
    val name: String,
    val icon: String?,
    val body: String,
)

/** `prompts.create` (strict): `icon` omitted when blank. */
internal fun createPromptInput(teamId: String, fields: PromptFields): JsonObject = buildJsonObject {
    put("teamId", teamId)
    put("name", fields.name.trim())
    fields.icon?.takeIf { it.isNotEmpty() }?.let { put("icon", it) }
    put("body", fields.body)
}

/** `prompts.update` (strict): every editable field travels; a blank icon is
 *  an EXPLICIT null (cleared), the router applies a key only when present. */
internal fun updatePromptInput(id: String, fields: PromptFields): JsonObject = buildJsonObject {
    put("id", id)
    put("name", fields.name.trim())
    put("icon", fields.icon?.takeIf { it.isNotEmpty() }?.let(::JsonPrimitive) ?: JsonNull)
    put("body", fields.body)
}

@Singleton
class PromptsApi @Inject constructor(private val trpc: TrpcClient) {

    /** Member read: a team's prompts (clients normally read the shape). */
    suspend fun list(accountId: String, teamId: String): List<PromptDto> = trpc.query(
        accountId,
        path = "prompts.list",
        input = PromptTeamInput(teamId),
        inputSerializer = PromptTeamInput.serializer(),
        outputSerializer = PromptListResult.serializer(),
    ).prompts

    /** Member read: the full row, body included (the ONLY body path). */
    suspend fun get(accountId: String, id: String): PromptDto = trpc.query(
        accountId,
        path = "prompts.get",
        input = PromptIdInput(id),
        inputSerializer = PromptIdInput.serializer(),
        outputSerializer = PromptResult.serializer(),
    ).prompt

    /** Owner-only. */
    suspend fun create(accountId: String, teamId: String, fields: PromptFields): PromptDto = trpc.mutation(
        accountId,
        path = "prompts.create",
        input = createPromptInput(teamId, fields),
        inputSerializer = JsonObject.serializer(),
        outputSerializer = PromptResult.serializer(),
    ).prompt

    /** Owner-only; Electric echoes the new metadata back into the shape. */
    suspend fun update(accountId: String, id: String, fields: PromptFields): PromptDto = trpc.mutation(
        accountId,
        path = "prompts.update",
        input = updatePromptInput(id, fields),
        inputSerializer = JsonObject.serializer(),
        outputSerializer = PromptResult.serializer(),
    ).prompt

    /** Owner-only, permanent. */
    suspend fun delete(accountId: String, id: String) {
        trpc.mutation(
            accountId,
            path = "prompts.delete",
            input = PromptIdInput(id),
            inputSerializer = PromptIdInput.serializer(),
            outputSerializer = JsonElement.serializer(),
        )
    }
}

/** A synced [PromptEntity] row as the UI's [PromptDto] (body-less). */
fun PromptEntity.toPromptDto(): PromptDto = PromptDto(
    id = id,
    teamId = teamId,
    name = name,
    icon = icon,
    sortOrder = sortOrder,
    createdAt = createdAt,
    updatedAt = updatedAt,
)
