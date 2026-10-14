import Foundation

/// EXP-1291/1303: the stack merge over `pull_requests` rows (EXP-1248's ONE
/// stack confirm, `stack-merge-choice.json` `confirm`, re-keyed from issue
/// rows to the PR's own `base_pull_request_id`, else head == base in one
/// repository), mirrored by Android `PullRequestStack` and desktop
/// `domain::pr_stack`. Only a LINEAR open stack of 2+ PRs (no fork) has one;
/// Merge = `pullRequests.merge({pullRequestId: top, stack: true})`, landing
/// the whole open chain bottom-up. The server refuses a plain merge of any
/// open-stack member, so such a member's Merge control IS this one.
/// EXP-1306: `stackView` = the thread Guide's stack rail
/// (`pr-stack-view.json`, replayed by `PrStackViewTests`).
public enum PullRequestStack {
    /// The confirm: `title` IS the primary button, `throughId` the PR merged
    /// with `stack = true`.
    public struct Confirm: Equatable, Sendable {
        public let title: String
        public let body: String
        public let throughId: String
    }

    /// The confirm's dismiss (`stack-merge-choice.json` `confirm.labels.cancel`).
    public static let cancelLabel = "Cancel"

    /// The PR `pr` is based on among `open`: the explicit
    /// `basePullRequestId`, else (EXP-1306, desktop `pr_stack::lower_of`) the
    /// first PR of the same repository whose head is `pr`'s base.
    static func lowerOf(_ pr: PullRequestEntity, in open: [PullRequestEntity]) -> PullRequestEntity? {
        if let baseId = nonEmpty(pr.basePullRequestId),
           let explicit = open.first(where: { $0.id == baseId && $0.id != pr.id }) {
            return explicit
        }
        guard let base = nonEmpty(pr.base) else { return nil }
        return open.first { $0.id != pr.id && $0.repositoryId == pr.repositoryId && nonEmpty($0.head) == base }
    }

    /// The open linear chain `pr` sits in, bottom-first; nil on a fork.
    public static func openChain(_ pr: PullRequestEntity, in all: [PullRequestEntity]) -> [PullRequestEntity]? {
        guard pr.isOpen else { return nil }
        let open = all.filter(\.isOpen)
        func uppers(_ of: PullRequestEntity) -> [PullRequestEntity] {
            open.filter { lowerOf($0, in: open)?.id == of.id }
        }
        var below: [PullRequestEntity] = []
        var seen: Set<String> = [pr.id]
        var cursor = pr
        while let base = lowerOf(cursor, in: open), seen.insert(base.id).inserted {
            below.insert(base, at: 0)
            cursor = base
        }
        let chain = below + [pr]
        // Walking up: a member with 2+ open children is a fork, no stack.
        if chain.contains(where: { uppers($0).count > 1 }) { return nil }
        var above: [PullRequestEntity] = []
        cursor = pr
        while let child = uppers(cursor).first, seen.insert(child.id).inserted {
            if uppers(child).count > 1 { return nil }
            above.append(child)
            cursor = child
        }
        return chain + above
    }

    /// Non-nil = `pr` is a member of an open stack: Merge lands it all.
    /// `through` = the rail's Merge through here on that member: it lands
    /// that member and everything beneath it, the rest stays open.
    public static func stackConfirm(
        _ pr: PullRequestEntity, in all: [PullRequestEntity], through: PullRequestEntity? = nil
    ) -> Confirm? {
        guard let chain = openChain(pr, in: all), chain.count >= 2 else { return nil }
        let index: Int? = if let through { chain.firstIndex { $0.id == through.id } } else { chain.count - 1 }
        guard let index else { return nil }
        let landing = chain[...index].map { "#\($0.number)" }
        let staysOpen = chain[(index + 1)...].map { "#\($0.number)" }
        let lands = landing.count == 1
            ? "Lands 1 pull request: \(landing[0])."
            : "Lands \(landing.count) pull requests, bottom-up: \(landing.joined(separator: ", "))."
        let open = staysOpen.isEmpty
            ? ""
            : " \(staysOpen.joined(separator: ", ")) \(staysOpen.count == 1 ? "stays" : "stay") open."
        return Confirm(
            title: through == nil ? DomainContract.diffUiMergeStack : DomainContract.diffUiMergeThrough,
            body: lands + open,
            throughId: chain[index].id
        )
    }

    // MARK: - EXP-1306: the stack rail

    /// One stack-rail row (desktop `pr_stack::StackViewRow`).
    public struct StackViewRow: Equatable, Sendable {
        public let pullRequestId: String
        /// The representative issue (its lowest identifier); nil = an
        /// issue-less PR.
        public let issueId: String?
        /// The representative's identifier, else `#<n>`.
        public let identifier: String
        /// The representative's title, else the PR's own.
        public let title: String
        public let prNumber: Int
        /// The thread a tap opens; nil = none synced.
        public let rootSessionId: String?
        /// The subject's PR: the row wears the active wash.
        public let isCurrent: Bool

        public init(
            pullRequestId: String, issueId: String?, identifier: String, title: String,
            prNumber: Int, rootSessionId: String?, isCurrent: Bool
        ) {
            self.pullRequestId = pullRequestId
            self.issueId = issueId
            self.identifier = identifier
            self.title = title
            self.prNumber = prNumber
            self.rootSessionId = rootSessionId
            self.isCurrent = isCurrent
        }
    }

    /// The rail: rows TOP first + the bottom member's base (nil = unknown).
    public struct StackView: Equatable, Sendable {
        public let rows: [StackViewRow]
        public let baseBranch: String?

        public init(rows: [StackViewRow], baseBranch: String?) {
            self.rows = rows
            self.baseBranch = baseBranch
        }
    }

    /// The Guide's stack rail of `pr`: nil unless it sits in a linear open
    /// stack of 2+. `issueIds` = per PR id the issues it closes
    /// (`pull_request_issues`); a PR closing several is named by its lowest
    /// identifier, an unsynced issue drops out.
    public static func stackView(
        _ pr: PullRequestEntity,
        in all: [PullRequestEntity],
        issueIds: [String: [String]],
        issues: [IssueEntity]
    ) -> StackView? {
        guard let chain = openChain(pr, in: all), chain.count >= 2 else { return nil }
        let byId = Dictionary(issues.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        return StackView(
            rows: chain.reversed().map { row in
                let issue = (issueIds[row.id] ?? [])
                    .compactMap { byId[$0] }
                    .min { ($0.identifier ?? "") < ($1.identifier ?? "") }
                return StackViewRow(
                    pullRequestId: row.id,
                    issueId: issue?.id,
                    identifier: issue?.identifier ?? "#\(row.number)",
                    title: issue?.title ?? row.title ?? "",
                    prNumber: row.number,
                    rootSessionId: row.rootSessionId,
                    isCurrent: row.id == pr.id
                )
            },
            baseBranch: nonEmpty(chain[0].base)
        )
    }

    private static func nonEmpty(_ value: String?) -> String? {
        guard let trimmed = value?.trimmingCharacters(in: .whitespaces), !trimmed.isEmpty else { return nil }
        return trimmed
    }
}
