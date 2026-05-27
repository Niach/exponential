package com.exponential.app.data.api

import com.exponential.app.data.db.WorkspaceEntity
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class EnsureDefaultResult(val workspace: WorkspaceEntity)

@Serializable
data class UpdateWorkspaceInput(
    val id: String,
    val name: String? = null,
    @SerialName("isPublic") val isPublic: Boolean? = null,
    @SerialName("publicWritePolicy") val publicWritePolicy: String? = null,
    @SerialName("iconUrl") val iconUrl: String? = null,
)

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

    suspend fun update(accountId: String, input: UpdateWorkspaceInput) {
        trpc.mutation(
            accountId,
            path = "workspaces.update",
            input = input,
            inputSerializer = UpdateWorkspaceInput.serializer(),
            outputSerializer = kotlinx.serialization.json.JsonElement.serializer(),
        )
    }

    suspend fun delete(accountId: String, workspaceId: String) {
        trpc.mutation(
            accountId,
            path = "workspaces.delete",
            input = DeleteWorkspaceInput(workspaceId),
            inputSerializer = DeleteWorkspaceInput.serializer(),
            outputSerializer = kotlinx.serialization.json.JsonElement.serializer(),
        )
    }

    suspend fun deleteProject(accountId: String, projectId: String) {
        trpc.mutation(
            accountId,
            path = "projects.delete",
            input = DeleteProjectInput(projectId),
            inputSerializer = DeleteProjectInput.serializer(),
            outputSerializer = kotlinx.serialization.json.JsonElement.serializer(),
        )
    }
}
