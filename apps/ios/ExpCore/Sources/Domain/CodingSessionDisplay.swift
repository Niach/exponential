import Foundation

/// EXP-214: how a LIVE coding session renders. The synced status alone is not
/// the whole story — `in_review` splits on the linked issue's PR outcome
/// (merged → the run is done, review otherwise, matching the issue-status
/// palette: review green, done blue), and the desktop-written `needs_input`
/// attention flag (agent parked on a plan-approval / AskUserQuestion picker)
/// overrides everything still actionable as an amber "Needs input". Callers
/// pass only sessions that already passed CodingSessionLiveness.
/// EXP-358: the server now says it outright — a PR merge flips the live session
/// to the `merged` status instead of ending it, so that status is the state,
/// checked before anything else (like the old PR-derived `done`, it also
/// outranks the needs-input flag). The `in_review` + merged-PR fallback stays
/// for rows written by older servers/desktops.
public enum CodingSessionDisplayState {
    case running
    case needsInput
    case review
    case done
    case merged

    public static func of(
        session: CodingSessionEntity,
        prState: String?
    ) -> CodingSessionDisplayState {
        if session.status == DomainContract.codingSessionStatusMerged { return .merged }
        let merged = prState == DomainContract.prStateMerged
        if session.needsInput && !merged { return .needsInput }
        if session.status == DomainContract.codingSessionStatusInReview {
            return merged ? .done : .review
        }
        return .running
    }
}
