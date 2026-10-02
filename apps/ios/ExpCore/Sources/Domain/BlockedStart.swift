import Foundation

/// EXP-897/EXP-980/SLOP-3, the blocked-start prompt, byte-identical ×4 (web
/// `lib/blocked-start.ts`, desktop `domain::blocked_start`, Android
/// `BlockedStart.kt`) and locked by the contract fixture
/// `domain-contract/fixtures/blocked-start.json`.
///
/// A start on work that open issues block asks first: Cancel, Start anyway
/// (an ordinary run off the board's base branch) or Stacked PR. The stacked
/// start is PROMPT TEXT only: the same remote start, its `prompt` led by the
/// base instruction a follow-up run gets from the playbook. The open blockers
/// come from `IssueGraph.openBlockersOfSet`, the chain under the sentence
/// from `IssueGraph.blockGraph`.
public enum BlockedStart {
    // MARK: Copy

    /// The prompt's title for one picked issue.
    public static let blockedStartTitle = "This issue is blocked"
    /// The title when two or more issues were picked.
    public static let blockedBatchTitle = "Some of these issues are blocked"
    /// The batch body, above the graph.
    public static let blockedBatchBody = "Open issues outside this batch block it. Start anyway?"
    /// The sentence around the blocker identifiers (one picked issue).
    public static let bodyPrefix = "This issue is blocked by "
    /// The sentence's end while Stacked PR is disabled.
    public static let bodySuffix = ". Start anyway?"
    /// The sentence's end while Stacked PR is enabled.
    public static let bodySuffixStackable = ". Start anyway, or start a stacked PR?"
    public static let startAnywayLabel = "Start anyway"
    public static let stackedPrLabel = "Stacked PR"

    // MARK: Stack target

    /// Why Stacked PR is disabled. First match wins, in this order.
    public enum StackReason: String, CaseIterable, Sendable {
        /// Two or more picked issues.
        case batch
        /// Not exactly one open blocker.
        case many
        /// The blocker has no OPEN pull request with a recorded branch.
        case noPr = "no-pr"
        /// The subject or the blocker has no repository, or they differ.
        case repo
    }

    /// One open blocker as the stack rule sees it. `repositoryId` = the
    /// blocker's BOARD repository.
    public struct Blocker: Equatable, Sendable {
        public let identifier: String
        public let prState: String?
        public let branch: String?
        public let repositoryId: String?

        public init(identifier: String, prState: String?, branch: String?, repositoryId: String?) {
            self.identifier = identifier
            self.prState = prState
            self.branch = branch
            self.repositoryId = repositoryId
        }
    }

    /// The one blocker to stack on, or the one reason the button is disabled
    /// (exactly one of the two is set).
    public struct StackTarget: Equatable, Sendable {
        public let target: Blocker?
        public let reason: StackReason?
    }

    public static func stackTarget(
        pickedCount: Int,
        subjectRepositoryId: String?,
        blockers: [Blocker]
    ) -> StackTarget {
        if pickedCount > 1 { return StackTarget(target: nil, reason: .batch) }
        guard blockers.count == 1, let blocker = blockers.first else {
            return StackTarget(target: nil, reason: .many)
        }
        let branch = blocker.branch ?? ""
        if blocker.prState != DomainContract.prStateOpen || branch.isEmpty {
            return StackTarget(target: nil, reason: .noPr)
        }
        guard let subjectRepositoryId, let blockerRepo = blocker.repositoryId,
              subjectRepositoryId == blockerRepo else {
            return StackTarget(target: nil, reason: .repo)
        }
        return StackTarget(target: blocker, reason: nil)
    }

    /// The note under the disabled button; `ident` names the blocker.
    public static func stackDisabledNote(_ reason: StackReason, ident: String) -> String {
        switch reason {
        case .batch: return "A stacked PR starts one issue at a time."
        case .many: return "A stacked PR needs exactly one open blocker."
        case .noPr: return "#\(ident) has no open pull request yet."
        case .repo: return "#\(ident) lives in another repository."
        }
    }

    // MARK: Prompt

    static let promptTemplate =
        "Stacked on #{ident}. Before any edit: `git fetch origin {branch}`; if this branch "
        + "has no commits of its own, `git reset --hard origin/{branch}`, else "
        + "`git rebase origin/{branch}`. Open your PR with "
        + "`exponential_pr_open({issueId, base: \"{branch}\"})`."

    /// The stacked start's `prompt`: the base instruction, then the typed
    /// text (trimmed) after a blank line when there is any.
    public static func stackedStartPrompt(identifier: String, branch: String, text: String) -> String {
        let base = promptTemplate
            .replacingOccurrences(of: "{ident}", with: identifier)
            .replacingOccurrences(of: "{branch}", with: branch)
        let typed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        return typed.isEmpty ? base : "\(base)\n\n\(typed)"
    }
}
