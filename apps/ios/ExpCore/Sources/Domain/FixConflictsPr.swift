import Foundation

/// EXP-1233: the pull request picked into the Fix merge conflicts builtin, as
/// the composer draws it — the headline's issue chips and the conflict card's
/// PR row (web `resolveFixConflictsPr` + `branchLine`, ×4). Resolved off the
/// synced issue rows: the REPRESENTATIVE issue (the `pr` input's value) and
/// every open-PR row sharing its `prUrl` (a batch PR links several), sorted by
/// identifier.
public struct FixConflictsPr: Equatable, Sendable {
    /// One linked issue as its chip shows it.
    public struct Issue: Equatable, Sendable, Identifiable {
        public let id: String
        public let identifier: String?
        public let title: String
        /// The wire status (the anchor enum), for the chip's glyph.
        public let status: String

        public init(id: String, identifier: String?, title: String, status: String) {
            self.id = id
            self.identifier = identifier
            self.title = title
            self.status = status
        }
    }

    /// The representative issue id — the `pr` input's value.
    public let issueId: String
    public let prNumber: Int?
    public let branch: String?
    public let baseBranch: String?
    /// Every synced issue the pull request links, by identifier.
    public let issues: [Issue]

    public init(
        issueId: String,
        prNumber: Int?,
        branch: String?,
        baseBranch: String?,
        issues: [Issue]
    ) {
        self.issueId = issueId
        self.prNumber = prNumber
        self.branch = branch
        self.baseBranch = baseBranch
        self.issues = issues
    }

    /// The row's branch text: `exp/APP-14 → master`, or the branch alone
    /// while the base is unknown; `` without a branch. Pure, ×4.
    public static func branchLine(branch: String?, base: String?) -> String {
        guard let branch, !branch.isEmpty else { return "" }
        guard let base, !base.isEmpty else { return branch }
        return "\(branch) → \(base)"
    }

    /// `branchLine` of this pull request.
    public var branchLine: String {
        Self.branchLine(branch: branch, base: baseBranch)
    }

    /// The picked pull request off the synced rows: nil with nothing picked
    /// or the representative row not (yet) synced. `issues` = the rows the
    /// caller holds (any order); the linked set is every OPEN-PR row sharing
    /// the representative's `prUrl`, sorted by identifier.
    public static func resolve(prIssueId: String?, issues: [IssueEntity]) -> FixConflictsPr? {
        guard let prIssueId, !prIssueId.isEmpty,
              let representative = issues.first(where: { $0.id == prIssueId })
        else { return nil }
        var linked: [IssueEntity]
        if let url = representative.prUrl, !url.isEmpty {
            linked = issues
                .filter { $0.prUrl == url && $0.prState == DomainContract.prStateOpen }
                .sorted { ($0.identifier ?? "", $0.id) < ($1.identifier ?? "", $1.id) }
        } else {
            linked = [representative]
        }
        if linked.isEmpty { linked = [representative] }
        return FixConflictsPr(
            issueId: representative.id,
            prNumber: representative.prNumber,
            branch: representative.branch,
            baseBranch: representative.prBaseBranch,
            issues: linked.map {
                Issue(id: $0.id, identifier: $0.identifier, title: $0.title, status: $0.status)
            }
        )
    }
}
