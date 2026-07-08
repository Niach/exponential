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
data class WorkspaceBySlugInput(val slug: String)

/// Minimal projection returned by the public-aware `workspaces.getBySlug`
/// lookup. `hasPublicBoard` is true when the workspace hosts at least one
/// public feedback-board project; `membership` is the caller's role, null for
/// non-members (mirrors the web route guard).
@Serializable
data class WorkspaceBySlugResult(
    val id: String,
    val name: String,
    val slug: String,
    val iconUrl: String? = null,
    val hasPublicBoard: Boolean = false,
    val membership: String? = null,
)

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

    /// Public-aware workspace lookup by slug. Resolves workspaces the user has
    /// NOT synced yet (public boards only sync after an explicit join), so the
    /// app can offer the in-app join gate. Throws NOT_FOUND for private
    /// workspaces the caller can't access.
    suspend fun getBySlug(accountId: String, slug: String): WorkspaceBySlugResult =
        trpc.query(
            accountId,
            path = "workspaces.getBySlug",
            input = WorkspaceBySlugInput(slug),
            inputSerializer = WorkspaceBySlugInput.serializer(),
            outputSerializer = WorkspaceBySlugResult.serializer(),
        )

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
