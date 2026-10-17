package com.exponential.app.data.api

import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

// EXP-1291 Threads v2: THE session router (apps/web/src/lib/trpc/sessions.ts)
// and the PR router beside it. ONE entrypoint starts and messages every
// session: `sessions.send` (a new thread root, a new team agent, a child, or
// a message to an existing session), landing in the ACTIVE team. The DB is
// the bus: the answer carries ids + a txid this client never awaits; the
// rows arrive through the sessions / messages shapes. Field names and
// optionality match `sessionsSendInputSchema` (db-schema/domain.ts) exactly;
// a null field is OMITTED (the shared Json has explicitNulls = false).

/**
 * An agent's facets on create (`agentFacetsSchema`): [icon] = contract
 * `boardIcon`, [color] = `#rrggbb`, [triggers] = `agentTriggerWireJson`
 * elements (an element without `id` is new).
 */
@Serializable
data class AgentFacetsInput(
    val name: String,
    val icon: String,
    val color: String,
    val instructions: String = "",
    val triggers: JsonArray = JsonArray(emptyList()),
)

/**
 * The `sessions.send` input. Build it through the four factories on the
 * companion ([root], [agent], [child], [message]) — the server refuses mixed
 * forms (`sessionId` + `parentId`, root facets on a message, …).
 */
@Serializable
data class SessionsSendInput(
    val teamId: String? = null,
    val sessionId: String? = null,
    val parentId: String? = null,
    // Runner pickers (new sessions; `account` alone on a message = a switch).
    val deviceId: String? = null,
    val cli: String? = null,
    val account: String? = null,
    val model: String? = null,
    val effort: String? = null,
    val planMode: Boolean? = null,
    val computerUse: Boolean? = null,
    val ultracode: Boolean? = null,
    val mcpServerIds: List<String>? = null,
    // The message.
    val text: String? = null,
    val attachmentIds: List<String>? = null,
    // Facets of a NEW root thread.
    val issueIds: List<String>? = null,
    val actionId: String? = null,
    val inputs: Map<String, String>? = null,
    val repositoryId: String? = null,
    // A NEW team agent root.
    val agent: AgentFacetsInput? = null,
    // EXP-1323: a `/btw` aside child (with [parentId]; text = the question).
    val aside: Boolean? = null,
) {
    companion object {
        /**
         * A NEW thread root in [teamId] (the ACTIVE team) on [deviceId]: text,
         * issues or an action (+ its [inputs]) — at least one of them.
         */
        fun root(
            teamId: String,
            deviceId: String,
            text: String? = null,
            attachmentIds: List<String>? = null,
            issueIds: List<String>? = null,
            actionId: String? = null,
            inputs: Map<String, String>? = null,
            repositoryId: String? = null,
            cli: String? = null,
            account: String? = null,
            model: String? = null,
            effort: String? = null,
            planMode: Boolean? = null,
            computerUse: Boolean? = null,
            ultracode: Boolean? = null,
            mcpServerIds: List<String>? = null,
        ): SessionsSendInput = SessionsSendInput(
            teamId = teamId,
            deviceId = deviceId,
            cli = cli,
            account = account,
            model = model,
            effort = effort,
            planMode = planMode,
            computerUse = computerUse,
            ultracode = ultracode,
            mcpServerIds = mcpServerIds?.takeIf { it.isNotEmpty() },
            text = text?.takeIf { it.isNotBlank() },
            attachmentIds = attachmentIds?.takeIf { it.isNotEmpty() },
            issueIds = issueIds?.takeIf { it.isNotEmpty() },
            actionId = actionId,
            inputs = inputs?.takeIf { actionId != null && it.isNotEmpty() },
            repositoryId = repositoryId,
        )

        /** A NEW team agent root (Settings → Agents → New agent). */
        fun agent(
            teamId: String,
            deviceId: String,
            agent: AgentFacetsInput,
            account: String? = null,
            cli: String? = null,
            model: String? = null,
        ): SessionsSendInput = SessionsSendInput(
            teamId = teamId,
            deviceId = deviceId,
            account = account,
            cli = cli,
            model = model,
            agent = agent,
        )

        /** A CHILD session under [parentId] (a follow-up / sub-session). */
        fun child(
            parentId: String,
            deviceId: String,
            text: String,
            attachmentIds: List<String>? = null,
        ): SessionsSendInput = SessionsSendInput(
            parentId = parentId,
            deviceId = deviceId,
            text = text,
            attachmentIds = attachmentIds?.takeIf { it.isNotEmpty() },
        )

        /** EXP-1323: `/btw <question>` — a read-only aside child of
         *  [parentId] (its device, no worktree); ends after its reply. */
        fun aside(parentId: String, text: String): SessionsSendInput =
            SessionsSendInput(parentId = parentId, text = text, aside = true)

        /**
         * A message to an existing session (resumes an idle one; an ENDED
         * thread continues server-side). Needs text or attachments.
         */
        fun message(
            sessionId: String,
            text: String,
            attachmentIds: List<String>? = null,
        ): SessionsSendInput = SessionsSendInput(
            sessionId = sessionId,
            text = text,
            attachmentIds = attachmentIds?.takeIf { it.isNotEmpty() },
        )

        /**
         * EXP-849: the mid-session account SWITCH — `{sessionId, account}`, no
         * text. The server moves the synced `sessions.account` and the host
         * device resumes the SAME row on that login between turns.
         */
        fun switchAccount(sessionId: String, account: String): SessionsSendInput =
            SessionsSendInput(sessionId = sessionId, account = account)

        /**
         * EXP-1252: MOVE the session to [deviceId] — `{sessionId, deviceId}`,
         * no text. The server sets the synced `sessions.move_to_device_id`
         * and the source host hands it off after its current turn; naming
         * the CURRENT device while a move is pending cancels it.
         */
        fun moveToDevice(sessionId: String, deviceId: String): SessionsSendInput =
            SessionsSendInput(sessionId = sessionId, deviceId = deviceId)
    }
}

