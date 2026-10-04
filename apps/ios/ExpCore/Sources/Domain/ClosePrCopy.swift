import Foundation

/// EXP-1154: the Close PR copy x4 (fixture
/// `packages/domain-contract/fixtures/close-pr.json`, read by
/// `ClosePrCopyTests`). The item lives in the issue's `…` menu (contract
/// `diffUi.closePr`, the `pr-closed` glyph, destructive, members only while
/// the PR is open) and asks this confirm first.
public enum ClosePrCopy {
    public static let menuItem = DomainContract.diffUiClosePr
    public static let title = "Close pull request?"
    public static let body = "Closes the pull request on GitHub without merging. Use this when the issue was dropped even though the work exists. The branch is kept and the PR can be reopened on GitHub."
    public static let batchLineTemplate = "It also closes the pull request for {n} linked issues."
    public static let confirm = "Close PR"

    /// The confirm's message: the body, plus the batch line when the PR
    /// links `otherLinkedIssues` more issues.
    public static func message(otherLinkedIssues: Int) -> String {
        guard otherLinkedIssues > 0 else { return body }
        let line = batchLineTemplate.replacingOccurrences(of: "{n}", with: String(otherLinkedIssues))
        return "\(body) \(line)"
    }
}
