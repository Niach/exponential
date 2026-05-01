package com.exponential.app.data.api

import com.exponential.app.data.db.LabelEntity
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable

@Serializable
data class CreateLabelInput(
    val workspaceId: String,
    val name: String,
    val color: String = "#6366f1",
)

@Serializable
data class CreateLabelResult(val label: LabelEntity)

@Serializable
data class IssueLabelInput(val issueId: String, val labelId: String)

@Serializable
private object EmptyResult

@Singleton
class LabelsApi @Inject constructor(private val trpc: TrpcClient) {
    suspend fun create(input: CreateLabelInput): LabelEntity =
        trpc.mutation(
            path = "labels.create",
            input = input,
            inputSerializer = CreateLabelInput.serializer(),
            outputSerializer = CreateLabelResult.serializer(),
        ).label

    suspend fun addLabel(issueId: String, labelId: String) {
        trpc.mutation(
            path = "issueLabels.add",
            input = IssueLabelInput(issueId, labelId),
            inputSerializer = IssueLabelInput.serializer(),
            outputSerializer = kotlinx.serialization.json.JsonElement.serializer(),
        )
    }

    suspend fun removeLabel(issueId: String, labelId: String) {
        trpc.mutation(
            path = "issueLabels.remove",
            input = IssueLabelInput(issueId, labelId),
            inputSerializer = IssueLabelInput.serializer(),
            outputSerializer = kotlinx.serialization.json.JsonElement.serializer(),
        )
    }
}
