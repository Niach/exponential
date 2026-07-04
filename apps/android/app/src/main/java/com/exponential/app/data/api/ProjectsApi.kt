package com.exponential.app.data.api

import com.exponential.app.data.db.ProjectEntity
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

// v4 (masterplan §3.2): a project is backed by a repository. Android only ever
// targets an ALREADY-connected registry repo (`{ repositoryId }`); the inline
// `{ fullName }` connect path — the GitHub-App install flow — stays web-only.
@Serializable
data class RepositoryRef(val repositoryId: String)

@Serializable
data class CreateProjectInput(
    @SerialName("workspaceId") val workspaceId: String,
    val name: String,
    val prefix: String,
    val color: String? = null,
    val repository: RepositoryRef,
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
