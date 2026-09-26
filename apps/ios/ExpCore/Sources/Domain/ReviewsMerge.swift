import Foundation

// EXP-1094: the ONE merge control a Reviews row carries, ×4 (web
// `lib/reviews-merge.ts`, desktop `domain::reviews_merge`, Android
// `ReviewsMerge.kt`), locked by
// `packages/domain-contract/fixtures/reviews-merge.json`.
//
// First match wins:
//   1. a workflow's FINAL PR row merges only while that PR is open;
//   2. a workflow NODE PR (its workflow running or paused) merges through the
//      workflow, never from the row;
//   3. a stack's bottom row merges the whole stack; an upper member merges
//      with it;
//   4. anything else is a plain Merge.
public enum ReviewsMerge {
    /// Where a row sits in its PR stack: `bottom` = the root row that merges
    /// the whole stack, `upper` = any member above it.
    public enum StackPosition: String, Sendable, Decodable {
        case none
        case bottom
        case upper
    }

    public enum Action: String, Sendable, Decodable {
        case merge
        case mergeStack = "merge_stack"
        case none
    }

    public struct Input: Sendable, Equatable, Decodable {
        public let stack: StackPosition
        /// The status of the workflow whose node covers this PR's issue.
        public let workflowStatus: String?
        /// The row IS a workflow's final PR.
        public let finalPr: Bool
        /// The final PR's state (`open` | `closed` | `merged`); nil off final rows.
        public let finalPrState: String?

        public init(
            stack: StackPosition = .none, workflowStatus: String? = nil,
            finalPr: Bool = false, finalPrState: String? = nil
        ) {
            self.stack = stack
            self.workflowStatus = workflowStatus
            self.finalPr = finalPr
            self.finalPrState = finalPrState
        }
    }

    public static let mergeLabel = "Merge"
    public static let mergeStackLabel = "Merge stack"
    /// An upper stack member's quiet caption.
    public static let mergesWithStack = "merges with its stack"
    /// A workflow node PR's quiet caption.
    public static let mergesThroughWorkflow = "merges through the workflow"

    public static func reviewRowMergeAction(_ input: Input) -> Action {
        if input.finalPr { return input.finalPrState == "open" ? .merge : .none }
        if liveWorkflow(input.workflowStatus) { return .none }
        switch input.stack {
        case .bottom: return .mergeStack
        case .upper: return .none
        case .none: return .merge
        }
    }

    /// Why a row offers no merge control; nil when it offers one or has
    /// nothing to say (a final PR that is not open).
    public static func reviewsMergeDisabledReason(_ input: Input) -> String? {
        if input.finalPr { return nil }
        if liveWorkflow(input.workflowStatus) { return mergesThroughWorkflow }
        if input.stack == .upper { return mergesWithStack }
        return nil
    }

    /// Issue id → the status of the workflow whose node covers it (the node's
    /// `issueId` or a `memberIssueIds` entry). A live workflow wins over a
    /// finished one covering the same issue.
    public static func workflowStatusByIssue(
        workflows: [(id: String, status: String)],
        nodes: [(workflowId: String, issueId: String, memberIssueIds: [String])]
    ) -> [String: String] {
        let statusById = Dictionary(workflows.map { ($0.id, $0.status) }, uniquingKeysWith: { a, _ in a })
        var byIssue: [String: String] = [:]
        for node in nodes {
            guard let status = statusById[node.workflowId] else { continue }
            for issueId in [node.issueId] + node.memberIssueIds {
                if byIssue[issueId] == nil
                    || (liveWorkflow(status) && !liveWorkflow(byIssue[issueId])) {
                    byIssue[issueId] = status
                }
            }
        }
        return byIssue
    }

    /// A PR's `workflowStatus` input over its linked issues: a live workflow
    /// first, else any covering one, else nil.
    public static func reviewWorkflowStatus(
        issueIds: [String], byIssue: [String: String]
    ) -> String? {
        let statuses = issueIds.compactMap { byIssue[$0] }
        return statuses.first(where: liveWorkflow) ?? statuses.first
    }

    private static func liveWorkflow(_ status: String?) -> Bool {
        status == "running" || status == "paused"
    }
}
