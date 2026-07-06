package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

// Mirrors apps/web/src/lib/trpc/projects.ts `create`. v4: a project IS a repo, so
// create always carries a `repository` — either an existing workspace-registry
// repo (`repositoryId`) or a brand-new repo connected inline by `fullName` (the
// server validates the GitHub-App install and upserts the registry row in the
// same transaction). Modeled after iOS `ProjectRepositoryChoice`.

/** The required backing-repo choice for `projects.create`. */
sealed interface ProjectRepositoryChoice {
    /** Target an existing registry repo (same-workspace, not archived). */
    data class Registry(val repositoryId: String) : ProjectRepositoryChoice

    /** Connect a new repo inline by `owner/name`; extra fields seed the registry row. */
    data class Inline(
        val fullName: String,
        val defaultBranch: String? = null,
        val isPrivate: Boolean? = null,
        val installationId: Int? = null,
    ) : ProjectRepositoryChoice

    fun toJson(): JsonObject = when (this) {
        is Registry -> buildJsonObject { put("repositoryId", repositoryId) }
        is Inline -> buildJsonObject {
            put("fullName", fullName)
            defaultBranch?.let { put("defaultBranch", it) }
            isPrivate?.let { put("private", it) }
            installationId?.let { put("installationId", it) }
        }
    }
}

@Serializable
private data class CreatedProject(val id: String)

@Serializable
private data class CreateProjectResult(val project: CreatedProject)

@Singleton
class ProjectsApi @Inject constructor(private val trpc: TrpcClient) {

    /**
     * Create a project. The server uppercases `prefix` and defaults `color` to
     * `#6366f1` when omitted. The inline-connect path needs owner/admin (repo
     * management). Returns the new project id.
     */
    suspend fun create(
        accountId: String,
        workspaceId: String,
        name: String,
        prefix: String,
        color: String?,
        repository: ProjectRepositoryChoice,
    ): String {
        // Built as a raw JsonObject so the `repository` union encodes exactly as
        // the server's `z.union` expects (registry vs inline shapes differ).
        val input: JsonElement = buildJsonObject {
            put("workspaceId", workspaceId)
            put("name", name)
            put("prefix", prefix)
            color?.let { put("color", it) }
            put("repository", repository.toJson())
        }
        return trpc.mutation(
            accountId,
            path = "projects.create",
            input = input,
            inputSerializer = JsonElement.serializer(),
            outputSerializer = CreateProjectResult.serializer(),
        ).project.id
    }
}
