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

@Serializable
data class LinkRepoInput(
    @SerialName("projectId") val projectId: String,
    val repo: String,
)

@Serializable
data class UnlinkRepoInput(@SerialName("projectId") val projectId: String)

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

    suspend fun linkGithubRepo(accountId: String, projectId: String, repo: String): ProjectEntity =
        trpc.mutation(
            accountId,
            path = "projects.linkGithubRepo",
            input = LinkRepoInput(projectId, repo),
            inputSerializer = LinkRepoInput.serializer(),
            outputSerializer = ProjectResult.serializer(),
        ).project

    suspend fun unlinkGithubRepo(accountId: String, projectId: String): ProjectEntity =
        trpc.mutation(
            accountId,
            path = "projects.unlinkGithubRepo",
            input = UnlinkRepoInput(projectId),
            inputSerializer = UnlinkRepoInput.serializer(),
            outputSerializer = ProjectResult.serializer(),
        ).project
}
