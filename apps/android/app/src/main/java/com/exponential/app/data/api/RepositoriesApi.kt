package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.builtins.nullable

// Mirrors apps/web/src/lib/trpc/repositories.ts + projects.ts. Repositories are
// server-only (NOT an Electric shape) — read on demand over tRPC for the
// workspace-settings registry and the create-project / retarget pickers.
// Connecting NEW repos (the GitHub-App install flow) stays web-only; Android
// links out to the web settings for that (masterplan v4 §6).

/**
 * A project that points at a repo, computed from `projects.repository_id`
 * (masterplan v4 §3.2 — `repositories.list` no longer returns join rows).
 * Powers the settings "used by" chips and the picker "in use" hints.
 */
@Serializable
data class RepoProjectRef(
    val id: String,
    val name: String,
    val slug: String,
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
    // v4: the projects backed by this repo (many for a monorepo).
    val projects: List<RepoProjectRef> = emptyList(),
)

@Serializable
private data class RepoWorkspaceIdInput(val workspaceId: String)

@Serializable
private data class RepositoryIdInput(val repositoryId: String)

@Serializable
private data class SetRepositoryInput(
    val projectId: String,
    val repositoryId: String,
)

@Serializable
private data class BranchDiffInput(@SerialName("issueId") val issueId: String)

@Singleton
class RepositoriesApi @Inject constructor(private val trpc: TrpcClient) {

    /** Member-readable: the workspace's repos with their backing projects. */
    suspend fun list(accountId: String, workspaceId: String): List<WorkspaceRepo> =
        trpc.query(
            accountId,
            path = "repositories.list",
            input = RepoWorkspaceIdInput(workspaceId),
            inputSerializer = RepoWorkspaceIdInput.serializer(),
            outputSerializer = ListSerializer(WorkspaceRepo.serializer()),
        )

    /**
     * Owner-only (server-enforced): remove a repo. Blocked (CONFLICT — "repository
     * backs N projects") while any project still points at it, via the
     * `projects.repository_id` FK `restrict`.
     */
    suspend fun remove(accountId: String, repositoryId: String) =
        trpc.mutationUnit(
            accountId,
            path = "repositories.remove",
            input = RepositoryIdInput(repositoryId),
            inputSerializer = RepositoryIdInput.serializer(),
        )

    /**
     * Owner/admin: retarget a project's backing repo (masterplan v4 §3.2 —
     * `projects.setRepository`, replacing the deleted link/unlink/setPrimary).
     */
    suspend fun setRepository(accountId: String, projectId: String, repositoryId: String) =
        trpc.mutationUnit(
            accountId,
            path = "projects.setRepository",
            input = SetRepositoryInput(projectId, repositoryId),
            inputSerializer = SetRepositoryInput.serializer(),
        )

    /**
     * Member-gated middle tier of remote Changes visibility (masterplan v4 §4.8,
     * L18): the issue's `exp/<IDENTIFIER>` branch compared against the repo
     * default branch, returned in the shared `prFiles` shape (reuses [PrFilesResult]).
     * Null when the branch was never pushed (the caller falls through to the
     * "being coded on <device>" tier).
     */
    suspend fun branchDiff(accountId: String, issueId: String): PrFilesResult? =
        trpc.query(
            accountId,
            path = "repositories.branchDiff",
            input = BranchDiffInput(issueId),
            inputSerializer = BranchDiffInput.serializer(),
            outputSerializer = PrFilesResult.serializer().nullable,
        )
}
