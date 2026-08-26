import Foundation

// EXP-535: batch sessions carry no issue linkage, so a batch row resolves its
// open PR client-side: the team's open-PR issues on an `exp/batch-` branch,
// collapsed by prUrl keeping the NEWEST issue as the representative (Reviews
// pattern — merging through it merges the ONE batch PR, the server resolves
// every linked issue by exact pr_url). EXP-545: the Merge shortcut must
// target the session's OWN PR — the head branch the server's pr_open batch
// flip stamped on the row. Matching "the team's sole open batch PR" alone
// could offer a teammate's PR once this session's own PR closed unmerged
// (prState `closed` while the row stays in_review — only merge ends it).
// EXP-546: the branch is now the ONLY key — rows flipped before the EXP-545
// stamp existed have drained, so a branchless in_review batch row simply shows
// no Merge shortcut rather than guessing at the sole open PR. Anything
// unmatched or ambiguous resolves to nil — Reviews still lists every PR.
// Mirrors web's `use-agents-data.ts` and Android's `AgentsViewModel.kt`.

public enum BatchPrResolution {
    /// The team's open batch PRs, one representative (newest linked) issue
    /// per distinct prUrl.
    ///
    /// Issues don't sync `team_id`, so callers pass the team's synced board
    /// ids as the scope (trashed/archived boards never sync, so presence in
    /// the set already implies visibility — same as StartPullRequestOption).
    public static func openBatchPrs(
        issues: [IssueEntity],
        teamBoardIds: Set<String>
    ) -> [IssueEntity] {
        var byPrUrl: [String: IssueEntity] = [:]
        // Deterministic representative on created-at ties regardless of the
        // fetch order GRDB happened to return.
        for issue in issues.sorted(by: { $0.id < $1.id }) {
            guard teamBoardIds.contains(issue.boardId),
                issue.prState == DomainContract.prStateOpen,
                let prUrl = issue.prUrl, !prUrl.isEmpty,
                issue.branch?.hasPrefix("exp/batch-") == true
            else { continue }
            if let current = byPrUrl[prUrl] {
                if createdAt(issue) > createdAt(current) {
                    byPrUrl[prUrl] = issue
                }
            } else {
                byPrUrl[prUrl] = issue
            }
        }
        return Array(byPrUrl.values)
    }

    /// The representative issue of the session's OWN open batch PR, matched by
    /// the row's stamped branch. Nil when the branch is missing, matches
    /// nothing, or is ambiguous.
    public static func resolve(
        sessionBranch: String?,
        openBatchPrs: [IssueEntity]
    ) -> IssueEntity? {
        guard let sessionBranch, !sessionBranch.isEmpty else { return nil }
        let matches = openBatchPrs.filter { $0.branch == sessionBranch }
        return matches.count == 1 ? matches.first : nil
    }

    /// created_at comes off the Electric wire as Postgres text and off
    /// tRPC/fixtures as ISO — WireTimestamps handles both (EXP-169); an
    /// unparseable value just loses the newest-wins comparison.
    private static func createdAt(_ issue: IssueEntity) -> Date {
        WireTimestamps.parse(issue.createdAt) ?? .distantPast
    }
}
