package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer

// Mirrors apps/web/src/lib/trpc/repositories.ts. Repositories are server-only
// (NOT an Electric shape) — read on demand over tRPC for the workspace-settings
// registry. Connecting NEW repos (the GitHub-App install flow) stays web-only;
// Android links out to the web settings for that.

/** A project ↔ repo link (`project_repositories` join row). */
@Serializable
data class RepoProjectLink(
    val projectId: String,
    val isPrimary: Boolean = false,
)

/**
 * One connected repo in the workspace registry (`repositories.list` row).
 * `private` is a Kotlin keyword so it's mapped via @SerialName.
 */
@Serializable
data class WorkspaceRepo(
    val id: String,
    val fullName: String,
    val defaultBranch: String = "main",
    @SerialName("private") val isPrivate: Boolean = false,
    val projectLinks: List<RepoProjectLink> = emptyList(),
)

@Serializable
private data class RepoWorkspaceIdInput(val workspaceId: String)

@Serializable
private data class RepositoryIdInput(val repositoryId: String)

@Serializable
private data class ProjectRepositoryInput(
    val projectId: String,
    val repositoryId: String,
)

@Singleton
class RepositoriesApi @Inject constructor(private val trpc: TrpcClient) {

    /** Member-readable: the workspace's repos with their project links. */
    suspend fun list(accountId: String, workspaceId: String): List<WorkspaceRepo> =
        trpc.query(
            accountId,
            path = "repositories.list",
            input = RepoWorkspaceIdInput(workspaceId),
            inputSerializer = RepoWorkspaceIdInput.serializer(),
            outputSerializer = ListSerializer(WorkspaceRepo.serializer()),
        )

    /** Owner-only (server-enforced): remove a repo; project links cascade. */
    suspend fun remove(accountId: String, repositoryId: String) =
        trpc.mutationUnit(
            accountId,
            path = "repositories.remove",
            input = RepositoryIdInput(repositoryId),
            inputSerializer = RepositoryIdInput.serializer(),
        )

    /** Owner-only: link a repo to a project. */
    suspend fun linkProject(accountId: String, projectId: String, repositoryId: String) =
        trpc.mutationUnit(
            accountId,
            path = "repositories.linkProject",
            input = ProjectRepositoryInput(projectId, repositoryId),
            inputSerializer = ProjectRepositoryInput.serializer(),
        )

    /** Owner-only: unlink a repo from a project. */
    suspend fun unlinkProject(accountId: String, projectId: String, repositoryId: String) =
        trpc.mutationUnit(
            accountId,
            path = "repositories.unlinkProject",
            input = ProjectRepositoryInput(projectId, repositoryId),
            inputSerializer = ProjectRepositoryInput.serializer(),
        )

    /** Owner-only: make a linked repo the project's primary clone target. */
    suspend fun setPrimary(accountId: String, projectId: String, repositoryId: String) =
        trpc.mutationUnit(
            accountId,
            path = "repositories.setPrimary",
            input = ProjectRepositoryInput(projectId, repositoryId),
            inputSerializer = ProjectRepositoryInput.serializer(),
        )
}
