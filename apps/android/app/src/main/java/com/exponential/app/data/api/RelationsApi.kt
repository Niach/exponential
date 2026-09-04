package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable

/**
 * Input for `relations.create` (EXP-736). The row is always stored in the
 * CANONICAL direction — `inverse` flips the two ids server-side, so a
 * "Blocked by" pick sends the same `blocks` type as "Blocking" does.
 */
@Serializable
data class CreateRelationInput(
    val issueId: String,
    val relatedIssueId: String,
    val type: String,
    val inverse: Boolean = false,
)

@Serializable
data class DeleteRelationInput(val id: String)

// Mirrors apps/web/src/lib/trpc/relations.ts. Marking a DUPLICATE never comes
// through here: it stays the issues.update({duplicateOfId}) lockstep write
// (status + FK + mirror row in one transaction), which is why the picker's
// "Duplicate of" entry routes to IssuesApi.setDuplicateOf instead.
@Singleton
class RelationsApi @Inject constructor(private val trpc: TrpcClient) {

    suspend fun create(accountId: String, input: CreateRelationInput) {
        trpc.mutationUnit(
            accountId,
            path = "relations.create",
            input = input,
            inputSerializer = CreateRelationInput.serializer(),
        )
    }

    suspend fun delete(accountId: String, id: String) {
        trpc.mutationUnit(
            accountId,
            path = "relations.delete",
            input = DeleteRelationInput(id),
            inputSerializer = DeleteRelationInput.serializer(),
        )
    }
}
