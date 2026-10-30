package com.exponential.app.domain

// #EXP-1456: where an inbox row (and its push) opens ×4. An issue row opens
// its issue; an issue-less row naming a session opens that session (an
// agent's question or plan, a mention, a wall); [InboxRoute.sessionTarget] is
// the S-3 inter-session rule. Hand-ported from
// `packages/domain-contract/src/inbox-route.ts`, locked to
// `fixtures/inbox-route.json` by InboxRouteTest.

object InboxRoute {
    sealed interface RowTarget {
        /** [results] = open the issue's Results face (an agent's message about it, EXP-933). */
        data class Issue(val issueId: String, val results: Boolean) : RowTarget
        data class Session(val sessionId: String) : RowTarget
        data object None : RowTarget
    }

    /** The issue-less types an inbox lists. */
    private val SESSION_ROW_TYPES = setOf(
        DomainContract.notificationTypeAgentMessage,
        DomainContract.notificationTypeSessionBlocked,
    )

    fun inboxRowTarget(type: String?, issueId: String?, sessionId: String?): RowTarget = when {
        !issueId.isNullOrEmpty() ->
            RowTarget.Issue(issueId, results = type == DomainContract.notificationTypeAgentMessage)
        type in SESSION_ROW_TYPES && !sessionId.isNullOrEmpty() -> RowTarget.Session(sessionId)
        else -> RowTarget.None
    }

    sealed interface SessionTarget {
        /** The viewer's own agent conversation root: the agent's page (its channel). */
        data class Agent(val agentId: String) : SessionTarget

        /** A thread root ([sessionId] null) or a thread's child inside it. */
        data class Thread(val threadId: String, val sessionId: String?) : SessionTarget

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
        return SessionTarget.Thread(root, if (root == id) null else id)
    }
}
