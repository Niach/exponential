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
    ///   - batchIssues: a batch run's covered issues (synced rows, any order).
    public static func resolve(
        session: CodingSessionEntity,
        issue: IssueEntity?,
        batchIssues: [IssueEntity] = []
    ) -> MergeTarget? {
        if session.issueId == nil, let carrier = batchCarrier(session: session, batchIssues: batchIssues) {
            return .issue(issueId: carrier.id)
        }
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

    /// EXP-1165: the covered issue that carries a BATCH run's combined PR
    /// (same url, still open). A batch merges through it, so the stack choice
    /// and the "Fix conflicts" recovery the issue path offers (Reviews) reach
    /// the run view too. Twin of web `lib/session-merge-target.ts`.
    public static func batchCarrier(
        session: CodingSessionEntity,
        batchIssues: [IssueEntity]
    ) -> IssueEntity? {
        guard let url = session.prUrl, !url.isEmpty else { return nil }
        // Covered issues only, in the run's naming order (`isBatch`-gated).
        return BatchRun.issues(session, issues: batchIssues).first {
            $0.prUrl == url && $0.prState == DomainContract.prStateOpen
        }
    }
}
