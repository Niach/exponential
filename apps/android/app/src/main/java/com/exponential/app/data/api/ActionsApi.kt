package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable

// Mirrors apps/web/src/lib/trpc/actions.ts (EXP-253). Team action prompts are
// tRPC-only — NOT an Electric shape: clients fetch on demand, and only the
// desktop ever executes a body (behind its per-device trust prompt). Mobile is
// view + run only: it lists actions and remote-starts them on a desktop via
// `steer.startSession({actionId})` — the body itself never matters here.

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
 * One team action (`actions.list` row). [repositoryId] is null for repo-less
 * actions (the desktop runs those in a scratch dir); [description] is the
 * optional one-liner under the name. [inputs]/[builtin] are EXP-257 additions
 * (nullable defaults so old servers still decode): [builtin] marks the
 * server-appended virtual "Create action" row
 * ([DomainContract.builtinCreateActionId][com.exponential.app.domain.DomainContract]),
 * which clients pin FIRST by this flag — never by sort order. The shared Json
 * has ignoreUnknownKeys, so server additions never break the decode.
 */
@Serializable
data class ActionDto(
    val id: String,
    val teamId: String,
    val repositoryId: String? = null,
    val name: String,
    val description: String? = null,
    val body: String = "",
    val sortOrder: Double = 0.0,
    val createdAt: String = "",
    val updatedAt: String = "",
    val inputs: List<ActionInputDto>? = null,
    val builtin: Boolean? = null,
) {
    /** Whether this is the server-defined virtual builtin row. */
    val isBuiltin: Boolean get() = builtin == true
}

/** Server envelope: `actions.list` returns `{ actions: [<row>] }`. */
@Serializable
data class ActionsListResult(val actions: List<ActionDto> = emptyList())

@Serializable
private data class ActionsListInput(val teamId: String)

@Singleton
class ActionsApi @Inject constructor(private val trpc: TrpcClient) {

    /** Member-gated `actions.list` — the team's actions, sortOrder-then-name ordered server-side. */
    suspend fun list(accountId: String, teamId: String): List<ActionDto> =
        trpc.query(
            accountId,
            path = "actions.list",
            input = ActionsListInput(teamId),
            inputSerializer = ActionsListInput.serializer(),
            outputSerializer = ActionsListResult.serializer(),
        ).actions
}
