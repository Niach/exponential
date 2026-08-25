package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity

// EXP-214: how a LIVE coding session renders. The synced status alone is not
// the whole story — `in_review` splits on the linked issue's PR outcome
// (merged → the run is done, review otherwise, matching the issue-status
// palette: review green, done blue), and the desktop-written `needs_input`
// attention flag (agent parked on a plan-approval / AskUserQuestion picker)
// paints a still-RUNNING session as an amber "Needs input". Callers pass only
// sessions that already passed CodingSessionLiveness.
//
// EXP-540: there is no `merged` session status any more (EXP-498 ends sessions
// on merge; the value is retired). The in_review + prState=merged split is what
// keeps OLD SERVERS readable — a lagging self-host can still park a row on
// in_review with a merged PR, and that reads as Done.
//
// EXP-531: `in_review` also wins over `needs_input` — once the PR is open the
// session IS in review, and claude's idle notification (which trips the flag
// ~60s after any turn ends, including the final one) must not mask "Ready for
// review" with attention noise. The server refuses needs_input=true on
// non-running rows for the same reason; this ordering also heals rows flagged
// by older desktops.
enum class CodingSessionDisplayState { Running, NeedsInput, Review, Done }

fun codingSessionDisplayState(
    session: CodingSessionEntity,
    prState: String?,
): CodingSessionDisplayState {
    val merged = prState == DomainContract.prStateMerged
    if (session.status == DomainContract.codingSessionStatusInReview) {
        return if (merged) CodingSessionDisplayState.Done else CodingSessionDisplayState.Review
    }
    if (session.needsInput && !merged) return CodingSessionDisplayState.NeedsInput
    return CodingSessionDisplayState.Running
}
