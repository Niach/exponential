package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity

// EXP-1184 (EXP-214/531/848 before it): how a LIVE coding session renders —
// ONE rule ×4, locked by `packages/domain-contract/fixtures/session-display.json`
// (web lib/coding-session-display.ts, desktop queries::coding_session_display,
// iOS CodingSessionDisplay.swift). First match wins:
// - NeedsInput: the run waits on a person — on EVERY live status, an open PR
//   included (the server clears the flag on every turn start and PR park, so
//   it is never stale; the old EXP-531 in_review mask is gone).
// - Working: the agent is mid-turn (the device-written `agent_busy`) — a
//   follow-up turn on an `in_review` run included.
// - Review: `in_review` with the PR neither merged nor closed.
// - Done: idle with no open PR, a closed PR, or merged (EXP-540: a lagging
//   server can still park a merged run on in_review — that reads Done).
// The session row's status never changes for this. A paused (offline) run and
// an ended row are the CALLER's to decide, before this rule.
enum class CodingSessionDisplayState { Working, NeedsInput, Review, Done }

/** The rule on raw inputs — the Work screen overlays its viewer's live
 *  signals (an open question in the feed) onto the synced flags. */
fun codingSessionDisplayState(
    status: String,
    needsInput: Boolean,
    agentBusy: Boolean,
    prState: String?,
): CodingSessionDisplayState {
    if (needsInput) return CodingSessionDisplayState.NeedsInput
    if (agentBusy) return CodingSessionDisplayState.Working
    val prOpen = prState != DomainContract.prStateMerged && prState != DomainContract.prStateClosed
    return if (status == DomainContract.codingSessionStatusInReview && prOpen) {
        CodingSessionDisplayState.Review
    } else {
        CodingSessionDisplayState.Done
    }
}

fun codingSessionDisplayState(
    session: CodingSessionEntity,
    prState: String?,
): CodingSessionDisplayState = codingSessionDisplayState(
    status = session.status,
    needsInput = session.needsInput,
    agentBusy = session.agentBusy,
    prState = prState,
)

/** EXP-848: whether a row animates — the agent is mid-turn on a row that is
 *  still live (an ended row never does, whatever the flag says). */
fun codingSessionIsWorking(status: String, state: CodingSessionDisplayState): Boolean =
    status != DomainContract.codingSessionStatusEnded && state == CodingSessionDisplayState.Working

/** The tone a live session row's status line paints in. */
enum class SessionStatusTone { Muted, Amber, Emerald, Sky }

fun sessionStatusTone(state: CodingSessionDisplayState): SessionStatusTone = when (state) {
    CodingSessionDisplayState.Working -> SessionStatusTone.Muted
    CodingSessionDisplayState.NeedsInput -> SessionStatusTone.Amber
    CodingSessionDisplayState.Review -> SessionStatusTone.Emerald
    CodingSessionDisplayState.Done -> SessionStatusTone.Sky
}
