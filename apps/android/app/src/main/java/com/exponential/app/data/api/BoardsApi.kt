package com.exponential.app.data.api

import com.exponential.app.data.db.BoardEntity
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

// Mirrors apps/web/src/lib/trpc/boards.ts `create`. A repository is OPTIONAL on
// every board (EXP-121); when supplied it's either an existing team-registry
// repo (`repositoryId`) or a brand-new repo connected inline by `fullName` (the
// server validates the GitHub-App install and upserts the registry row in the
// same transaction). Modeled after iOS `BoardRepositoryChoice`.

/** The optional backing-repo choice for `boards.create`. */
sealed interface BoardRepositoryChoice {
    /** Target an existing registry repo (same-team, not archived). */
    data class Registry(val repositoryId: String) : BoardRepositoryChoice

    /**
     * Connect a new repo inline by `owner/name`; extra fields seed the registry
     * row. The installation id is resolved server-side — never sent by clients.
     */
    data class Inline(
        val fullName: String,
        val defaultBranch: String? = null,
        val isPrivate: Boolean? = null,
    ) : BoardRepositoryChoice

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
private data class CreateBoardResult(val board: JsonObject)

/**
 * `boards.create` result: the new board's id (always present) plus the
 * full row when the server response decodes into a [BoardEntity] (the server
 * returns every column via `.returning()`). The entity drives the optimistic
 * local Room upsert (EXP-46); it's null on older/self-hosted servers whose
 * response is missing required fields — callers then just wait for Electric.
 */
data class CreatedBoardResult(val id: String, val entity: BoardEntity?)

@Singleton
class BoardsApi @Inject constructor(
    private val trpc: TrpcClient,
    private val json: Json,
) {

    /**
     * Create a board. The server uppercases `prefix` and defaults `color` to
     * `#6366f1` when omitted. Since the board-type collapse (EXP-121) we send
     * `icon` (curated contract name) instead of the legacy `type`; a
     * `repository` is OPTIONAL on every board. The inline-connect path needs
     * owner/admin (repo management). Returns the new board id plus the full
     * row when decodable (see [CreatedBoardResult]).
     */
    suspend fun create(
        accountId: String,
        teamId: String,
        name: String,
        prefix: String,
        color: String?,
        icon: String,
        repository: BoardRepositoryChoice?,
    ): CreatedBoardResult {
        // Built as a raw JsonObject so the `repository` union encodes exactly as
        // the server's `z.union` expects (registry vs inline shapes differ).
        val input: JsonElement = buildJsonObject {
            put("teamId", teamId)
            put("name", name)
            put("prefix", prefix)
            color?.let { put("color", it) }
            put("icon", icon)
            repository?.let { put("repository", it.toJson()) }
        }
        val board = trpc.mutation(
            accountId,
            path = "boards.create",
            input = input,
            inputSerializer = JsonElement.serializer(),
            outputSerializer = CreateBoardResult.serializer(),
        ).board
        val id = (board["id"] as? JsonPrimitive)?.contentOrNull
            ?: throw TrpcException("boards.create returned no board id")
        // Tolerant full-row decode (BoardEntity accepts the tRPC camelCase
        // names via @JsonNames): a server returning fewer fields degrades to
        // id-only instead of failing the already-committed create.
        val entity = runCatching {
            json.decodeFromJsonElement(BoardEntity.serializer(), board)
        }.getOrNull()
        return CreatedBoardResult(id = id, entity = entity)
    }
}
