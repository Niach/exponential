package com.exponential.app.data.api

import com.exponential.app.data.db.ActionEntity
import com.exponential.app.domain.ActionTrigger
import com.exponential.app.domain.DomainContract
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject

// Team action prompts (EXP-253). Since EXP-268 actions are Electric-synced
// (the 15th shape — see SyncManager/ActionEntity), so consumers list them
// LIVE from the local Room flow instead of tRPC `actions.list`; the ≤64KB
// markdown `body` is deliberately excluded from sync (tRPC `actions.get`
// stays the only body path — mobile never needs it: it is view + run only,
// remote-starting actions on a desktop via `steer.startSession({actionId})`).

/**
 * One typed run input an action declares (EXP-257): the run sheet renders a
 * field per def ([type] `text` | `repo` | `board`) and sends the filled values
 * with `steer.startSession`. [required] defaults false (absent = optional).
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
 * One team action row. [repositoryId] is null for repo-less actions (the
 * desktop runs those in a scratch dir); [description] is the optional
 * one-liner under the name. [body] is always empty on mobile (excluded from
 * sync — nothing in the list/run UI needs it). [builtin] marks the virtual
 * "Create action" row ([builtinCreateAction] — id
 * [DomainContract.builtinCreateActionId]), which clients PREPEND themselves
 * now that rows come from the synced shape (the server used to append it in
 * `actions.list`); the pin-first rule keys off this flag, never sort order.
 */
@Serializable
data class ActionDto(
    val id: String,
    val teamId: String,
    val repositoryId: String? = null,
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
     * EXP-530: the action's optional automation trigger as its raw JSON
     * string (null on the builtins). Parse via [ActionTrigger.parse] —
     * tolerant, so an unknown future kind reads as "no automation".
     */
    val trigger: String? = null,
    val builtin: Boolean? = null,
) {
    /** Whether this is the virtual builtin "Create action" row. */
    val isBuiltin: Boolean get() = builtin == true

    /** The parsed automation trigger, or null (absent/unknown/malformed). */
    val parsedTrigger: ActionTrigger? get() = ActionTrigger.parse(trigger)
}

/**
 * The virtual builtin "Create action" row (EXP-257): describe a new action in
 * a text input and let your agent author it for the team. Synced rows can't carry
 * it, so every consumer PREPENDS this factory's row to the local-flow list.
 */
fun builtinCreateAction(teamId: String): ActionDto = ActionDto(
    id = DomainContract.builtinCreateActionId,
    teamId = teamId,
    name = "Create action",
    description = "Describe a new action and let your agent author it for the team",
    icon = "sparkles",
    inputs = listOf(
        ActionInputDto(
            key = "description",
            label = "Description",
            type = "text",
            required = true,
            placeholder = "What should this action do?",
        ),
        ActionInputDto(
            key = "repo",
            label = "Repository",
            type = "repo",
            required = false,
        ),
        // EXP-273: the author picks the new action's glyph up front.
        ActionInputDto(
            key = "icon",
            label = "Icon",
            type = "icon",
            required = false,
        ),
    ),
    sortOrder = 1e9,
    builtin = true,
)

/**
 * The virtual builtin "Fix merge conflicts" row (EXP-259, mobile parity
 * EXP-270): pick a conflicted open pull request and let the desktop rebase,
 * resolve, push and merge it. The `pr` input's value is the REPRESENTATIVE
 * issue id of an issue-linked open PR (batch PRs dedupe by prUrl). Mirrors
 * apps/web/src/lib/builtin-actions.ts field-for-field.
 */
fun builtinFixConflictsAction(teamId: String): ActionDto = ActionDto(
    id = DomainContract.builtinFixConflictsId,
    teamId = teamId,
    name = "Fix merge conflicts",
    description = "Pick a conflicted pull request and let your agent rebase, resolve, and merge it",
    icon = "git-branch",
    inputs = listOf(
        ActionInputDto(
            key = "pr",
            label = "Pull request",
            type = "pr",
            required = true,
        ),
    ),
    sortOrder = 1e9 + 1,
    builtin = true,
)

/**
 * Both builtins in the order every client pins them. EXP-270: mobile used to
 * construct only "Create action", so "Fix merge conflicts" silently vanished
 * from Android when EXP-268 moved the list onto the synced shape.
 */
fun builtinActions(teamId: String): List<ActionDto> =
    listOf(builtinCreateAction(teamId), builtinFixConflictsAction(teamId))

/**
 * Map a synced [ActionEntity] row to the UI's [ActionDto], parsing the stored
 * `inputs` JSON string with the shared lenient [json] (ignoreUnknownKeys — a
 * malformed/unknown defs array degrades to no inputs, never a crash).
 */
fun ActionEntity.toActionDto(json: Json): ActionDto = ActionDto(
    id = id,
    teamId = teamId,
    repositoryId = repositoryId,
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
    trigger = trigger,
)

@Serializable
private data class UpdateTriggerInput(
    val id: String,
    val trigger: JsonObject,
)

/**
 * The one tRPC call mobile makes against the `actions` router (EXP-530): the
 * Automations tab's enabled toggle. Everything else stays view-only off the
 * synced shape (the SteerApi transport pattern).
 */
@Singleton
class ActionsApi @Inject constructor(private val trpc: TrpcClient) {

    /**
     * `actions.update({id, trigger})` — flip an automation's paused flag.
     * The server replaces the WHOLE trigger object (never merges), so the
     * caller sends the parsed trigger with `enabled` flipped. Owner-only
     * server-side; Electric echoes the flipped row back into the actions
     * shape, so success needs no local write.
     */
    suspend fun updateTrigger(accountId: String, actionId: String, trigger: ActionTrigger) {
        trpc.mutationUnit(
            accountId,
            path = "actions.update",
            input = UpdateTriggerInput(id = actionId, trigger = trigger.toWireJson()),
            inputSerializer = UpdateTriggerInput.serializer(),
        )
    }
}
