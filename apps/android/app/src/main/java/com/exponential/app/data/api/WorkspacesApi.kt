package com.exponential.app.data.api

import com.exponential.app.data.db.WorkspaceEntity
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable

@Serializable
data class EnsureDefaultResult(val workspace: WorkspaceEntity)

@Serializable
data class DeleteWorkspaceInput(val workspaceId: String)

@Serializable
data class DeleteProjectInput(val projectId: String)

@Serializable
private object EmptyInput

@Singleton
class WorkspacesApi @Inject constructor(private val trpc: TrpcClient) {

    suspend fun ensureDefault(accountId: String): WorkspaceEntity =
        trpc.mutation(
            accountId,
            path = "workspaces.ensureDefault",
            input = EmptyInput,
            inputSerializer = EmptyInput.serializer(),
            outputSerializer = EnsureDefaultResult.serializer(),
        ).workspace

    suspend fun delete(accountId: String, workspaceId: String) {
        trpc.mutationUnit(
            accountId,
            path = "workspaces.delete",
            input = DeleteWorkspaceInput(workspaceId),
            inputSerializer = DeleteWorkspaceInput.serializer(),
        )
    }

    suspend fun deleteProject(accountId: String, projectId: String) {
        trpc.mutationUnit(
            accountId,
            path = "projects.delete",
            input = DeleteProjectInput(projectId),
            inputSerializer = DeleteProjectInput.serializer(),
        )
    }
}
