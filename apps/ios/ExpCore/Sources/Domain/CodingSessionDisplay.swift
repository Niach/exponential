import Foundation

/// EXP-214: how a LIVE coding session renders. The synced status alone is not
/// the whole story — `in_review` splits on the linked issue's PR outcome
/// (merged → the run is done, review otherwise, matching the issue-status
/// palette: review green, done blue), and the desktop-written `needs_input`
/// attention flag (agent parked on a plan-approval / AskUserQuestion picker)
/// overrides everything still actionable as an amber "Needs input". Callers
/// pass only sessions that already passed CodingSessionLiveness.
public enum CodingSessionDisplayState {
    case running
    case needsInput
    case review
    case done

    public static func of(
        session: CodingSessionEntity,
        prState: String?
    ) -> CodingSessionDisplayState {
        let merged = prState == DomainContract.prStateMerged
        if session.needsInput && !merged { return .needsInput }
        if session.status == DomainContract.codingSessionStatusInReview {
            return merged ? .done : .review
        }
        return .running
    }
}
