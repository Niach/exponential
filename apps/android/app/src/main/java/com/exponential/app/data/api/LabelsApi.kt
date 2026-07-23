package com.exponential.app.data.api

import com.exponential.app.data.db.LabelEntity
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable

@Serializable
data class CreateLabelInput(
    val teamId: String,
    val name: String,
    val color: String = "#6366f1",
)

@Serializable
data class CreateLabelResult(val label: LabelEntity)

@Serializable
data class IssueLabelInput(val issueId: String, val labelId: String)

/**
 * Input for `issueLabels.bulkAdd` / `issueLabels.bulkRemove` — one label
 * across many issues in ONE transaction. Server caps `issueIds` at 200.
 */
@Serializable
data class BulkIssueLabelInput(val labelId: String, val issueIds: List<String>)

@Serializable
data class UpdateLabelInput(
    val teamId: String,
    val labelId: String,
    val name: String? = null,
    val color: String? = null,
)

@Serializable
data class DeleteLabelInput(val teamId: String, val labelId: String)

@Singleton
class LabelsApi @Inject constructor(private val trpc: TrpcClient) {
    suspend fun create(accountId: String, input: CreateLabelInput): LabelEntity =
        trpc.mutation(
            accountId,
            path = "labels.create",
            input = input,
            inputSerializer = CreateLabelInput.serializer(),
            outputSerializer = CreateLabelResult.serializer(),
        ).label

    suspend fun update(accountId: String, input: UpdateLabelInput) {
        trpc.mutationUnit(
            accountId,
            path = "labels.update",
            input = input,
            inputSerializer = UpdateLabelInput.serializer(),
        )
    }

    suspend fun delete(accountId: String, teamId: String, labelId: String) {
        trpc.mutationUnit(
            accountId,
            path = "labels.delete",
            input = DeleteLabelInput(teamId, labelId),
            inputSerializer = DeleteLabelInput.serializer(),
        )
    }

    suspend fun addLabel(accountId: String, issueId: String, labelId: String) {
        trpc.mutationUnit(
            accountId,
            path = "issueLabels.add",
            input = IssueLabelInput(issueId, labelId),
            inputSerializer = IssueLabelInput.serializer(),
        )
    }

    suspend fun removeLabel(accountId: String, issueId: String, labelId: String) {
        trpc.mutationUnit(
            accountId,
            path = "issueLabels.remove",
            input = IssueLabelInput(issueId, labelId),
            inputSerializer = IssueLabelInput.serializer(),
        )
    }

    /**
     * Add one label to many issues in a single transaction (selection bar).
     * Unlike the per-issue [addLabel], the server only records a `label_added`
     * timeline event for rows it actually inserted, so re-adding a label an
     * issue already carries writes no spurious event.
     */
    suspend fun bulkAddLabel(accountId: String, issueIds: List<String>, labelId: String) {
        trpc.mutationUnit(
            accountId,
            path = "issueLabels.bulkAdd",
            input = BulkIssueLabelInput(labelId, issueIds),
            inputSerializer = BulkIssueLabelInput.serializer(),
        )
    }

    /** Remove one label from many issues in a single transaction. */
    suspend fun bulkRemoveLabel(accountId: String, issueIds: List<String>, labelId: String) {
        trpc.mutationUnit(
            accountId,
            path = "issueLabels.bulkRemove",
            input = BulkIssueLabelInput(labelId, issueIds),
            inputSerializer = BulkIssueLabelInput.serializer(),
        )
    }
}
