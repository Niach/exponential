package com.exponential.app.domain

// #EXP-1456: where an inbox row (and its push) opens ×4. An issue row opens
// its issue; an issue-less row naming a session opens that session (an
// agent's question or plan, a mention, a wall); #EXP-1534: an
// `agent_signed_out` row opens its device's settings; [InboxRoute.sessionTarget] is
// the S-3 inter-session rule. Hand-ported from
// `packages/domain-contract/src/inbox-route.ts`, locked to
// `fixtures/inbox-route.json` by InboxRouteTest.

object InboxRoute {
    sealed interface RowTarget {
        /** [results] = open the issue's Results face (an agent's message about it, EXP-933). */
        data class Issue(val issueId: String, val results: Boolean) : RowTarget
        data class Session(val sessionId: String) : RowTarget

        /** #EXP-1534: the Devices page with this device's (devices row id) settings open. */
        data class Device(val deviceId: String) : RowTarget
        data object None : RowTarget
    }

    /** The issue-less types an inbox lists. */
    private val SESSION_ROW_TYPES = setOf(
        DomainContract.notificationTypeAgentMessage,
        DomainContract.notificationTypeSessionBlocked,
    )

    fun inboxRowTarget(
        type: String?,
        issueId: String?,
        sessionId: String?,
        deviceId: String? = null,
    ): RowTarget = when {
        !issueId.isNullOrEmpty() ->
            RowTarget.Issue(issueId, results = type == DomainContract.notificationTypeAgentMessage)
        type in SESSION_ROW_TYPES && !sessionId.isNullOrEmpty() -> RowTarget.Session(sessionId)
        type == DomainContract.notificationTypeAgentSignedOut && !deviceId.isNullOrEmpty() ->
            RowTarget.Device(deviceId)
        else -> RowTarget.None
    }

    sealed interface SessionTarget {
        /** The viewer's own agent conversation root: the agent's page (its channel). */
        data class Agent(val agentId: String) : SessionTarget

        /** A chat root ([sessionId] null) or a chat's child inside it. */
        data class Chat(val chatId: String, val sessionId: String?) : SessionTarget

        /** Anything else under an agent: the session on its own screen. */
        data class Session(val sessionId: String) : SessionTarget
    }

    /**
     * Where a session opens (S-3). [agentId] and [userId] are the ROOT's (a
     * child carries no agent: it resolves through its root); [rootId]
     * null/empty = the row is its own root.
     */
    fun sessionTarget(
        id: String,
        rootId: String?,
        agentId: String?,
        userId: String?,
        viewerId: String?,
    ): SessionTarget {
        val root = rootId?.takeIf { it.isNotEmpty() } ?: id
        if (!agentId.isNullOrEmpty()) {
            return if (root == id && userId == viewerId) SessionTarget.Agent(agentId) else SessionTarget.Session(id)
        }
        return SessionTarget.Chat(root, if (root == id) null else id)
    }

    /** waiting-cards.json DEEP LINKS: where a jump INTO a session's message lands. */
    sealed interface Landing {
        /** THE plan page of the chat [rootId], on the plan [messageId]. */
        data class Plan(val rootId: String, val messageId: String) : Landing

        /** The chat [rootId]'s Conversation, the pending deep link on [messageId]. */
        data class Card(val rootId: String, val messageId: String) : Landing

        /** No card to reach: the session by [sessionTarget]. */
        data object Session : Landing
    }

    /**
     * An inbox row's or a push's jump into [messageId] (the push's
     * `messageId`, else the session's waiting card): a plan opens THE plan
     * page, a question its chat at the card (the chat lands a descendant's
     * card in that session, `waitingTarget`). [rootId] = the session's root
     * (synced, else the push's `rootId`); none = the session's own route. A
     * session opening on its own screen ([SessionTarget.Session]) keeps it.
     */
    fun landing(target: SessionTarget?, rootId: String?, messageId: String?, plan: Boolean): Landing {
        val root = rootId?.takeIf { it.isNotEmpty() } ?: return Landing.Session
        if (messageId.isNullOrEmpty()) return Landing.Session
        if (plan) return Landing.Plan(root, messageId)
        if (target is SessionTarget.Session) return Landing.Session
        return Landing.Card(root, messageId)
    }
}
