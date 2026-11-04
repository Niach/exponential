package com.exponential.app.data.api

import com.exponential.app.data.db.TeamEntity
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

// `team` is null when the user has no non-feedback membership — signup no
// longer auto-creates a team (EXP-188), so callers route null to the
// create-or-join choice instead of expecting a heal.
@Serializable
data class GetDefaultResult(val team: TeamEntity? = null)

@Serializable
data class CreateTeamInput(val name: String)

// The server also returns `txId` (Electric sync bookkeeping for the web
// client); ignoreUnknownKeys drops it.
@Serializable
data class CreateTeamResult(val team: TeamEntity)

@Serializable
data class DeleteTeamInput(val teamId: String)

@Serializable
data class DeleteBoardInput(val boardId: String)

// EXP-725: how many more people this team may invite — pending invites count.
// `remaining` is null for an UNLIMITED team (self-hosted, or a paid/comp
// tier), which is not the same as 0. Any member may ask.
@Serializable
data class InviteCapacityInput(val teamId: String)

@Serializable
data class InviteCapacityResult(val remaining: Int? = null)

@Serializable
private object EmptyInput

/** `teams.update` (owner-only): only the fields set are written. */
@Serializable
data class UpdateTeamInput(
    val teamId: String,
    val name: String? = null,
    val yoloMode: Boolean? = null,
    val agentPrompt: String? = null,
)

/**
 * VAPP-201: `teams.update {teamId, timeZone}`. The router applies a field only
 * when its key is PRESENT, so clearing the zone has to travel as a literal
 * `null`, which the shared Json's `explicitNulls = false` would drop off a
 * `@Serializable` class (the `updateBoardBranchInput` story).
 */
internal fun updateTeamTimeZoneInput(teamId: String, timeZone: String?): JsonObject = buildJsonObject {
    put("teamId", teamId)
    put("timeZone", timeZone?.let(::JsonPrimitive) ?: JsonNull)
}

/** #EXP-1645: `teams.update {teamId, iconUrl: null}`: the same literal null. */
internal fun updateTeamIconInput(teamId: String): JsonObject = buildJsonObject {
    put("teamId", teamId)
    put("iconUrl", JsonNull)
}

@Serializable
private data class TeamIdInput(val teamId: String)

/** EXP-1025: the team prompt (server-only, member-readable). */
@Serializable
data class TeamAgentPrompt(
    val agentPrompt: String = "",
    val agentPromptUpdatedAt: String? = null,
    val maxBytes: Int? = null,
)

@Singleton
class TeamsApi @Inject constructor(
    private val trpc: TrpcClient,
    private val uploads: IssueImagesApi,
) {

    /** Oldest non-feedback membership team, or null when the user has none
     * (`teams.getDefault` — the non-creating replacement for ensureDefault). */
    suspend fun getDefault(accountId: String): TeamEntity? =
        trpc.query(
            accountId,
            path = "teams.getDefault",
            input = EmptyInput,
            inputSerializer = EmptyInput.serializer(),
            outputSerializer = GetDefaultResult.serializer(),
        ).team

    /** Open to every authed user (EXP-188) — the creator becomes owner. */
    suspend fun create(accountId: String, name: String): TeamEntity =
        trpc.mutation(
            accountId,
            path = "teams.create",
            input = CreateTeamInput(name),
            inputSerializer = CreateTeamInput.serializer(),
            outputSerializer = CreateTeamResult.serializer(),
        ).team

    /** Free invite seats left, or null when the team has no cap (EXP-725). */
    suspend fun inviteCapacity(accountId: String, teamId: String): Int? =
        trpc.query(
            accountId,
            path = "teams.inviteCapacity",
            input = InviteCapacityInput(teamId),
            inputSerializer = InviteCapacityInput.serializer(),
            outputSerializer = InviteCapacityResult.serializer(),
        ).remaining

    /** Owner-only: name, yolo mode, the team prompt (`teams.update`). */
    suspend fun update(accountId: String, input: UpdateTeamInput) {
        trpc.mutationUnit(
            accountId,
            path = "teams.update",
            input = input,
            inputSerializer = UpdateTeamInput.serializer(),
        )
    }

    /** VAPP-201: set (or, null, clear) the team time zone; owners only. */
    suspend fun updateTimeZone(accountId: String, teamId: String, timeZone: String?) {
        trpc.mutationUnit(
            accountId,
            path = "teams.update",
            input = updateTeamTimeZoneInput(teamId, timeZone),
            inputSerializer = JsonElement.serializer(),
        )
    }

    /** #EXP-1645: the team image, owners only; answers the new `iconUrl`. */
    suspend fun uploadIcon(
        accountId: String,
        teamId: String,
        bytes: ByteArray,
        filename: String,
        contentType: String,
    ): String = uploads.uploadTeamIcon(accountId, teamId, bytes, filename, contentType)

    /** #EXP-1645: remove the team image, owners only. */
    suspend fun removeIcon(accountId: String, teamId: String) {
        trpc.mutationUnit(
            accountId,
            path = "teams.update",
            input = updateTeamIconInput(teamId),
            inputSerializer = JsonElement.serializer(),
        )
    }

    suspend fun getAgentPrompt(accountId: String, teamId: String): TeamAgentPrompt =
        trpc.query(
            accountId,
            path = "teams.getAgentPrompt",
            input = TeamIdInput(teamId),
            inputSerializer = TeamIdInput.serializer(),
            outputSerializer = TeamAgentPrompt.serializer(),
        )

    suspend fun delete(accountId: String, teamId: String) {
        trpc.mutationUnit(
            accountId,
            path = "teams.delete",
            input = DeleteTeamInput(teamId),
            inputSerializer = DeleteTeamInput.serializer(),
        )
    }

    suspend fun deleteBoard(accountId: String, boardId: String) {
        trpc.mutationUnit(
            accountId,
            path = "boards.delete",
            input = DeleteBoardInput(boardId),
            inputSerializer = DeleteBoardInput.serializer(),
        )
    }
}
