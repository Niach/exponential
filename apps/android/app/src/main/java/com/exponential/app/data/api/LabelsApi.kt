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
}
