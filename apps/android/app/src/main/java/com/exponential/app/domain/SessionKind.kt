package com.exponential.app.domain

// EXP-1341/NEW-2: ONE session model ×4. A thread and an agent's conversation
// run the same way; a conversation session (a root with `agentId`) is just
// `persistent` (never ends, revived on its device's return). Branch on these
// traits, never on the kind string: [SessionKind.sessionKindOf] derives the
// kind from `agentId`; the row's `kind` column is a legacy read shim (NEW-8
// drops it). Hand-ported from `packages/domain-contract/src/session-kind.ts`,
// locked to `fixtures/session-kind.json` by SessionKindTest.

data class SessionTraits(
    /** Never ends: exits idle, `sessions.end` refuses, revived by its device. */
    val persistent: Boolean,
    /** The kind whose definition carries triggers (the server fires them). */
    val triggers: Boolean,
)

object SessionKind {
    /** fixture `kinds`. */
    val KINDS: Map<String, SessionTraits> = mapOf(
        DomainContract.sessionKindThread to SessionTraits(persistent = false, triggers = false),
        DomainContract.sessionKindAgent to SessionTraits(persistent = true, triggers = true),
    )

    /** An unknown or missing kind = `thread`. */
    fun sessionTraits(kind: String?): SessionTraits =
        kind?.let { KINDS[it] } ?: KINDS.getValue(DomainContract.sessionKindThread)

    /** NEW-2: the kind of a session row: `agent` when it is an agent's
     *  conversation session (`agentId` set), else `thread`. */
    fun sessionKindOf(agentId: String?): String =
        if (!agentId.isNullOrEmpty()) DomainContract.sessionKindAgent else DomainContract.sessionKindThread

    /** A persistent session (an agent's conversation): it idles instead of ending. */
    fun isPersistent(agentId: String?): Boolean = sessionTraits(sessionKindOf(agentId)).persistent
}