/** `sessions.send`'s answer: the session it landed in and the message row
 *  (null for a start without text). */
@Serializable
data class SessionsSendResult(
    val sessionId: String,
    val messageId: String? = null,
    val txid: Long? = null,
)

@Serializable
private data class SessionIdInput(val sessionId: String)

@Serializable
private data class MessageIdInput(val messageId: String)

@Serializable
private data class LinkIssuesInput(val sessionId: String, val issueIds: List<String>)

@Serializable
private data class UnlinkIssueInput(val sessionId: String, val issueId: String)

/** EXP-1323 `sessions.startSuggestion`: [where] = `child` (a new child of
 *  the posting session, optional runner picks) | `here` (a message to it). */
@Serializable
data class StartSuggestionInput(
    val messageId: String,
    val itemId: String,
    val where: String,
    val deviceId: String? = null,
    val account: String? = null,
    val model: String? = null,
)

@Serializable
private data class SuggestionItemInput(val messageId: String, val itemId: String)

/** `sessions.answer`: [answer] for a `question` card, [decision]
 *  (`approved` | `rejected`) + optional [feedback] for a `plan` card.
 *  EXP-1280 (plan-card.json): [answers] = a plan's `{questionId: labels}`
 *  object or a question's labels array ([planAnswers] / [questionAnswers]
 *  build it); [surfaceData] = the inline surface's data model at answer time. */
@Serializable
data class AnswerInput(
    val messageId: String,
    val answer: String? = null,
    val decision: String? = null,
    /** An approval that starts in a fresh context (ask-dock.json
     *  `planOptions`); only with decision `approved`, else omitted. */
    val fresh: Boolean? = null,
    val feedback: String? = null,
    val answers: JsonElement? = null,
    val surfaceData: JsonObject? = null,
) {
    companion object {
        fun planAnswers(answers: Map<String, List<String>>): JsonObject =
            JsonObject(answers.mapValues { (_, labels) -> JsonArray(labels.map(::JsonPrimitive)) })

        fun questionAnswers(labels: List<String>): JsonArray = JsonArray(labels.map(::JsonPrimitive))
    }
}

/** EXP-1334 `sessions.configure`: a running session's launch picks changed
 *  live from THE session composer; a null field is omitted (unchanged). */
@Serializable
data class SessionConfigureInput(
    val sessionId: String,
    val planMode: Boolean? = null,
    val effort: String? = null,
    val ultracode: Boolean? = null,
    val computerUse: Boolean? = null,
    val mcpServerIds: List<String>? = null,
)

/** EXP-1334 `sessions.mcpPicks`: the session's server-only MCP picks. */
@Serializable
data class SessionMcpPicks(val mcpServerIds: List<String> = emptyList())

/** `sessions.getAgent`: the agent's server-only facets. */
@Serializable
data class AgentDetails(
    val sessionId: String,
    val instructions: String = "",
)

/**
 * `sessions.updateAgent` patch. An absent field keeps its value; [account]
 * and [model] are NULLABLE server-side — [clearAccount] / [clearModel] send
 * an explicit JSON null (the shared Json would otherwise drop it).
 */
