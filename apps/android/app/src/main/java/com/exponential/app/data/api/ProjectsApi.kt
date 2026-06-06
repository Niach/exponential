package com.exponential.app.data.api

import com.exponential.app.data.db.ProjectEntity
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class CreateProjectInput(
    @SerialName("workspaceId") val workspaceId: String,
    val name: String,
    val prefix: String,
    val color: String? = null,
    val repo: String? = null,
)

@Serializable
data class ProjectResult(val project: ProjectEntity)

@Singleton
class ProjectsApi @Inject constructor(private val trpc: TrpcClient) {

    suspend fun create(accountId: String, input: CreateProjectInput): ProjectEntity =
        trpc.mutation(
            accountId,
            path = "projects.create",
            input = input,
            inputSerializer = CreateProjectInput.serializer(),
            outputSerializer = ProjectResult.serializer(),
        ).project
}
