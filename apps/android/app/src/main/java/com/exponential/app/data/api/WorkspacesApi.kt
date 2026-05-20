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
private object EmptyInput

@Singleton
class WorkspacesApi @Inject constructor(private val trpc: TrpcClient) {

    suspend fun ensureDefault(): WorkspaceEntity =
        trpc.mutation(
            path = "workspaces.ensureDefault",
            input = EmptyInput,
            inputSerializer = EmptyInput.serializer(),
            outputSerializer = EnsureDefaultResult.serializer(),
        ).workspace

    suspend fun update(input: UpdateWorkspaceInput) {
        trpc.mutation(
            path = "workspaces.update",
            input = input,
            inputSerializer = UpdateWorkspaceInput.serializer(),
            outputSerializer = kotlinx.serialization.json.JsonElement.serializer(),
        )
    }
}
