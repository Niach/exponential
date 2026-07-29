package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity

// EXP-214: how a LIVE coding session renders. The synced status alone is not
// the whole story — `in_review` splits on the linked issue's PR outcome
// (merged → the run is done, review otherwise, matching the issue-status
// palette: review green, done blue), and the desktop-written `needs_input`
// attention flag (agent parked on a plan-approval / AskUserQuestion picker)
// overrides everything still actionable as an amber "Needs input". Callers
// pass only sessions that already passed CodingSessionLiveness.
//
// EXP-358: the server now parks merged sessions on their own `merged` status,
// so that is the FIRST thing checked. The legacy in_review + prState=merged
// split stays for rows written before the flip (and older servers).
enum class CodingSessionDisplayState { Running, NeedsInput, Review, Done, Merged }

fun codingSessionDisplayState(
    session: CodingSessionEntity,
    prState: String?,
): CodingSessionDisplayState {
    if (session.status == DomainContract.codingSessionStatusMerged) {
        return CodingSessionDisplayState.Merged
    }
    val merged = prState == DomainContract.prStateMerged
    if (session.needsInput && !merged) return CodingSessionDisplayState.NeedsInput
    if (session.status == DomainContract.codingSessionStatusInReview) {
        return if (merged) CodingSessionDisplayState.Done else CodingSessionDisplayState.Review
    }
    return CodingSessionDisplayState.Running
}
