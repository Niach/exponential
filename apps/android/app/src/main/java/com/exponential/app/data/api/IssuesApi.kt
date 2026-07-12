package com.exponential.app.data.api

import com.exponential.app.data.db.IssueEntity
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

@Serializable
data class CreateIssueInput(
    @SerialName("projectId") val projectId: String,
    val title: String,
    val status: String? = null,
    val priority: String? = null,
    val description: String? = null,
    @SerialName("assigneeId") val assigneeId: String? = null,
    @SerialName("dueDate") val dueDate: String? = null,
    @SerialName("dueTime") val dueTime: String? = null,
    @SerialName("endTime") val endTime: String? = null,
    @SerialName("recurrenceInterval") val recurrenceInterval: Int? = null,
    @SerialName("recurrenceUnit") val recurrenceUnit: String? = null,
    // Workspace label ids assigned at create (issues.create inserts the
    // issue_labels joins in the same transaction). Null = none.
    @SerialName("labelIds") val labelIds: List<String>? = null,
)

@Serializable
data class UpdateIssueInput(
    val id: String,
    val title: String? = null,
    val status: String? = null,
    val priority: String? = null,
    val description: String? = null,
    @SerialName("assigneeId") val assigneeId: String? = null,
    @SerialName("dueDate") val dueDate: String? = null,
    @SerialName("dueTime") val dueTime: String? = null,
    @SerialName("endTime") val endTime: String? = null,
    @SerialName("recurrenceInterval") val recurrenceInterval: Int? = null,
    @SerialName("recurrenceUnit") val recurrenceUnit: String? = null,
    // Canonical issue this one duplicates (pairs with status='duplicate').
    // NOTE: the shared Json omits nulls (explicitNulls=false), so clearing the
    // FK goes through setDuplicateOf() which sends an explicit JSON null.
    @SerialName("duplicateOfId") val duplicateOfId: String? = null,
)

@Serializable
data class DeleteIssueInput(val id: String)

/**
 * `issues.move` (EXP-57): same-workspace project move. The server renumbers
 * the issue in the target project (EXP-42 → ABC-17) and re-points the
 * denormalized children; the response's extra keys (txId, projectSlug) are
 * ignored by the shared Json.
 */
@Serializable
data class MoveIssueInput(
    val id: String,
    @SerialName("projectId") val projectId: String,
)

@Serializable
data class IssueResult(val issue: IssueEntity)

@Serializable
data class SearchIssuesInput(
    @SerialName("workspaceId") val workspaceId: String,
    val query: String,
    // Server default 20, max 50. Null omits the field (shared Json has
    // explicitNulls=false) so the server default applies.
    val limit: Int? = null,
)

/** One relevance-ordered hit from the server-side full-text `issues.search`. */
@Serializable
data class SearchIssueHit(
    val id: String,
    val identifier: String,
    val title: String,
    @SerialName("projectId") val projectId: String,
    val status: String,
    val priority: String,
)

@Singleton
class IssuesApi @Inject constructor(private val trpc: TrpcClient) {

    suspend fun create(accountId: String, input: CreateIssueInput): IssueEntity =
        trpc.mutation(
            accountId,
            path = "issues.create",
            input = input,
            inputSerializer = CreateIssueInput.serializer(),
            outputSerializer = IssueResult.serializer(),
        ).issue

    suspend fun update(accountId: String, input: UpdateIssueInput): IssueEntity =
        trpc.mutation(
            accountId,
            path = "issues.update",
            input = input,
            inputSerializer = UpdateIssueInput.serializer(),
            outputSerializer = IssueResult.serializer(),
        ).issue

    /**
     * Move an issue to another project in the SAME workspace (EXP-57). The
     * returned entity already carries the new projectId + identifier.
     */
    suspend fun move(accountId: String, issueId: String, projectId: String): IssueEntity =
        trpc.mutation(
            accountId,
            path = "issues.move",
            input = MoveIssueInput(id = issueId, projectId = projectId),
            inputSerializer = MoveIssueInput.serializer(),
            outputSerializer = IssueResult.serializer(),
        ).issue

    suspend fun delete(accountId: String, id: String) {
        trpc.mutationUnit(
            accountId,
            path = "issues.delete",
            input = DeleteIssueInput(id),
            inputSerializer = DeleteIssueInput.serializer(),
        )
    }

    /**
     * Server-side full-text search over a workspace's issues — matches title,
     * description, AND comment text (things the local Room substring filter
     * can't see). Requires membership of [workspaceId]; results come back
     * relevance-ordered.
     */
    suspend fun search(
        accountId: String,
        workspaceId: String,
        query: String,
        limit: Int? = null,
    ): List<SearchIssueHit> =
        trpc.query(
            accountId,
            path = "issues.search",
            input = SearchIssuesInput(workspaceId = workspaceId, query = query, limit = limit),
            inputSerializer = SearchIssuesInput.serializer(),
            outputSerializer = ListSerializer(SearchIssueHit.serializer()),
        )

    /**
     * Mark/unmark an issue as a duplicate of a canonical issue — one atomic
     * `issues.update` mutation (masterplan §5e): marking sets `duplicateOfId`
     * AND flips status to the terminal `duplicate` value; unmarking clears the
     * FK (explicit JSON null — the shared Json would otherwise omit it) and
     * restores status to [restoreStatus].
     */
    suspend fun setDuplicateOf(
        accountId: String,
        issueId: String,
        duplicateOfId: String?,
        restoreStatus: String = "backlog",
    ): IssueEntity =
        trpc.mutation(
            accountId,
            path = "issues.update",
            input = buildJsonObject {
                put("id", issueId)
                if (duplicateOfId != null) {
                    put("duplicateOfId", duplicateOfId)
                    put("status", "duplicate")
                } else {
                    put("duplicateOfId", JsonNull)
                    put("status", restoreStatus)
                }
            },
            inputSerializer = JsonObject.serializer(),
            outputSerializer = IssueResult.serializer(),
        ).issue
}
