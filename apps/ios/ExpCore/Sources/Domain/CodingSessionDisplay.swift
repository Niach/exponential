import Foundation

/// EXP-214: how a LIVE coding session renders. The synced status alone is not
/// the whole story — `in_review` splits on the linked issue's PR outcome
/// (merged → the run is done, review otherwise, matching the issue-status
/// palette: review green, done blue), and the desktop-written `needs_input`
/// attention flag (agent parked on a plan-approval / AskUserQuestion picker)
/// marks a still-RUNNING session as an amber "Needs input". Callers
/// pass only sessions that already passed CodingSessionLiveness.
/// EXP-540: a PR merge ENDS the session (EXP-498), so a merged run leaves the
/// live set instead of parking in a status of its own. The `in_review` +
/// merged-PR arm stays as old-server tolerance: a lagging self-host server can
/// still leave a row in `in_review` after its PR merged.
/// EXP-531: `in_review` also outranks the needs-input flag — once the PR is
/// open the run is done coding, and claude's idle-nudge notification (which
/// the desktop forwards as needs_input) must not mask "Ready for review".
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
        if session.status == DomainContract.codingSessionStatusInReview {
            return merged ? .done : .review
        }
        if session.needsInput && !merged { return .needsInput }
        return .running
    }
}
