package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer

// Mirrors apps/web/src/lib/trpc/helpdesk.ts — the team-level Support inbox
// (EXP-180): standalone support tickets (conversations with external
// reporters, NOT issues) with reply / internal note / close / reopen /
// escalate-to-issue. The support tables are server-only (never Electric-synced),
// so natives read everything over tRPC and poll. Responses carry Drizzle's
// camelCase property names; the injected Json is ignoreUnknownKeys, so the
// DTOs model just what the UI renders.

/** `lastMessage` projection on a list row (latest PUBLIC message). */
@Serializable
data class SupportLastMessage(
    val body: String,
    // "inbound" (the reporter) | "outbound" (a member).
    val direction: String,
    val createdAt: String,
)

/**
 * One ticket. `helpdesk.listThreads` rows add [lastMessage] + [unread]
 * (unread = the reporter spoke last — no per-member read state);
 * `helpdesk.getThread` returns the bare thread row, so both default.
 */
@Serializable
data class SupportThreadRow(
    val id: String,
    val teamId: String,
    val title: String,
    // "open" | "resolved" (server-only vocabulary, not the domain contract).
    val status: String,
    val linkedIssueId: String? = null,
    val reporterEmail: String,
    val reporterName: String? = null,
    val lastReporterSeenAt: String? = null,
    val createdAt: String,
    val updatedAt: String,
    val lastMessage: SupportLastMessage? = null,
    val unread: Boolean = false,
)

@Serializable
data class SupportMessage(
    val id: String,
    val threadId: String,
    // NULL = the external reporter wrote it.
    val authorUserId: String? = null,
    val direction: String,
    // "public" | "internal" — internal notes never reach the reporter.
    val visibility: String,
    val body: String,
    val createdAt: String,
    val updatedAt: String,
)

/** Minimal linked-issue projection for the escalation chip. */
@Serializable
data class SupportLinkedIssue(
    val id: String,
    val identifier: String,
    val title: String,
    val status: String,
    val boardId: String,
)

@Serializable
data class SupportThreadDetail(
    val thread: SupportThreadRow,
    val messages: List<SupportMessage> = emptyList(),
    val linkedIssue: SupportLinkedIssue? = null,
)

@Serializable
data class EscalatedIssue(val id: String, val identifier: String, val title: String)

@Serializable
private data class ListThreadsInput(val teamId: String, val filter: String)

@Serializable
private data class ThreadIdInput(val threadId: String)

@Serializable
private data class ThreadBodyInput(val threadId: String, val body: String)

// `title` stays optional-absent (explicitNulls=false omits the null) — the
// server derives the issue title from the thread when unset.
@Serializable
private data class EscalateInput(
    val threadId: String,
    val boardId: String,
    val title: String? = null,
)

@Serializable
private data class EscalateResult(val issue: EscalatedIssue)

/** Server cap on reply/note bodies (helpdesk MAX_SUPPORT_MESSAGE_CHARS). */
const val MAX_SUPPORT_MESSAGE_CHARS = 10_000

@Singleton
class HelpdeskApi @Inject constructor(private val trpc: TrpcClient) {

    /** One row per ticket in the team; `filter` is "open" or "resolved". */
    suspend fun listThreads(
        accountId: String,
        teamId: String,
        filter: String,
    ): List<SupportThreadRow> =
        trpc.query(
            accountId,
            path = "helpdesk.listThreads",
            input = ListThreadsInput(teamId, filter),
            inputSerializer = ListThreadsInput.serializer(),
            outputSerializer = ListSerializer(SupportThreadRow.serializer()),
        )

    /** Full conversation including internal notes (member-only surface). */
    suspend fun getThread(accountId: String, threadId: String): SupportThreadDetail =
        trpc.query(
            accountId,
            path = "helpdesk.getThread",
            input = ThreadIdInput(threadId),
            inputSerializer = ThreadIdInput.serializer(),
            outputSerializer = SupportThreadDetail.serializer(),
        )

    /** Public reply — the server emails the reporter their magic link. */
    suspend fun reply(accountId: String, threadId: String, body: String) {
        trpc.mutationUnit(
            accountId,
            path = "helpdesk.reply",
            input = ThreadBodyInput(threadId, body),
            inputSerializer = ThreadBodyInput.serializer(),
        )
    }

    /** Internal note — member-only, never emailed. */
    suspend fun note(accountId: String, threadId: String, body: String) {
        trpc.mutationUnit(
            accountId,
            path = "helpdesk.note",
            input = ThreadBodyInput(threadId, body),
            inputSerializer = ThreadBodyInput.serializer(),
        )
    }

    suspend fun close(accountId: String, threadId: String) {
        trpc.mutationUnit(
            accountId,
            path = "helpdesk.close",
            input = ThreadIdInput(threadId),
            inputSerializer = ThreadIdInput.serializer(),
        )
    }

    suspend fun reopen(accountId: String, threadId: String) {
        trpc.mutationUnit(
            accountId,
            path = "helpdesk.reopen",
            input = ThreadIdInput(threadId),
            inputSerializer = ThreadIdInput.serializer(),
        )
    }

    /**
     * File an ordinary issue on a same-team board and link it to the ticket
     * (server-rejected when already linked — one escalation per ticket).
     */
    suspend fun escalate(accountId: String, threadId: String, boardId: String): EscalatedIssue =
        trpc.mutation(
            accountId,
            path = "helpdesk.escalate",
            input = EscalateInput(threadId, boardId),
            inputSerializer = EscalateInput.serializer(),
            outputSerializer = EscalateResult.serializer(),
        ).issue
}
