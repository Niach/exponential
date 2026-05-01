package com.exponential.app.data.api

import com.exponential.app.data.db.IssueEntity
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class CreateIssueInput(
    @SerialName("projectId") val projectId: String,
    val title: String,
    val status: String? = null,
    val priority: String? = null,
    val description: IssueDescription? = null,
    @SerialName("dueDate") val dueDate: String? = null,
)

@Serializable
data class UpdateIssueInput(
    val id: String,
    val title: String? = null,
    val status: String? = null,
    val priority: String? = null,
    val description: IssueDescription? = null,
    @SerialName("dueDate") val dueDate: String? = null,
)

@Serializable
data class DeleteIssueInput(val id: String)

@Serializable
data class IssueDescription(val text: String)

@Serializable
data class IssueResult(val issue: IssueEntity)

@Singleton
class IssuesApi @Inject constructor(private val trpc: TrpcClient) {

    suspend fun create(input: CreateIssueInput): IssueEntity =
        trpc.mutation(
            path = "issues.create",
            input = input,
            inputSerializer = CreateIssueInput.serializer(),
            outputSerializer = IssueResult.serializer(),
        ).issue

    suspend fun update(input: UpdateIssueInput): IssueEntity =
        trpc.mutation(
            path = "issues.update",
            input = input,
            inputSerializer = UpdateIssueInput.serializer(),
            outputSerializer = IssueResult.serializer(),
        ).issue

    suspend fun delete(id: String) {
        trpc.mutation(
            path = "issues.delete",
            input = DeleteIssueInput(id),
            inputSerializer = DeleteIssueInput.serializer(),
            outputSerializer = kotlinx.serialization.json.JsonElement.serializer(),
        )
    }
}
