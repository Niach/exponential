package com.exponential.app.data.api

import com.exponential.app.data.db.ReleaseEntity
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put

// tRPC surface for the releases router (EXP-56). Mobile is view/manage only:
// create, ship/unship, delete, and issue membership — reads come entirely
// from the synced `releases` shape.

@Serializable
data class CreateReleaseInput(
    val workspaceId: String,
    // Both optional on the wire: the shared Json (explicitNulls=false) drops
    // null keys, so a plain create still sends the pre-EXP-62 shape. name
    // absent => the server auto-names ("Release N"); issueIds (EXP-62, max
    // 200 per call) attach in the SAME server transaction.
    val name: String? = null,
    val issueIds: List<String>? = null,
)

@Serializable
data class MarkReleaseShippedInput(val id: String, val shipped: Boolean)

@Serializable
data class DeleteReleaseInput(val id: String)

@Serializable
data class AddReleaseIssuesInput(val releaseId: String, val issueIds: List<String>)

@Singleton
class ReleasesApi @Inject constructor(private val trpc: TrpcClient, private val json: Json) {

    /**
     * Create a release WITH its creation-time issue bundle (EXP-62): the
     * server attaches [issueIds] in the same transaction as the insert. A
     * blank/absent name lets the server auto-name sequentially ("Release N").
     * The server caps issueIds at 200 per call — the first 200 ride create
     * itself, any overflow chunks through [addIssues]. Returns the full
     * created row (ReleaseEntity carries @JsonNames camelCase aliases, so the
     * tRPC payload decodes directly) so the caller can upsert it locally as a
     * visibility head-start instead of spinning until the Electric shape
     * delivers it (EXP-61).
     */
    suspend fun create(
        accountId: String,
        workspaceId: String,
        name: String? = null,
        issueIds: List<String> = emptyList(),
    ): ReleaseEntity {
        val first = issueIds.take(200)
        val result = trpc.mutation(
            accountId,
            path = "releases.create",
            input = CreateReleaseInput(
                workspaceId = workspaceId,
                name = name?.takeIf { it.isNotBlank() },
                issueIds = first.ifEmpty { null },
            ),
            inputSerializer = CreateReleaseInput.serializer(),
            // Decode as a raw JsonObject: the full release row rides along and
            // the shared strict Json must not choke on columns we don't model.
            outputSerializer = JsonObject.serializer(),
        )
        val release = json.decodeFromJsonElement(
            ReleaseEntity.serializer(),
            result["release"]!!.jsonObject,
        )
        val rest = issueIds.drop(200)
        if (rest.isNotEmpty()) addIssues(accountId, release.id, rest)
        return release
    }

    suspend fun markShipped(accountId: String, id: String, shipped: Boolean) {
        trpc.mutationUnit(
            accountId,
            path = "releases.markShipped",
            input = MarkReleaseShippedInput(id, shipped),
            inputSerializer = MarkReleaseShippedInput.serializer(),
        )
    }

    suspend fun delete(accountId: String, id: String) {
        trpc.mutationUnit(
            accountId,
            path = "releases.delete",
            input = DeleteReleaseInput(id),
            inputSerializer = DeleteReleaseInput.serializer(),
        )
    }

    /**
     * Bulk-add issues to a release (the detail's add-issues sheet). The server
     * caps issueIds at 200 per call — chunk sequentially so any selection size
     * lands (wire contract: clients chunk >200 ids).
     */
    suspend fun addIssues(accountId: String, releaseId: String, issueIds: List<String>) {
        for (chunk in issueIds.chunked(200)) {
            trpc.mutationUnit(
                accountId,
                path = "releases.addIssues",
                input = AddReleaseIssuesInput(releaseId, chunk),
                inputSerializer = AddReleaseIssuesInput.serializer(),
            )
        }
    }

    /**
     * Move an issue into a release, or out with `releaseId = null`.
     * `releaseId` is a REQUIRED nullable key server-side, so clearing must send
     * an explicit JSON null — the shared Json (explicitNulls=false) would
     * otherwise drop the key and fail Zod validation (setDuplicateOf pattern).
     */
    suspend fun setIssueRelease(accountId: String, issueId: String, releaseId: String?) {
        trpc.mutationUnit(
            accountId,
            path = "releases.setIssueRelease",
            input = buildJsonObject {
                put("issueId", issueId)
                if (releaseId != null) put("releaseId", releaseId) else put("releaseId", JsonNull)
            },
            inputSerializer = JsonObject.serializer(),
        )
    }
}
