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
    val description: String? = null,
    @SerialName("assigneeId") val assigneeId: String? = null,
    @SerialName("dueDate") val dueDate: String? = null,
    @SerialName("dueTime") val dueTime: String? = null,
    @SerialName("endTime") val endTime: String? = null,
    @SerialName("recurrenceInterval") val recurrenceInterval: Int? = null,
    @SerialName("recurrenceUnit") val recurrenceUnit: String? = null,
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
    // ISO-8601 timestamp string or null. The web tRPC coerces this back to
    // a Date — there is no superjson transformer in the pipe so we can't
    // ship a JS Date{} payload from Kotlin.
    @SerialName("archivedAt") val archivedAt: String? = null,
)

@Serializable
data class DeleteIssueInput(val id: String)

@Serializable
data class IssueResult(val issue: IssueEntity)

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

    suspend fun delete(accountId: String, id: String) {
        trpc.mutationUnit(
            accountId,
            path = "issues.delete",
            input = DeleteIssueInput(id),
            inputSerializer = DeleteIssueInput.serializer(),
        )
    }
}