data class UpdateAgentPatch(
    val name: String? = null,
    val icon: String? = null,
    val color: String? = null,
    val instructions: String? = null,
    /** `agentTriggerWireJson` elements; the WHOLE array is replaced. */
    val triggers: List<JsonObject>? = null,
    val deviceId: String? = null,
    val account: String? = null,
    val clearAccount: Boolean = false,
    val model: String? = null,
    val clearModel: Boolean = false,
    val sortOrder: Double? = null,
)

internal fun updateAgentInput(sessionId: String, patch: UpdateAgentPatch): JsonObject =
    buildJsonObject {
        put("sessionId", sessionId)
        patch.name?.let { put("name", it) }
        patch.icon?.let { put("icon", it) }
        patch.color?.let { put("color", it) }
        patch.instructions?.let { put("instructions", it) }
        patch.triggers?.let { put("triggers", JsonArray(it)) }
        patch.deviceId?.let { put("deviceId", it) }
        when {
            patch.account != null -> put("account", patch.account)
            patch.clearAccount -> put("account", JsonNull)
        }
        when {
            patch.model != null -> put("model", patch.model)
            patch.clearModel -> put("model", JsonNull)
        }
        patch.sortOrder?.let { put("sortOrder", it) }
    }

@Singleton
class SessionsApi @Inject constructor(private val trpc: TrpcClient) {

    /** THE entrypoint: start a root / agent / child, or message a session. */
    suspend fun send(accountId: String, input: SessionsSendInput): SessionsSendResult =
        trpc.mutation(
            accountId,
            path = "sessions.send",
            input = input,
            inputSerializer = SessionsSendInput.serializer(),
            outputSerializer = SessionsSendResult.serializer(),
        )

    /** Interrupt the current turn (the session stays live, goes idle). */
    suspend fun stop(accountId: String, sessionId: String) =
        unit(accountId, "sessions.stop", sessionId)

    /** End a session (agents refuse); ending a ROOT ends its whole tree. */
    suspend fun end(accountId: String, sessionId: String) =
        unit(accountId, "sessions.end", sessionId)

    /** Link issues to a thread (lands on the ROOT). 1–30 ids. */
    suspend fun linkIssues(accountId: String, sessionId: String, issueIds: List<String>) {
        trpc.mutationUnit(
            accountId,
            path = "sessions.linkIssues",
            input = LinkIssuesInput(sessionId, issueIds),
            inputSerializer = LinkIssuesInput.serializer(),
        )
    }

    suspend fun unlinkIssue(accountId: String, sessionId: String, issueId: String) {
        trpc.mutationUnit(
            accountId,
            path = "sessions.unlinkIssue",
            input = UnlinkIssueInput(sessionId, issueId),
            inputSerializer = UnlinkIssueInput.serializer(),
        )
    }

    /** Answer a `question` card or decide a `plan` card IN the card. */
    suspend fun answer(accountId: String, input: AnswerInput) {
        trpc.mutationUnit(
            accountId,
            path = "sessions.answer",
            input = input,
            inputSerializer = AnswerInput.serializer(),
        )
    }

    /** EXP-1334: the session's MCP picks (anyone who may steer it). */
    suspend fun mcpPicks(accountId: String, sessionId: String): SessionMcpPicks =
        trpc.query(
            accountId,
            path = "sessions.mcpPicks",
            input = SessionIdInput(sessionId),
            inputSerializer = SessionIdInput.serializer(),
            outputSerializer = SessionMcpPicks.serializer(),
        )

    /** EXP-1334: change a running session's Plan / Effort / Ultracode /
     *  Computer use / MCP servers. */
    suspend fun configure(accountId: String, input: SessionConfigureInput) {
        trpc.mutationUnit(
            accountId,
            path = "sessions.configure",
            input = input,
            inputSerializer = SessionConfigureInput.serializer(),
        )
    }

    /** Revoke one of your own messages the device has not consumed yet. */
    suspend fun unqueue(accountId: String, messageId: String) {
        trpc.mutationUnit(
            accountId,
            path = "sessions.unqueue",
            input = MessageIdInput(messageId),
            inputSerializer = MessageIdInput.serializer(),
        )
    }

    /** EXP-1323: stamp the viewer's read of a session (a thread = its root). */
    suspend fun markRead(accountId: String, sessionId: String) =
        unit(accountId, "sessions.markRead", sessionId)

    /** EXP-1323: Start / Do it here on one suggested task. */
    suspend fun startSuggestion(accountId: String, input: StartSuggestionInput) {
        trpc.mutationUnit(
            accountId,
            path = "sessions.startSuggestion",
            input = input,
            inputSerializer = StartSuggestionInput.serializer(),
        )
    }

