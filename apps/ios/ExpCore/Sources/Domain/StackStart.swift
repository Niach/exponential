import Foundation

/// EXP-897 — the blocked-issue start prompt, one pure rule mirrored ×4 (web
/// `lib/stack-start.ts`, desktop `chat_launch.rs`, Android `StackStart.kt`).
///
/// Starting a run on an issue that is BLOCKED by unfinished work has three
/// answers, and the composer asks before it sends: start anyway (an ordinary
/// run off the board's base branch), start a STACKED pull request (the branch
/// is cut from the blocker's PR branch and the PR is based on it, so the diff
/// shows only this issue's own work and the run builds the blocker first if
/// nobody has), or cancel.
///
/// The copy is byte-identical on every client; only the chip rendering
/// diverges (a UIKit alert cannot host chips, so iOS prints the identifiers
/// monospaced inside the sentence).
public enum StackStart {
    /// The prompt's title.
    public static let blockedStartTitle = "This issue is blocked"
    /// The secondary answer: an ordinary, unstacked run.
    public static let startAnywayLabel = "Start anyway"
    /// The primary answer: cut from the blocker's branch, base the PR on it.
    public static let stackedPrLabel = "Stacked PR"
    /// The sentence around the blocker chips.
    public static let bodyPrefix = "This issue is blocked by "
    public static let bodySuffix = ". Start anyway, or start a stacked PR?"

    // EXP-980: the prompt also asks for a BATCH (blockers outside the picked
    // set, `IssueGraph.openBlockersOfSet`), draws the transitive chain as the
    // mini-graph, and never hides the stacked button: it is DISABLED with one
    // of the reason notes below. All byte-identical ×4.

    /// The title when two or more issues were picked.
    public static let blockedBatchTitle = "Some of these issues are blocked"
    /// The batch body, above the graph.
    public static let blockedBatchBody = "Open issues outside this batch block it. Start anyway?"
    /// Why "Stacked PR" is disabled: the runner device lacks the
    /// `stacked-start` capability (an older app or daemon).
    public static let stackNeedsUpdateNote = "Update Exponential on this device to start stacked PRs."
    /// Why "Stacked PR" is disabled for a batch.
    public static let stackSingleIssueNote = "A stacked PR starts one issue at a time."
    /// Why "Stacked PR" is disabled on a blocking cycle.
    public static let stackCycleNote = "These issues block each other in a cycle. Remove one relation to stack them."

    /// Why the stacked start is off; nil means it is on.
    public enum StackDisabledReason: String, Sendable {
        case cycle
        case batch
        case cap
    }

    /// One reason at a time, the most fundamental first: a cycle can never
    /// stack, a batch never does, a missing capability is fixed by an update.
    public static func stackDisabledReason(
        pickedCount: Int, canStack: Bool, hasCycle: Bool
    ) -> StackDisabledReason? {
        if hasCycle { return .cycle }
        if pickedCount > 1 { return .batch }
        if !canStack { return .cap }
        return nil
    }

    public static func stackDisabledNote(_ reason: StackDisabledReason) -> String {
        switch reason {
        case .cycle: return stackCycleNote
        case .batch: return stackSingleIssueNote
        case .cap: return stackNeedsUpdateNote
        }
    }

    /// The OPEN blockers of `issueId`: the issues that must land first.
    ///
    /// Exactly the inverse side of the canonical `blocks` row — a row whose
    /// `relatedIssueId` is this issue means "this issue is blocked by
    /// `issueId`'s counterpart". Three drops, in order:
    ///
    /// 1. Only `type == blocks` rows pointing AT this issue (a `parent`,
    ///    `duplicate` or `related` row blocks nothing, and the FORWARD side
    ///    means this issue blocks the other one).
    /// 2. A blocker whose issue row has not synced is unknowable — it cannot
    ///    be named, ordered or stacked on, so it is not a blocker here.
    /// 3. A blocker whose ANCHOR status is terminal (`done`, `cancelled`,
    ///    `duplicate`) is finished work — it blocks nothing.
    ///
    /// Ordered by identifier (deduped by issue id), so the sentence and the
    /// stack read the same on every client.
    public static func openBlockers(
        issueId: String,
        relations: [IssueRelationEntity],
        issues: [IssueEntity]
    ) -> [IssueEntity] {
        let byId = Dictionary(issues.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        var seen = Set<String>()
        var out: [IssueEntity] = []
        for relation in relations
        where relation.type == IssueRelationType.blocks.rawValue
            && relation.relatedIssueId == issueId {
            let blockerId = relation.issueId
            guard !seen.contains(blockerId) else { continue }
            guard let blocker = byId[blockerId] else { continue }
            guard !isFinished(blocker) else { continue }
            seen.insert(blockerId)
            out.append(blocker)
        }
        return out.sorted { orderKey($0) < orderKey($1) }
    }

    /// The terminal anchors: a blocker in one of them is done with.
    private static func isFinished(_ issue: IssueEntity) -> Bool {
        switch IssueStatus.from(issue.status) {
        case .done, .cancelled, .duplicate: return true
        case .backlog, .inProgress, .inReview: return false
        }
    }

    /// Identifier order, id as the deterministic tie-break for a row whose
    /// identifier has not been stamped yet.
    private static func orderKey(_ issue: IssueEntity) -> String {
        let identifier = issue.identifier ?? ""
        return identifier.isEmpty ? "~\(issue.id)" : identifier
    }
}
