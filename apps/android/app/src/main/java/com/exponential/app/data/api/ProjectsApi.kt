package com.exponential.app.data.api

import com.exponential.app.data.db.ProjectEntity
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.put

// Mirrors apps/web/src/lib/trpc/projects.ts `create`. A repository is OPTIONAL on
// every project (EXP-121); when supplied it's either an existing workspace-registry
// repo (`repositoryId`) or a brand-new repo connected inline by `fullName` (the
// server validates the GitHub-App install and upserts the registry row in the
// same transaction). Modeled after iOS `ProjectRepositoryChoice`.

/** The optional backing-repo choice for `projects.create`. */
sealed interface ProjectRepositoryChoice {
    /** Target an existing registry repo (same-workspace, not archived). */
    data class Registry(val repositoryId: String) : ProjectRepositoryChoice

    /**
     * Connect a new repo inline by `owner/name`; extra fields seed the registry
     * row. The installation id is resolved server-side — never sent by clients.
     */
    data class Inline(
        val fullName: String,
        val defaultBranch: String? = null,
        val isPrivate: Boolean? = null,
    ) : ProjectRepositoryChoice

    fun toJson(): JsonObject = when (this) {
        is Registry -> buildJsonObject { put("repositoryId", repositoryId) }
        is Inline -> buildJsonObject {
            put("fullName", fullName)
            defaultBranch?.let { put("defaultBranch", it) }
            isPrivate?.let { put("private", it) }
        }
    }
}

@Serializable
private data class CreateProjectResult(val project: JsonObject)

/**
 * `projects.create` result: the new project's id (always present) plus the
 * full row when the server response decodes into a [ProjectEntity] (the server
 * returns every column via `.returning()`). The entity drives the optimistic
 * local Room upsert (EXP-46); it's null on older/self-hosted servers whose
 * response is missing required fields — callers then just wait for Electric.
 */
data class CreatedProjectResult(val id: String, val entity: ProjectEntity?)

@Singleton
class ProjectsApi @Inject constructor(
    private val trpc: TrpcClient,
    private val json: Json,
) {

    /**
     * Create a project. The server uppercases `prefix` and defaults `color` to
     * `#6366f1` when omitted. Since the project-type collapse (EXP-121) we send
     * `icon` (curated contract name) instead of the legacy `type`; a
     * `repository` is OPTIONAL on every project. The inline-connect path needs
     * owner/admin (repo management). Returns the new project id plus the full
     * row when decodable (see [CreatedProjectResult]).
     */
    suspend fun create(
        accountId: String,
        workspaceId: String,
        name: String,
        prefix: String,
        color: String?,
        icon: String,
        repository: ProjectRepositoryChoice?,
    ): CreatedProjectResult {
        // Built as a raw JsonObject so the `repository` union encodes exactly as
        // the server's `z.union` expects (registry vs inline shapes differ).
        val input: JsonElement = buildJsonObject {
            put("workspaceId", workspaceId)
            put("name", name)
            put("prefix", prefix)
            color?.let { put("color", it) }
            put("icon", icon)
            repository?.let { put("repository", it.toJson()) }
        }
        val project = trpc.mutation(
            accountId,
            path = "projects.create",
            input = input,
            inputSerializer = JsonElement.serializer(),
            outputSerializer = CreateProjectResult.serializer(),
        ).project
        val id = (project["id"] as? JsonPrimitive)?.contentOrNull
            ?: throw TrpcException("projects.create returned no project id")
        // Tolerant full-row decode (ProjectEntity accepts the tRPC camelCase
        // names via @JsonNames): a server returning fewer fields degrades to
        // id-only instead of failing the already-committed create.
        val entity = runCatching {
            json.decodeFromJsonElement(ProjectEntity.serializer(), project)
        }.getOrNull()
        return CreatedProjectResult(id = id, entity = entity)
    }
}
