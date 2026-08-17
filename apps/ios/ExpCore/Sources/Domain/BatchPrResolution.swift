import Foundation

// EXP-535: batch sessions carry no issue/PR linkage (the schema has none — the
// server's in_review flip is equally loose), so a batch row resolves its open
// PR client-side: the team's open-PR issues on an `exp/batch-` branch,
// collapsed by prUrl keeping the NEWEST issue as the representative (Reviews
// pattern — merging through it merges the ONE batch PR, the server resolves
// every linked issue by exact pr_url). Only an UNAMBIGUOUS match (exactly one
// open batch PR in the team) offers the merge shortcut — with concurrent batch
// runs Reviews still lists every PR. Mirrors web's `use-agents-data.ts`.

public enum BatchPrResolution {
    /// The sole open batch PR's representative issue, or nil when there is
    /// none or more than one distinct open batch PR.
    ///
    /// Issues don't sync `team_id`, so callers pass the team's synced board
    /// ids as the scope (trashed/archived boards never sync, so presence in
    /// the set already implies visibility — same as StartPullRequestOption).
    public static func soleOpenBatchPr(
        issues: [IssueEntity],
        teamBoardIds: Set<String>
    ) -> IssueEntity? {
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
        guard byPrUrl.count == 1 else { return nil }
        return byPrUrl.values.first
    }

    /// created_at comes off the Electric wire as Postgres text and off
    /// tRPC/fixtures as ISO — WireTimestamps handles both (EXP-169); an
    /// unparseable value just loses the newest-wins comparison.
    private static func createdAt(_ issue: IssueEntity) -> Date {
        WireTimestamps.parse(issue.createdAt) ?? .distantPast
    }
}
