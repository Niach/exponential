package com.exponential.app.domain

// EXP-1341: ONE session model ×4. A thread and an agent run the same way; an
// agent is just `persistent` (never ends, revived on its device's return) and
// carries `triggers`. Branch on these traits, never on the kind string.
// Hand-ported from `packages/domain-contract/src/session-kind.ts`, locked to
// `fixtures/session-kind.json` by SessionKindTest.

data class SessionTraits(
    /** Never ends: exits idle, `sessions.end` refuses, revived by its device. */
    val persistent: Boolean,
    /** Carries triggers (schedules / events) its device fires as messages. */
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

    /** A persistent session (an agent root): it idles instead of ending. */
    fun isPersistent(kind: String?): Boolean = sessionTraits(kind).persistent
}
