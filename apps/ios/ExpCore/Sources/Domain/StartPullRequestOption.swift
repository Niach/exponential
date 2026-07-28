import Foundation

// Options for a `pr`-typed action input (EXP-259, mobile parity EXP-270): the
// team's OPEN issue-linked pull requests, deduped by prUrl. A batch coding run
// links several issues to ONE pull request, so a row lists every linked
// identifier and carries the REPRESENTATIVE issue's id as its value — exactly
// what `steer.startSession` resolves server-side (team-scoped, open-state
// checked). Mirrors the web picker in
// apps/web/src/components/launch-dialog/action-input-fields.tsx.

public struct StartPullRequestOption: Identifiable, Sendable, Equatable {
    /// The representative issue's id — the value the `pr` input submits.
    public let issueId: String
    public let prNumber: Int?
    /// Every issue identifier linked to this pull request, sorted.
    public let identifiers: [String]
    /// Every linked issue's id — the membership set `option(in:forIssueId:)`
    /// resolves against (EXP-323).
    public let linkedIssueIds: [String]

    public var id: String { issueId }

    public init(
        issueId: String,
        prNumber: Int?,
        identifiers: [String],
        linkedIssueIds: [String]? = nil
    ) {
        self.issueId = issueId
        self.prNumber = prNumber
        self.identifiers = identifiers
        self.linkedIssueIds = linkedIssueIds ?? [issueId]
    }

    /// `#42 · EXP-1, EXP-2` — the PR number when known, then the linked issues.
    public var label: String {
        let joined = identifiers.joined(separator: ", ")
        guard let prNumber else { return joined }
        return joined.isEmpty ? "#\(prNumber)" : "#\(prNumber) · \(joined)"
    }

    /// Collapse open-PR issue rows into one option per pull request.
    ///
    /// Issues don't sync `team_id`, so callers pass the team's synced board ids
    /// as the scope. Rows without a `prUrl` are skipped (an issue can carry
    /// `pr_state` without a URL only in malformed data, and the id would not
    /// resolve server-side anyway), and rows whose `pr_state` is not `open`
    /// are filtered HERE — the open-PR guarantee lives in the helper, not in
    /// the caller's SQL (matching Android's builder). Ordering is stable: by
    /// label, so the list doesn't reshuffle as sync lands rows.
    public static func build(
        from issues: [IssueEntity],
        teamBoardIds: Set<String>
    ) -> [StartPullRequestOption] {
        var byPrUrl: [String: (
            issueId: String,
            prNumber: Int?,
            identifiers: [String],
            linkedIssueIds: [String]
        )] = [:]
        // Deterministic representative + identifier order regardless of the
        // fetch order GRDB happened to return.
        for issue in issues.sorted(by: { $0.id < $1.id }) {
            guard teamBoardIds.contains(issue.boardId),
                issue.prState == DomainContract.prStateOpen,
                let prUrl = issue.prUrl, !prUrl.isEmpty
            else { continue }
            let identifier = issue.identifier ?? ""
            if var entry = byPrUrl[prUrl] {
                if !identifier.isEmpty { entry.identifiers.append(identifier) }
                entry.linkedIssueIds.append(issue.id)
                byPrUrl[prUrl] = entry
            } else {
                byPrUrl[prUrl] = (
                    issueId: issue.id,
                    prNumber: issue.prNumber,
                    identifiers: identifier.isEmpty ? [] : [identifier],
                    linkedIssueIds: [issue.id]
                )
            }
        }
        return byPrUrl.values
            .map {
                StartPullRequestOption(
                    issueId: $0.issueId,
                    prNumber: $0.prNumber,
                    identifiers: $0.identifiers.sorted(),
                    linkedIssueIds: $0.linkedIssueIds
                )
            }
            .sorted { ($0.label, $0.issueId) < ($1.label, $1.issueId) }
    }

    /// The option a given issue id belongs to — by MEMBERSHIP, not by the
    /// representative id. A caller seeding the `pr` input holds whatever issue
    /// its surface acts on (the Reviews row picks the NEWEST linked issue,
    /// `build` the lowest one), but only the representative id matches a
    /// `Picker` tag, so seeds are normalised through this (EXP-323).
    public static func option(
        in options: [StartPullRequestOption],
        forIssueId issueId: String
    ) -> StartPullRequestOption? {
        options.first { $0.linkedIssueIds.contains(issueId) }
    }
}