    /** EXP-1323: Start all — every open item of a suggestions card. */
    suspend fun startAllSuggestions(accountId: String, messageId: String) {
        trpc.mutationUnit(
            accountId,
            path = "sessions.startAllSuggestions",
            input = MessageIdInput(messageId),
            inputSerializer = MessageIdInput.serializer(),
        )
    }

    /** EXP-1323: × / Dismiss one suggested task. */
    suspend fun dismissSuggestion(accountId: String, messageId: String, itemId: String) =
        suggestionItem(accountId, "sessions.dismissSuggestion", messageId, itemId)

    /** EXP-1323: File as issue — the item becomes an issue of the thread. */
    suspend fun fileSuggestion(accountId: String, messageId: String, itemId: String) =
        suggestionItem(accountId, "sessions.fileSuggestion", messageId, itemId)

    private suspend fun suggestionItem(accountId: String, path: String, messageId: String, itemId: String) {
        trpc.mutationUnit(
            accountId,
            path = path,
            input = SuggestionItemInput(messageId, itemId),
            inputSerializer = SuggestionItemInput.serializer(),
        )
    }

    /** The agent's server-only instructions (the settings page). */
    suspend fun getAgent(accountId: String, sessionId: String): AgentDetails =
        trpc.query(
            accountId,
            path = "sessions.getAgent",
            input = SessionIdInput(sessionId),
            inputSerializer = SessionIdInput.serializer(),
            outputSerializer = AgentDetails.serializer(),
        )

    /** Owner-only: patch an agent's facets / runner. */
    suspend fun updateAgent(accountId: String, sessionId: String, patch: UpdateAgentPatch) {
        trpc.mutationUnit(
            accountId,
            path = "sessions.updateAgent",
            input = updateAgentInput(sessionId, patch),
            inputSerializer = JsonObject.serializer(),
        )
    }

    /** Owner-only: delete an agent (its conversation goes with it). */
    suspend fun deleteAgent(accountId: String, sessionId: String) =
        unit(accountId, "sessions.deleteAgent", sessionId)

    private suspend fun unit(accountId: String, path: String, sessionId: String) {
        trpc.mutationUnit(
            accountId,
            path = path,
            input = SessionIdInput(sessionId),
            inputSerializer = SessionIdInput.serializer(),
        )
    }
}

// ── pullRequests (EXP-1291: the PR model, synced via pull_requests) ─────────

@Serializable
private data class PullRequestIdInput(val pullRequestId: String)

@Serializable
private data class MergePullRequestInput(val pullRequestId: String, val stack: Boolean? = null)

/** `pullRequests.merge`'s answer: [queued] = a merge queue took it; [error]
 *  = GitHub's refusal (render via `MergeFailure`). */
@Serializable
data class PullRequestMergeResult(
    val merged: Boolean = false,
    val queued: Boolean? = null,
    val error: String? = null,
    val txid: Long? = null,
)

@Serializable
data class PullRequestFile(
    val path: String,
    val additions: Int = 0,
    val deletions: Int = 0,
    val patch: String? = null,
)

@Serializable
data class PullRequestFilesResult(val files: List<PullRequestFile> = emptyList())

@Singleton
class PullRequestsApi @Inject constructor(private val trpc: TrpcClient) {

    /** Squash-merge; [stack] = merge THROUGH this member of a native stack. */
    suspend fun merge(accountId: String, pullRequestId: String, stack: Boolean? = null): PullRequestMergeResult =
        trpc.mutation(
            accountId,
            path = "pullRequests.merge",
            input = MergePullRequestInput(pullRequestId, stack),
            inputSerializer = MergePullRequestInput.serializer(),
            outputSerializer = PullRequestMergeResult.serializer(),
        )

    /** Close without merging. */
    suspend fun close(accountId: String, pullRequestId: String) {
        trpc.mutationUnit(
            accountId,
            path = "pullRequests.close",
            input = PullRequestIdInput(pullRequestId),
            inputSerializer = PullRequestIdInput.serializer(),
        )
    }

    /** The PR's changed files (the Guide's Changes rows). */
    suspend fun files(accountId: String, pullRequestId: String): List<PullRequestFile> =
        trpc.query(
            accountId,
            path = "pullRequests.files",
            input = PullRequestIdInput(pullRequestId),
            inputSerializer = PullRequestIdInput.serializer(),
            outputSerializer = PullRequestFilesResult.serializer(),
        ).files
}
