package com.exponential.app.ui.actions

import com.exponential.app.data.api.ActionInputDto
import com.exponential.app.data.api.ActionResult
import com.exponential.app.data.api.TrpcClient
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

// EXP-1296: the saved-prompt WRITES (`apps/web/src/lib/trpc/actions.ts`):
// create, a full update (inputs included) and delete, all team-OWNER-only
// server-side. Reads stay on the synced `actions` shape + `actions.get`.

/** The editor's fields, the create/update payload. */
data class ActionFields(
    val name: String,
    val icon: String?,
    val body: String,
    val inputs: List<ActionInputDto>,
    val promptPlaceholder: String?,
)

internal fun inputsJson(inputs: List<ActionInputDto>): JsonArray = JsonArray(
    inputs.map { def ->
        buildJsonObject {
            put("key", def.key)
            put("label", def.label.trim())
            put("type", def.type)
            put("required", def.required)
            def.placeholder?.trim()?.takeIf { it.isNotEmpty() }?.let { put("placeholder", it) }
        }
    },
)

/** `actions.create` — `icon`/`promptPlaceholder` omitted when blank. */
internal fun createActionInput(teamId: String, fields: ActionFields): JsonObject = buildJsonObject {
    put("teamId", teamId)
    put("name", fields.name.trim())
    fields.icon?.takeIf { it.isNotEmpty() }?.let { put("icon", it) }
    put("body", fields.body)
    put("inputs", inputsJson(fields.inputs))
    fields.promptPlaceholder?.trim()?.takeIf { it.isNotEmpty() }?.let { put("promptPlaceholder", it) }
}

/** `actions.update` — every field travels; a blank icon/hint is an EXPLICIT
 *  null (cleared), the router applies a key only when present. */
internal fun updateActionFieldsInput(id: String, fields: ActionFields): JsonObject = buildJsonObject {
    put("id", id)
    put("name", fields.name.trim())
    put("icon", fields.icon?.takeIf { it.isNotEmpty() }?.let(::JsonPrimitive) ?: JsonNull)
    put("body", fields.body)
    put("inputs", inputsJson(fields.inputs))
    put(
        "promptPlaceholder",
        fields.promptPlaceholder?.trim()?.takeIf { it.isNotEmpty() }?.let(::JsonPrimitive) ?: JsonNull,
    )
}

/**
 * A snake_case input key off its label (`^[a-z][a-z0-9_]{0,31}$`), unique
 * among [taken] (a `_2` suffix on a clash).
 */
internal fun actionInputKey(label: String, taken: Set<String>): String {
    val slug = label.lowercase()
        .replace(Regex("[^a-z0-9]+"), "_")
        .trim('_')
        .replace(Regex("^[0-9_]+"), "")
        .ifEmpty { "input" }
        .take(28)
    if (slug !in taken) return slug
    var n = 2
    while ("${slug}_$n" in taken) n++
    return "${slug}_$n"
}

@Singleton
class ActionWrites @Inject constructor(private val trpc: TrpcClient) {

    suspend fun create(accountId: String, teamId: String, fields: ActionFields) = trpc.mutation(
        accountId,
        path = "actions.create",
        input = createActionInput(teamId, fields),
        inputSerializer = JsonObject.serializer(),
        outputSerializer = ActionResult.serializer(),
    ).action

    suspend fun update(accountId: String, id: String, fields: ActionFields) = trpc.mutation(
        accountId,
        path = "actions.update",
        input = updateActionFieldsInput(id, fields),
        inputSerializer = JsonObject.serializer(),
        outputSerializer = ActionResult.serializer(),
    ).action

    suspend fun delete(accountId: String, id: String) {
        trpc.mutation(
            accountId,
            path = "actions.delete",
            input = buildJsonObject { put("id", id) },
            inputSerializer = JsonObject.serializer(),
            outputSerializer = JsonElement.serializer(),
        )
    }
}
