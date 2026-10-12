package com.exponential.app.data.api

import com.exponential.app.data.db.ThreadEntity
import com.exponential.app.data.db.ThreadItemEntity
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

// Mirrors apps/web/src/lib/trpc/threads.ts (EXP-1284). Every write returns a
// `txid` the web awaits; this client never decodes it (ignoreUnknownKeys) and
// lets the threads / thread-items shapes land the change, like every other
// Api here. The rows tRPC returns are Drizzle camelCase, which the entities'
// @JsonNames accept.

@Serializable
data class ThreadIdInput(val id: String)

@Serializable
data class ThreadWithItems(
    val thread: ThreadEntity,
    val items: List<ThreadItemEntity> = emptyList(),
)

/** `threads.create`: a new thread in [teamId], optionally holding [issueIds]. */
@Serializable
data class CreateThreadInput(
    @SerialName("teamId") val teamId: String,
    val title: String? = null,
    @SerialName("issueIds") val issueIds: List<String>? = null,
)

@Serializable
data class ThreadResult(val thread: ThreadEntity)

/**
 * `threads.update`. The title is ALWAYS sent: the shared Json drops nulls, so
 * a nullable title could never clear it — `""` is the honest "back to the
 * derived title" write (the server clears on null or empty).
 */
@Serializable
data class UpdateThreadInput(
    val id: String,
    val title: String,
)

/**
 * `threads.link` / `threads.unlink`: exactly ONE of [issueId] / [sessionId].
 * Linking a run MOVES it (a run sits in exactly one thread); unlinking a run
 * moves it into a fresh thread of its own ([UnlinkThreadResult.newThreadId]).
 */
@Serializable
data class ThreadItemTargetInput(
    val id: String,
    @SerialName("issueId") val issueId: String? = null,
    @SerialName("sessionId") val sessionId: String? = null,
)

@Serializable
data class LinkThreadResult(val added: Boolean = false)

@Serializable
data class UnlinkThreadResult(
    val removed: Boolean = false,
    @SerialName("newThreadId") val newThreadId: String? = null,
)

@Singleton
class ThreadsApi @Inject constructor(private val trpc: TrpcClient) {

    /** The thread and its items (member read; items follow the shape's rules). */
    suspend fun get(accountId: String, id: String): ThreadWithItems =
        trpc.query(
            accountId,
            path = "threads.get",
            input = ThreadIdInput(id),
            inputSerializer = ThreadIdInput.serializer(),
            outputSerializer = ThreadWithItems.serializer(),
        )

    suspend fun create(accountId: String, input: CreateThreadInput): ThreadEntity =
        trpc.mutation(
            accountId,
            path = "threads.create",
            input = input,
            inputSerializer = CreateThreadInput.serializer(),
            outputSerializer = ThreadResult.serializer(),
        ).thread

    /** Renames the thread; a blank [title] clears it back to the derived one. */
    suspend fun update(accountId: String, id: String, title: String): ThreadEntity =
        trpc.mutation(
            accountId,
            path = "threads.update",
            input = UpdateThreadInput(id, title.trim()),
            inputSerializer = UpdateThreadInput.serializer(),
            outputSerializer = ThreadResult.serializer(),
        ).thread

    suspend fun linkIssue(accountId: String, id: String, issueId: String): Boolean =
        link(accountId, ThreadItemTargetInput(id, issueId = issueId))

    suspend fun linkRun(accountId: String, id: String, sessionId: String): Boolean =
        link(accountId, ThreadItemTargetInput(id, sessionId = sessionId))

    suspend fun unlinkIssue(accountId: String, id: String, issueId: String): UnlinkThreadResult =
        unlink(accountId, ThreadItemTargetInput(id, issueId = issueId))

    suspend fun unlinkRun(accountId: String, id: String, sessionId: String): UnlinkThreadResult =
        unlink(accountId, ThreadItemTargetInput(id, sessionId = sessionId))

    private suspend fun link(accountId: String, input: ThreadItemTargetInput): Boolean =
        trpc.mutation(
            accountId,
            path = "threads.link",
            input = input,
            inputSerializer = ThreadItemTargetInput.serializer(),
            outputSerializer = LinkThreadResult.serializer(),
        ).added

    private suspend fun unlink(accountId: String, input: ThreadItemTargetInput): UnlinkThreadResult =
        trpc.mutation(
            accountId,
            path = "threads.unlink",
            input = input,
            inputSerializer = ThreadItemTargetInput.serializer(),
            outputSerializer = UnlinkThreadResult.serializer(),
        )
}
