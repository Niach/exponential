package com.exponential.app.data.api

import com.exponential.app.data.db.ActionEntity
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

// Team action prompts (EXP-253). Since EXP-268 actions are Electric-synced
// (the 15th shape — see SyncManager/ActionEntity), so consumers list them
// LIVE from the local Room flow instead of tRPC `actions.list`; the ≤64KB
// markdown `body` is deliberately excluded from sync (tRPC `actions.get`
// stays the only body path — the mobile editor (EXP-694) fetches it when the
// sheet opens, exactly like the web dialog and the desktop editor do).

/**
 * One typed input a saved prompt declares (EXP-257): the composer renders a
 * pick per def ([type] = contract `actionInputType`: `repo` | `board` | `pr`
 * | `issue`) and sends the filled values as `sessions.send({actionId,
 * inputs})`. [required] defaults false (absent = optional). [placeholder]
 * survives on the wire for older rows.
 */
@Serializable
data class ActionInputDto(
    val key: String,
    val label: String,
    val type: String,
    val required: Boolean = false,
    val placeholder: String? = null,
)

/**
 * One team action = a SAVED PROMPT (EXP-1291): name, icon, markdown [body],
 * typed [inputs] and the composer hint. [description] is the optional
 * one-liner under the name. [body] is always empty off the synced shape
 * (excluded from sync — `actions.get` is the only body path). No triggers
 * (they live on agents now), no repository, no builtins.
 */
@Serializable
data class ActionDto(
    val id: String,
    val teamId: String,
    val name: String,
    val description: String? = null,
    /** EXP-273: curated registry icon name; null = the generic action glyph. */
    val icon: String? = null,
    val body: String = "",
    val sortOrder: Double = 0.0,
    val createdAt: String = "",
    val updatedAt: String = "",
    val inputs: List<ActionInputDto>? = null,
    /**
     * EXP-825: the composer's field hint while this action is picked (≤200
     * chars); null = the generic "Additional instructions (optional)…".
     */
    val promptPlaceholder: String? = null,
) {
    /** Whether the action declares a REQUIRED input. */
    val hasRequiredInputs: Boolean get() = inputs.orEmpty().any { it.required }

    companion object {
        /**
         * The server's cap on [promptPlaceholder] (`MAX_ACTION_PROMPT_PLACEHOLDER`
         * in db-schema/domain.ts) — the editor refuses at the field, not at
         * submit.
         */
        const val PROMPT_PLACEHOLDER_MAX_LENGTH = 200
    }
}

/** `actions.get`'s / `.update`'s answer — the full row, body included. */
@Serializable
data class ActionResult(val action: ActionDto)

@Serializable
private data class ActionIdInput(val id: String)

/**
 * The editor's patch (EXP-694): every editable field travels, and a null
 * [description]/[icon] rides as an EXPLICIT JSON null meaning
 * "cleared" — exactly what the web dialog sends. The router applies a key only
 * when it is `!== undefined`, so an OMITTED key means "keep", and the shared
 * Json (`explicitNulls = false`) would drop a null property from a
 * `@Serializable` class outright — hence the hand-built object with [JsonNull].
 * `inputs` and `sortOrder` are deliberately
 * absent: mobile edits neither, and an omitted key leaves the stored value
 * alone. EXP-825: [promptPlaceholder] (the composer hint) follows the
 * description's rule — null clears.
 */
internal fun updateActionInput(
    id: String,
    name: String,
    description: String?,
    icon: String?,
    body: String,
    promptPlaceholder: String?,
): JsonObject = buildJsonObject {
    put("id", id)
    put("name", name)
    put("description", description?.let(::JsonPrimitive) ?: JsonNull)
    put("icon", icon?.let(::JsonPrimitive) ?: JsonNull)
    put("body", body)
    put("promptPlaceholder", promptPlaceholder?.let(::JsonPrimitive) ?: JsonNull)
}

/**
 * The action WRITE path (EXP-694 — mobile got the full editor). Reads stay on
 * the synced shape; only the body, which sync excludes by design, and the
 * owner-gated update come through tRPC.
 */
@Singleton
class ActionsApi @Inject constructor(private val trpc: TrpcClient) {

    /**
     * `actions.get` — the ONLY body path (member-gated). Builtins have no
     * server row: the caller must never ask for one (the server refuses).
     */
    suspend fun get(accountId: String, id: String): ActionDto = trpc.query(
        accountId,
        path = "actions.get",
        input = ActionIdInput(id = id),
        inputSerializer = ActionIdInput.serializer(),
        outputSerializer = ActionResult.serializer(),
    ).action

    /**
     * `actions.update` from the edit sheet — owner-only server-side. Null
     * [description]/[icon]/[promptPlaceholder] CLEAR those
     * fields (explicit JSON nulls, see [updateActionInput]); Electric echoes
     * the new metadata back into the shape, so a success needs no local write.
     */
    suspend fun update(
        accountId: String,
        id: String,
        name: String,
        description: String?,
        icon: String?,
        body: String,
        promptPlaceholder: String?,
    ): ActionDto = trpc.mutation(
        accountId,
        path = "actions.update",
        input = updateActionInput(
            id = id,
            name = name,
            description = description,
            icon = icon,
            body = body,
            promptPlaceholder = promptPlaceholder,
        ),
        inputSerializer = JsonObject.serializer(),
        outputSerializer = ActionResult.serializer(),
    ).action

}

/**
 * Map a synced [ActionEntity] row to the UI's [ActionDto], parsing the stored
 * `inputs` JSON string with the shared lenient [json] (ignoreUnknownKeys — a
 * malformed/unknown defs array degrades to no inputs, never a crash).
 */
fun ActionEntity.toActionDto(json: Json): ActionDto = ActionDto(
    id = id,
    teamId = teamId,
    name = name,
    description = description,
    icon = icon,
    sortOrder = sortOrder,
    createdAt = createdAt,
    updatedAt = updatedAt,
    inputs = inputs?.let { raw ->
        runCatching {
            json.decodeFromString(ListSerializer(ActionInputDto.serializer()), raw)
        }.getOrNull()
    },
    promptPlaceholder = promptPlaceholder,
)
