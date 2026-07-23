package com.exponential.app.data.api

import com.exponential.app.data.db.IssueEntity
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

@Serializable
data class CreateIssueInput(
    @SerialName("boardId") val boardId: String,
    val title: String,
    val status: String? = null,
    val priority: String? = null,
    val description: String? = null,
    @SerialName("assigneeId") val assigneeId: String? = null,
    @SerialName("dueDate") val dueDate: String? = null,
    @SerialName("dueTime") val dueTime: String? = null,
    @SerialName("endTime") val endTime: String? = null,
    // Team label ids assigned at create (issues.create inserts the
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
    // NOTE: a null here means "don't touch" — the shared Json omits it. Use
    // setAssignee()/bulkUpdate(clearAssignee=true) to actually UNASSIGN.
    @SerialName("assigneeId") val assigneeId: String? = null,
    @SerialName("dueDate") val dueDate: String? = null,
    @SerialName("dueTime") val dueTime: String? = null,
    @SerialName("endTime") val endTime: String? = null,
    // Canonical issue this one duplicates (pairs with status='duplicate').
    // NOTE: the shared Json omits nulls (explicitNulls=false), so clearing the
    // FK goes through setDuplicateOf() which sends an explicit JSON null.
    @SerialName("duplicateOfId") val duplicateOfId: String? = null,
)

@Serializable
data class DeleteIssueInput(val id: String)

@Serializable
data class ClosePrInput(@SerialName("issueId") val issueId: String)

/**
 * `issues.move` (EXP-57): same-team board move. The server renumbers
 * the issue in the target board (EXP-42 → ABC-17) and re-points the
 * denormalized children; the response's extra keys (txId, boardSlug) are
 * ignored by the shared Json.
 */
@Serializable
data class MoveIssueInput(
    val id: String,
    @SerialName("boardId") val boardId: String,
)

@Serializable
data class IssueResult(val issue: IssueEntity)

@Serializable
data class SearchIssuesInput(
    @SerialName("teamId") val teamId: String,
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
    @SerialName("boardId") val boardId: String,
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
     * Move an issue to another board in the SAME team (EXP-57). The
     * returned entity already carries the new boardId + identifier.
     */
    suspend fun move(accountId: String, issueId: String, boardId: String): IssueEntity =
        trpc.mutation(
            accountId,
            path = "issues.move",
            input = MoveIssueInput(id = issueId, boardId = boardId),
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
     * Close the issue's open PR WITHOUT merging (EXP-100 — the reject path
     * for an issue that got dropped after the work was done). Server-side via
     * the GitHub App; the `prState` flip arrives through Electric sync.
     */
    suspend fun closePr(accountId: String, issueId: String) {
        trpc.mutationUnit(
            accountId,
            path = "issues.closePr",
            input = ClosePrInput(issueId),
            inputSerializer = ClosePrInput.serializer(),
        )
    }

    /**
     * Squash-merge the issue's open PR via the GitHub App (EXP-131 Reviews).
     * For a batch PR (one prUrl linked to several issues) pass the
     * representative issue's id — the server resolves the PR to ALL linked
     * issues and completes them together; the `done` flip arrives via Electric.
     */
    suspend fun mergePr(accountId: String, issueId: String) {
        trpc.mutationUnit(
            accountId,
            path = "issues.mergePr",
            input = ClosePrInput(issueId),
            inputSerializer = ClosePrInput.serializer(),
        )
    }

    /**
     * Server-side full-text search over a team's issues — matches title,
     * description, AND comment text (things the local Room substring filter
     * can't see). Requires membership of [teamId]; results come back
     * relevance-ordered.
     */
    suspend fun search(
        accountId: String,
        teamId: String,
        query: String,
        limit: Int? = null,
    ): List<SearchIssueHit> =
        trpc.query(
            accountId,
            path = "issues.search",
            input = SearchIssuesInput(teamId = teamId, query = query, limit = limit),
            inputSerializer = SearchIssuesInput.serializer(),
            outputSerializer = ListSerializer(SearchIssueHit.serializer()),
        )

    /**
     * (Re)assign or UNASSIGN a single issue. Assignment can't go through the
     * plain [update] path: the shared Json omits nulls (explicitNulls=false),
     * so a null `assigneeId` never reaches the wire and the server reads the
     * missing key as "leave the assignee alone" — clearing an assignee was a
     * silent no-op. Same explicit-JSON-null escape hatch as [setDuplicateOf].
     */
    suspend fun setAssignee(accountId: String, issueId: String, assigneeId: String?): IssueEntity =
        trpc.mutation(
            accountId,
            path = "issues.update",
            input = buildJsonObject {
                put("id", issueId)
                if (assigneeId != null) put("assigneeId", assigneeId)
                else put("assigneeId", JsonNull)
            },
            inputSerializer = JsonObject.serializer(),
            outputSerializer = IssueResult.serializer(),
        ).issue

    /**
     * Bulk property write for the multi-select bar — the same
     * `issues.bulkUpdate` procedure web and desktop use. One server
     * transaction for the whole chunk, and (deliberately) NO per-issue
     * notification fan-out past 25 ids: the old client-side loop of
     * `issues.update` calls bypassed that cap, so a 60-issue sweep pushed 60
     * assignment notifications at one person. Callers chunk at the server's
     * 200-id input cap.
     *
     * The body is hand-built for the same reason [setAssignee] is: unassigning
     * the selection ([clearAssignee]) must reach the server as an explicit
     * JSON null, which the shared Json would otherwise drop.
     */
    suspend fun bulkUpdate(
        accountId: String,
        ids: List<String>,
        status: String? = null,
        priority: String? = null,
        assigneeId: String? = null,
        clearAssignee: Boolean = false,
    ) {
        trpc.mutationUnit(
            accountId,
            path = "issues.bulkUpdate",
            input = buildJsonObject {
                put("ids", JsonArray(ids.map { JsonPrimitive(it) }))
                if (status != null) put("status", status)
                if (priority != null) put("priority", priority)
                if (assigneeId != null) put("assigneeId", assigneeId)
                else if (clearAssignee) put("assigneeId", JsonNull)
            },
            inputSerializer = JsonObject.serializer(),
        )
    }

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
