import Foundation

/// EXP-734: WHAT a run's Merge affordance merges through. An issue-linked run
/// (and a batch run, through its PR's representative issue) merges the ISSUE's
/// PR; an action or chat run's PR links no issue at all, so it merges through
/// the SESSION row the server stamped it on.
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
    ///   - openBatchPrs: the team's open batch PRs, one representative issue
    ///     per distinct prUrl (`BatchPrResolution.openBatchPrs`).
    public static func resolve(
        session: CodingSessionEntity,
        issue: IssueEntity?,
        openBatchPrs: [IssueEntity]
    ) -> MergeTarget? {
        // An issue run merges its own issue's PR.
        if session.issueId != nil {
            if let issue, issue.prState == DomainContract.prStateOpen {
                return .issue(issueId: issue.id)
            }
            return nil
        }
        // A batch run carries no issue linkage — its PR resolves client-side
        // off the branch the pr_open flip stamped (EXP-535/545).
        if session.actionName == nil,
            session.status == DomainContract.codingSessionStatusInReview,
            let batchIssue = BatchPrResolution.resolve(
                sessionBranch: session.branch, openBatchPrs: openBatchPrs
            )
        {
            return .issue(issueId: batchIssue.id)
        }
        // Anything else merges through its OWN stamped PR — an action or chat
        // run's issue-less chore PR (EXP-734).
        if session.hasOpenPr {
            return .session(sessionId: session.id)
        }
        return nil
    }
}
