import Foundation

/// EXP-734: WHAT a run's Merge affordance merges through. An issue run merges
/// the ISSUE's PR; every issue-less run (batch, action, chat) owns the PR it
/// opened on its OWN row (`coding_sessions.pr_*`), so it merges through the
/// SESSION.
public enum MergeTarget: Equatable, Sendable {
    case issue(issueId: String)
    case session(sessionId: String)
}

/// The one rule every iOS merge surface applies (Agents rows, the steering
/// screen, Reviews) — mirrors web's `use-agents-data.ts` and the desktop.
public enum MergeTargetResolution {
    /// - Parameters:
    ///   - session: the run.
    ///   - issue: the run's own issue, when it has one (already observed).
    public static func resolve(
        session: CodingSessionEntity,
        issue: IssueEntity?
    ) -> MergeTarget? {
        // An issue run merges its own issue's PR.
        if session.issueId != nil {
            if let issue, issue.prState == DomainContract.prStateOpen {
                return .issue(issueId: issue.id)
            }
            return nil
        }
        // An issue-less run merges through its OWN stamped PR.
        if session.hasOpenPr {
            return .session(sessionId: session.id)
        }
        return nil
    }
}
