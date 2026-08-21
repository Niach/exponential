package com.exponential.app.data.api

import com.exponential.app.data.db.ActionEntity
import com.exponential.app.domain.DomainContract
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.Json

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
    val builtin: Boolean? = null,
) {
    /** Whether this is the virtual builtin "Create action" row. */
    val isBuiltin: Boolean get() = builtin == true

    /** Whether an automation can target this action (EXP-583): a real team
     * row (never a builtin) whose every input is optional — an automated run
     * has nobody to type a required one, and the server refuses to enable
     * such an automation. */
    val automatable: Boolean
        get() = !isBuiltin && inputs.orEmpty().none { it.required }
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
)
