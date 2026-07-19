package com.exponential.app.data.api

import com.exponential.app.data.db.TeamEntity
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable

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

@Serializable
private object EmptyInput

@Singleton
class TeamsApi @Inject constructor(private val trpc: TrpcClient) {

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
