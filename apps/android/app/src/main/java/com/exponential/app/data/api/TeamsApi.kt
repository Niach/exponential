package com.exponential.app.data.api

import com.exponential.app.data.db.TeamEntity
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable

@Serializable
data class EnsureDefaultResult(val team: TeamEntity)

@Serializable
data class DeleteTeamInput(val teamId: String)

@Serializable
data class DeleteBoardInput(val boardId: String)

@Serializable
private object EmptyInput

@Singleton
class TeamsApi @Inject constructor(private val trpc: TrpcClient) {

    suspend fun ensureDefault(accountId: String): TeamEntity =
        trpc.mutation(
            accountId,
            path = "teams.ensureDefault",
            input = EmptyInput,
            inputSerializer = EmptyInput.serializer(),
            outputSerializer = EnsureDefaultResult.serializer(),
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
