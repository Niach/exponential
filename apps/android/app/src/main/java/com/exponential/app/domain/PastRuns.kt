package com.exponential.app.domain

// EXP-746: the Devices screen's "Past" section — the caller's own FINISHED,
// person-started runs. Everything here is pure and mirrored ×4 (web
// `past-runs.ts`, iOS `PastRuns.swift`, desktop `devices_view.rs`); the row
// SELECTION lives in `AgentsViewModel.pastRunRows`, this file owns the copy.

/**
 * How a run's `ended_by` reads in a byline. The wire vocabulary is the
 * contract's `agent | user | client | merge | system`; anything else (an older
 * or newer publisher) drops the clause rather than printing a raw token.
 *
 * Byte-identical ×4 — the whole byline is asserted by
 * `the past byline names device, agent and who ended it`.
 */
fun endedByPhrase(endedBy: String?): String? = when (endedBy) {
    "agent" -> "agent"
    "user" -> "you"
    "client" -> "the app"
    "merge" -> "a merge"
    "system" -> "the system"
    else -> null
}

/**
 * One Past row's byline: `<device> · <agent label> · ended by <who> · <time>`.
 *
 * Every part is optional — an old row may carry no agent, a swept row no
 * `ended_by` — and a missing one simply drops its segment, so the separators
 * never double up. Byte-identical ×4.
 */
fun pastRunByline(
    deviceLabel: String?,
    agentLabel: String?,
    endedBy: String?,
    timeLabel: String,
): String = listOfNotNull(
    deviceLabel?.takeIf { it.isNotBlank() },
    agentLabel?.takeIf { it.isNotBlank() },
    endedByPhrase(endedBy)?.let { "ended by $it" },
    timeLabel.takeIf { it.isNotBlank() },
).joinToString(" · ")
