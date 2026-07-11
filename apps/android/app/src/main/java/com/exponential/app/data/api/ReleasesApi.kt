package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

// tRPC surface for the releases router (EXP-56). Mobile is view/manage only:
// create, ship/unship, delete, and issue membership — reads come entirely
// from the synced `releases` shape.

@Serializable
data class CreateReleaseInput(
    val workspaceId: String,
    val name: String,
    // Optional; nulls are omitted by the shared Json (explicitNulls=false),
    // matching the server's `.optional()` zod fields.
    val description: String? = null,
    val targetDate: String? = null,
)

@Serializable
data class MarkReleaseShippedInput(val id: String, val shipped: Boolean)

@Serializable
data class DeleteReleaseInput(val id: String)

@Singleton
class ReleasesApi @Inject constructor(private val trpc: TrpcClient) {

    suspend fun create(accountId: String, input: CreateReleaseInput) {
        trpc.mutationUnit(
            accountId,
            path = "releases.create",
            input = input,
            inputSerializer = CreateReleaseInput.serializer(),
        )
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
