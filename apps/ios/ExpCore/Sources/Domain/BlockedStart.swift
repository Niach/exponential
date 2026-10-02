import Foundation

/// EXP-897/EXP-980/SLOP-3, the blocked-start prompt, byte-identical ×4 (web
/// `lib/blocked-start.ts`, desktop `domain::blocked_start`, Android
/// `BlockedStart.kt`) and locked by the contract fixture
/// `domain-contract/fixtures/blocked-start.json`.
///
/// A start on work that open issues block asks first: Cancel, Start anyway
/// (an ordinary run off the board's base branch) or Stacked PR. A stacked
/// start builds the whole dependency LINE bottom-up and is PROMPT TEXT only:
/// the client starts the line's lowest unstarted issue (`StackPlan.run[0]`)
/// with `stackedStartPrompt`, and the playbook's follow-up rule carries the
/// line from run to run. Two pure steps: `stackLine` walks the line down from
/// the subject, `stackPlan` turns it into a plan or the one reason the button
/// is disabled.
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

    /// The most issues one stacked start may start in a line.
    public static let maxRun = 5

    // MARK: Line

    /// The issues BELOW the subject along its single open-blocker line,
    /// BOTTOM first; `fork` = the issue with more than one open blocker that
    /// stopped the walk; `cycle` = the walk met an issue twice.
    public struct StackLine: Sendable {
        public let line: [IssueEntity]
        public let fork: IssueEntity?
        public let cycle: Bool
    }

    /// Walk DOWN from `subjectId` over the open direct blockers
    /// (`IssueGraph.openBlockers`): none = stop; exactly one = it joins the
    /// line and the walk continues from it; more = a fork at the current
    /// issue; an issue seen twice = a cycle.
    public static func stackLine(
        subjectId: String,
        relations: [IssueRelationEntity],
        issues: [IssueEntity]
    ) -> StackLine {
        var above: [IssueEntity] = []
        var seen: Set<String> = [subjectId]
        var cursorId = subjectId
        var fork: IssueEntity?
        var cycle = false
        let byId = Dictionary(issues.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        while true {
            let blockers = IssueGraph.openBlockers(
                issueId: cursorId, relations: relations, issues: issues
            )
            if blockers.isEmpty { break }
            if blockers.count > 1 {
                fork = byId[cursorId]
                break
            }
            let next = blockers[0]
            if seen.contains(next.id) {
                cycle = true
                break
            }
            seen.insert(next.id)
            above.append(next)
            cursorId = next.id
        }
        return StackLine(line: above.reversed(), fork: fork, cycle: cycle)
    }

    // MARK: Plan

    /// Why Stacked PR is disabled. First match wins, in this order.
    public enum StackReason: String, CaseIterable, Sendable {
        /// Two or more picked issues.
        case batch
        /// The line runs into a cycle.
        case cycle
        /// An issue on the line has more than one open blocker.
        case many
        /// A line member lives in another repository (or none).
        case repo
        /// More than `maxRun` issues would start.
        case long
        /// A run member below the subject already runs without an open PR.
        case running
    }

    /// The picked issue as the plan sees it.
    public struct Subject: Equatable, Sendable {
        public let identifier: String
        public let repositoryId: String?

        public init(identifier: String, repositoryId: String?) {
            self.identifier = identifier
            self.repositoryId = repositoryId
        }
    }

    /// One line member as the plan sees it. `repositoryId` = its BOARD's
    /// repository; `running` = a live run on it and no open PR yet.
    public struct LineMember: Equatable, Sendable {
        public let identifier: String
        public let prState: String?
        public let branch: String?
        public let repositoryId: String?
        public let running: Bool

        public init(
            identifier: String, prState: String?, branch: String?,
            repositoryId: String?, running: Bool
        ) {
            self.identifier = identifier
            self.prState = prState
            self.branch = branch
            self.repositoryId = repositoryId
            self.running = running
        }
    }

    /// The open pull request the line stacks on.
    public struct StackBase: Equatable, Sendable {
        public let identifier: String
        public let branch: String

        public init(identifier: String, branch: String) {
            self.identifier = identifier
            self.branch = branch
        }
    }

    /// What a stacked start does: `run[0]` starts on `base` (nil = the
    /// default branch); `run` ends with the subject.
    public struct StackPlan: Equatable, Sendable {
        public let base: StackBase?
        public let run: [String]

        public init(base: StackBase?, run: [String]) {
            self.base = base
            self.run = run
        }
    }

    /// The plan, or the one reason the button is disabled (exactly one of
    /// the two is set); `ident` names the issue the reason's note mentions.
    public struct StackPlanResult: Equatable, Sendable {
        public let plan: StackPlan?
        public let reason: StackReason?
        public let ident: String?

        /// The note under the disabled button.
        public var note: String? {
            reason.map { BlockedStart.stackDisabledNote($0, ident: ident ?? "") }
        }

        /// The note under the enabled button when the run starts 2+ issues.
        public var planNote: String? {
            plan.flatMap { BlockedStart.stackPlanNote($0.run) }
        }
    }

    public static func stackPlan(
        pickedCount: Int,
        subject: Subject,
        line: [LineMember],
        fork: String?,
        cycle: Bool
    ) -> StackPlanResult {
        func disabled(_ reason: StackReason, _ ident: String? = nil) -> StackPlanResult {
            StackPlanResult(plan: nil, reason: reason, ident: ident)
        }
        if pickedCount > 1 { return disabled(.batch) }
        if cycle { return disabled(.cycle) }
        if let fork { return disabled(.many, fork) }
        if subject.repositoryId == nil {
            if let bottom = line.first { return disabled(.repo, bottom.identifier) }
        } else if let other = line.first(where: {
            $0.repositoryId == nil || $0.repositoryId != subject.repositoryId
        }) {
            return disabled(.repo, other.identifier)
        }

        let baseIndex = line.lastIndex {
            $0.prState == DomainContract.prStateOpen && !($0.branch ?? "").isEmpty
        }
        let base = baseIndex.map {
            StackBase(identifier: line[$0].identifier, branch: line[$0].branch ?? "")
        }
        let below = baseIndex.map { Array(line[($0 + 1)...]) } ?? line
        let run = below.map(\.identifier) + [subject.identifier]

        if run.count > maxRun { return disabled(.long) }
        if let busy = below.first(where: \.running) { return disabled(.running, busy.identifier) }
        return StackPlanResult(plan: StackPlan(base: base, run: run), reason: nil, ident: nil)
    }

    // MARK: Notes

    /// The note under the disabled button; `ident` names the issue in
    /// question (the fork, the foreign or the running member).
    public static func stackDisabledNote(_ reason: StackReason, ident: String) -> String {
        switch reason {
        case .batch: return "A stacked PR starts one issue at a time."
        case .cycle:
            return "These issues block each other in a cycle. Remove one relation to stack them."
        case .many: return "#\(ident) has more than one open blocker. A stacked PR follows one line."
        case .repo: return "#\(ident) lives in another repository."
        case .long: return "A stacked PR starts at most \(maxRun) issues in a line."
        case .running: return "#\(ident) is already running. Its pull request is not open yet."
        }
    }

    static let planNoteTemplate = "Starts #{first} first, then #{rest}."

    /// The note under the enabled button, nil for a run of one.
    public static func stackPlanNote(_ run: [String]) -> String? {
        guard run.count >= 2 else { return nil }
        return planNoteTemplate
            .replacingOccurrences(of: "{first}", with: run[0])
            .replacingOccurrences(of: "{rest}", with: run.dropFirst().joined(separator: ", then #"))
    }

    // MARK: Prompt

    static let baseTemplate =
        "Stacked on #{ident}. Before any edit: `git fetch origin {branch}`; if this branch "
        + "has no commits of its own, `git reset --hard origin/{branch}`, else "
        + "`git rebase origin/{branch}`. Open your PR with "
        + "`exponential_pr_open({issueId, base: \"{branch}\"})`."

    static let lineTemplate =
        "This is the bottom of a stacked line: {line}. After your PR is open and your branch "
        + "is pushed, start #{next} as a follow-up run on your branch (playbook: Follow-ups "
        + "and follow-up runs), even under `no follow-up runs`. If more issues follow it, "
        + "give it this paragraph with your own issue dropped from the line. Do not wait for it."

    static let textTemplate = "Instructions for #{subject}, pass them along unchanged:\n{text}"

    /// The `prompt` `run[0]`'s start carries, its parts joined by a blank
    /// line: the base instruction (when stacked on an open PR), the line
    /// paragraph (a run of 2+), the trimmed typed text (raw for a run of
    /// one, else addressed to the subject, the last of the run).
    public static func stackedStartPrompt(plan: StackPlan, text: String) -> String {
        var parts: [String] = []
        if let base = plan.base {
            parts.append(
                baseTemplate
                    .replacingOccurrences(of: "{ident}", with: base.identifier)
                    .replacingOccurrences(of: "{branch}", with: base.branch)
            )
        }
        if plan.run.count >= 2 {
            parts.append(
                lineTemplate
                    .replacingOccurrences(
                        of: "{line}",
                        with: plan.run.map { "#\($0)" }.joined(separator: ", then ")
                    )
                    .replacingOccurrences(of: "{next}", with: plan.run[1])
            )
        }
        let typed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if !typed.isEmpty {
            if plan.run.count <= 1 {
                parts.append(typed)
            } else {
                parts.append(
                    textTemplate
                        .replacingOccurrences(of: "{subject}", with: plan.run[plan.run.count - 1])
                        .replacingOccurrences(of: "{text}", with: typed)
                )
            }
        }
        return parts.joined(separator: "\n\n")
    }
}
