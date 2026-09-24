import Foundation

/// EXP-818: the session TREE — a run started by another run through
/// `exponential_sessions_start` carries `parent_session_id`, and every session
/// list (the Devices screen's Running/Recent sections here; the rail and the
/// Agent page on web/desktop) nests it under its parent instead of listing it
/// as a stranger.
///
/// ONE pure rule, mirrored ×4 (web `lib/session-tree.ts`, desktop
/// `domain::session_tree`, Android `SessionTree.kt`) with the same four tests:
///
/// 1. The caller's order is the ROOT order — the tree never re-sorts roots.
/// 2. A row is a child iff its parent names ANOTHER row of the input; an
///    unlisted parent leaves the child a root at depth 0.
/// 3. Children follow their parent directly, oldest start first (then id),
///    recursively — depth grows by one per level.
/// 4. A cycle (defensive) breaks at the first repeat.
public enum SessionTree {
    public struct Row<T> {
        public let session: T
        /// 0 for a root, +1 per nesting level.
        public let depth: Int
        /// Whether at least one child is nested right below.
        public let hasChildren: Bool
    }

    public static func nest<T>(
        _ sessions: [T],
        id: (T) -> String,
        parent: (T) -> String?,
        startedAt: (T) -> String?
    ) -> [Row<T>] {
        let ids = Set(sessions.map(id))
        func isChild(_ session: T) -> Bool {
            guard let p = parent(session) else { return false }
            return p != id(session) && ids.contains(p)
        }
        var childrenOf: [String: [Int]] = [:]
        for (index, session) in sessions.enumerated() where isChild(session) {
            childrenOf[parent(session) ?? "", default: []].append(index)
        }
        for key in childrenOf.keys {
            childrenOf[key]?.sort { a, b in
                let (sa, sb) = (sessions[a], sessions[b])
                let (ta, tb) = (startedAt(sa) ?? "", startedAt(sb) ?? "")
                return ta != tb ? ta < tb : id(sa) < id(sb)
            }
        }
        var placed = Array(repeating: false, count: sessions.count)
        var order: [(Int, Int, Bool)] = []
        func visit(_ index: Int, depth: Int) {
            if placed[index] { return }
            placed[index] = true
            let children = (childrenOf[id(sessions[index])] ?? []).filter { !placed[$0] }
            order.append((index, depth, !children.isEmpty))
            for child in children { visit(child, depth: depth + 1) }
        }
        for (index, session) in sessions.enumerated() where !isChild(session) {
            visit(index, depth: 0)
        }
        // A child whose ancestry cycled without a root — keep it, at depth 0.
        for index in sessions.indices { visit(index, depth: 0) }
        return order.map { Row(session: sessions[$0.0], depth: $0.1, hasChildren: $0.2) }
    }

    /// The synced-row convenience: nests `CodingSessionEntity` rows.
    public static func nest(_ sessions: [CodingSessionEntity]) -> [Row<CodingSessionEntity>] {
        nest(sessions, id: { $0.id }, parent: { $0.parentSessionId }, startedAt: { $0.startedAt })
    }

    /// EXP-897: the rows a FOLDED list draws — every row whose parent (at any
    /// depth) is collapsed drops out. The ×4 rule (web `visibleTreeRows`,
    /// desktop `sessions_section::drop_collapsed`, Android
    /// `SessionTree.visibleRows`): only a row with children can collapse, and
    /// collapsing hides its whole subtree, however deep.
    public static func visibleRows<T>(
        _ rows: [Row<T>], collapsed: Set<String>, rowId: (T) -> String
    ) -> [Row<T>] {
        var out: [Row<T>] = []
        var hideBelow: Int?
        for row in rows {
            if let depth = hideBelow, row.depth > depth { continue }
            hideBelow = nil
            out.append(row)
            if row.hasChildren, collapsed.contains(rowId(row.session)) { hideBelow = row.depth }
        }
        return out
    }

    /// The synced-row convenience.
    public static func visibleRows(
        _ rows: [Row<CodingSessionEntity>], collapsed: Set<String>
    ) -> [Row<CodingSessionEntity>] {
        visibleRows(rows, collapsed: collapsed, rowId: { $0.id })
    }

    /// The ids of every row nested (at any depth) under `id`.
    public static func descendantIds<T>(_ rows: [Row<T>], of id: String, rowId: (T) -> String) -> [String] {
        guard let start = rows.firstIndex(where: { rowId($0.session) == id }) else { return [] }
        let depth = rows[start].depth
        var out: [String] = []
        for row in rows[(start + 1)...] {
            if row.depth <= depth { break }
            out.append(rowId(row.session))
        }
        return out
    }
}

// MARK: - The node tree (EXP-996, contract EXP-1029)

/// EXP-996 — the session list as a TREE of NODES, not a flat roll of
/// strangers. `nest` above still answers "which row hangs off which"; this is
/// the selector that also says a dozen rows are ONE thing: the nodes of a
/// workflow, a stack, a resume succession.
///
/// The CONTRACT is web `lib/sessions/session-tree.ts`; these are its rules, in
/// this order, with the same test names (`SessionTreeTests`, desktop
/// `session_tree`, Android `SessionTree.kt`):
///
///   1. Resumed runs COLLAPSE: every resume succession (`RunChain`, EXP-974)
///      is ONE node, keyed by its newest row, `chain` oldest-first.
///   2. Children nest under their `parentSessionId`, following the parent's
///      resume succession (EXP-906: a resume inherits it).
///   3. The sessions of ONE workflow group under a workflow node, NAMED by the
///      `workflows` row — a workflow the caller did not sync leaves its runs
///      ungrouped, so `startedReason == workflow` alone never groups.
///   4. A stack (`issues.pr_base_branch`, `PrStack`) groups in LINEAR order,
///      lowest first, and only when TWO of its runs are listed. Stacks and
///      workflows are NOT unified: a stack is a linear group with its own icon.
///   5. Groups and top-level nodes sort by last activity, newest first;
///      children keep creation order, and a parent's activity counts its whole
///      subtree (so folding one never moves it).
///   6. An orphan child whose parent is gone (swept, not synced) sits at top
///      level.
///
/// CONCRETE over `CodingSessionEntity`, not generic like `nest` above: rule 1
/// is `RunChain.chain`, which is entity-typed, and going generic would mean a
/// second copy of its backwards/forwards walk. A view whose row wraps a
/// session (`AgentsViewModel.Row`) passes `rows.map(\.session)` and looks its
/// own row back up by `session.id` — which is what web's list does too.
extension SessionTree {

    /// EXP-996: what a stack GROUP row is called (a workflow group wears its
    /// own name). Byte-identical ×4 (web `STACK_GROUP_LABEL`).
    public static let stackGroupLabel = "Stacked pull requests"
    /// A group row's fold labels — the session rows' own pair says CHILD RUNS,
    /// which a group has none of.
    public static let collapseGroupLabel = "Collapse these runs"
    public static let expandGroupLabel = "Expand these runs"

    /// What the rows alone cannot say: which workflow an issue belongs to, and
    /// which issues stack on which. Every list is optional — a caller with no
    /// workflows synced still gets the session/parent tree.
    public struct Context: Sendable {
        /// The named workflows: a run only groups under one that is HERE,
        /// because this is where the group row's name comes from.
        public struct Workflow: Sendable {
            public let id: String
            public let name: String
            public let status: String

            public init(id: String, name: String, status: String) {
                self.id = id
                self.name = name
                self.status = status
            }
        }

        /// `workflow_nodes`: which issue (or which run) sits in which workflow.
        public struct WorkflowNode: Sendable {
            public let workflowId: String
            public let issueId: String
            public let sessionId: String?

            public init(workflowId: String, issueId: String, sessionId: String? = nil) {
                self.workflowId = workflowId
                self.issueId = issueId
                self.sessionId = sessionId
            }
        }

        /// The stack EDGES — the issues the listed sessions name (web's
        /// `PrStackNode` pick).
        public struct StackIssue: Sendable {
            public let id: String
            public let branch: String?
            public let prBaseBranch: String?

            public init(id: String, branch: String?, prBaseBranch: String?) {
                self.id = id
                self.branch = branch
                self.prBaseBranch = prBaseBranch
            }
        }

        public let workflows: [Workflow]
        public let workflowNodes: [WorkflowNode]
        public let issues: [StackIssue]

        public init(
            workflows: [Workflow] = [],
            workflowNodes: [WorkflowNode] = [],
            issues: [StackIssue] = []
        ) {
            self.workflows = workflows
            self.workflowNodes = workflowNodes
            self.issues = issues
        }

        /// The synced-row convenience: the shapes as they arrive.
        public init(
            workflows: [WorkflowEntity],
            workflowNodes: [WorkflowNodeEntity],
            issues: [IssueEntity]
        ) {
            self.init(
                workflows: workflows.map {
                    Workflow(id: $0.id, name: $0.name, status: $0.status)
                },
                workflowNodes: workflowNodes.map {
                    WorkflowNode(
                        workflowId: $0.workflowId, issueId: $0.issueId, sessionId: $0.sessionId
                    )
                },
                issues: issues.map {
                    StackIssue(id: $0.id, branch: $0.branch, prBaseBranch: $0.prBaseBranch)
                }
            )
        }
    }

    /// One run — its whole resume succession — and whatever it started.
    public struct SessionNode: Sendable {
        /// The NEWEST row of the succession: the node's identity.
        public let session: CodingSessionEntity
        /// The succession oldest-first (`RunChain`); `[session]` when unresumed.
        public let chain: [CodingSessionEntity]
        public let children: [Node]
        /// The newest `updatedAt` across the chain AND the children, seconds.
        public let lastActivityAt: TimeInterval

        public init(
            session: CodingSessionEntity,
            chain: [CodingSessionEntity],
            children: [Node],
            lastActivityAt: TimeInterval
        ) {
            self.session = session
            self.chain = chain
            self.children = children
            self.lastActivityAt = lastActivityAt
        }
    }

    /// The runs of ONE workflow (EXP-978), under a row that links to it.
    public struct WorkflowGroup: Sendable {
        public let workflowId: String
        public let name: String
        /// The node runs, newest first.
        public let children: [Node]
        public let lastActivityAt: TimeInterval

        public init(
            workflowId: String, name: String, children: [Node], lastActivityAt: TimeInterval
        ) {
            self.workflowId = workflowId
            self.name = name
            self.children = children
            self.lastActivityAt = lastActivityAt
        }
    }

    /// One stack (EXP-897), LINEAR: lowest first.
    public struct StackGroup: Sendable {
        /// The lowest issue of the chain.
        public let rootIssueId: String
        public let children: [Node]
        public let lastActivityAt: TimeInterval

        public init(rootIssueId: String, children: [Node], lastActivityAt: TimeInterval) {
            self.rootIssueId = rootIssueId
            self.children = children
            self.lastActivityAt = lastActivityAt
        }
    }

    public enum Node: Sendable {
        case session(SessionNode)
        case workflow(WorkflowGroup)
        case stack(StackGroup)

        public var children: [Node] {
            switch self {
            case let .session(node): node.children
            case let .workflow(group): group.children
            case let .stack(group): group.children
            }
        }

        public var lastActivityAt: TimeInterval {
            switch self {
            case let .session(node): node.lastActivityAt
            case let .workflow(group): group.lastActivityAt
            case let .stack(group): group.lastActivityAt
            }
        }

        /// The run behind a session node; nil on a group row.
        public var sessionNode: SessionNode? {
            switch self {
            case let .session(node): node
            case .workflow, .stack: nil
            }
        }

        /// This node's stable identity (`SessionTree.nodeKey`).
        public var key: String { SessionTree.nodeKey(self) }
    }

    /// A node's stable identity — the key a collapsed set and a list use. Web
    /// `sessionTreeNodeKey`.
    public static func nodeKey(_ node: Node) -> String {
        switch node {
        case let .session(entry): entry.session.id
        case let .workflow(group): "workflow:\(group.workflowId)"
        case let .stack(group): "stack:\(group.rootIssueId)"
        }
    }

    /// One row of a DRAWN session tree: a node, how deep it sits and whether
    /// it can fold — what the EXP-965 connector is computed over.
    public struct FlatRow: Sendable {
        public let node: Node
        public let key: String
        public let depth: Int
        public let hasChildren: Bool

        public init(node: Node, key: String, depth: Int, hasChildren: Bool) {
            self.node = node
            self.key = key
            self.depth = depth
            self.hasChildren = hasChildren
        }
    }

    // MARK: - The selector

    /// The sessions list as a tree. Pure: no clock, no IO; sort ties break on
    /// the node key, so two clients agree.
    public static func sessionTree(
        _ sessions: [CodingSessionEntity],
        context: Context = Context()
    ) -> [Node] {
        // Rule 1. Oldest row first, so the primary succession (`RunChain`'s
        // newest-successor walk) claims its members before an older fork
        // sibling does; whatever is left becomes its own node.
        let ordered = sessions.sorted { a, b in
            let (sa, sb) = (stamp(a.createdAt), stamp(b.createdAt))
            return sa != sb ? sa < sb : a.id < b.id
        }
        var canonicalOf: [String: String] = [:]
        var chainOf: [String: [CodingSessionEntity]] = [:]
        for row in ordered {
            if canonicalOf[row.id] != nil { continue }
            let chain = RunChain.chain(sessions, sessionId: row.id)
                .filter { canonicalOf[$0.id] == nil }
            let canonical = chain.last ?? row
            for member in chain { canonicalOf[member.id] = canonical.id }
            chainOf[canonical.id] = chain.isEmpty ? [row] : chain
        }

        // Rule 2: a child follows its parent's whole SUCCESSION, named by the
        // newest row of the chain that names one at all.
        var parentOf: [String: String] = [:]
        for (canonicalId, chain) in chainOf {
            var named: String?
            for member in chain.reversed() {
                if let parent = member.parentSessionId, !parent.isEmpty {
                    named = parent
                    break
                }
            }
            // Rule 6: a parent that is gone (swept, another team, not synced)
            // leaves the child at top level; so does a row naming itself.
            if let parent = named.flatMap({ canonicalOf[$0] }), parent != canonicalId {
                parentOf[canonicalId] = parent
            }
        }

        // A cycle (never written by the server, but a synced row is a synced
        // row) leaves the row it closes on at top level.
        let nodeIds = Set(chainOf.keys)
        var childrenOf: [String: [String]] = [:]
        var rootIds: [String] = []
        for canonicalId in chainOf.keys {
            if let parent = ancestor(canonicalId, parentOf: parentOf, nodeIds: nodeIds) {
                childrenOf[parent, default: []].append(canonicalId)
            } else {
                rootIds.append(canonicalId)
            }
        }

        var activity: [String: TimeInterval] = [:]
        for (canonicalId, chain) in chainOf {
            activity[canonicalId] = chain.map { stamp($0.updatedAt) }.max() ?? 0
        }

        /// One node and its subtree. Rule 5: children keep CREATION order, and
        /// the node's activity counts the subtree's.
        func build(_ id: String) -> SessionNode? {
            guard let chain = chainOf[id], let session = chain.last else { return nil }
            let children = (childrenOf[id] ?? [])
                .compactMap { build($0) }
                .sorted { a, b in
                    let (ca, cb) = (stamp(a.session.createdAt), stamp(b.session.createdAt))
                    return ca != cb ? ca < cb : a.session.id < b.session.id
                }
                .map { Node.session($0) }
            let own = activity[id] ?? 0
            return SessionNode(
                session: session,
                chain: chain,
                children: children,
                lastActivityAt: max(own, children.map(\.lastActivityAt).max() ?? 0)
            )
        }

        // The walk order is FIXED rather than the store's dictionary order: it
        // decides which top-level node claims a stack member when two of them
        // name the same issue, and two clients must claim the same one.
        //
        // The key is the chain's OLDEST row, not its newest: web, Rust and
        // Kotlin discover a node when they reach the first row of its
        // succession (walking every row oldest-first), so a resumed run holds
        // the place its FIRST row earned. Keying on the canonical newest row
        // would move it and break the tie-break ×4.
        let roots = rootIds
            .compactMap { build($0) }
            .sorted { a, b in
                let first = { (node: SessionNode) in node.chain.first ?? node.session }
                let (fa, fb) = (first(a), first(b))
                let (ca, cb) = (stamp(fa.createdAt), stamp(fb.createdAt))
                return ca != cb ? ca < cb : fa.id < fb.id
            }

        // Rules 3 then 4, over the TOP-LEVEL nodes only (a child run stays
        // under its parent wherever the parent lands).
        let grouped = groupStacks(groupWorkflows(roots, context: context), context: context)

        // Rule 5: groups and lone nodes sort by last activity, newest first.
        return grouped.sorted { a, b in
            a.lastActivityAt != b.lastActivityAt
                ? a.lastActivityAt > b.lastActivityAt
                : nodeKey(a) < nodeKey(b)
        }
    }

    /// The tree flattened top to bottom, skipping everything under a COLLAPSED
    /// node (keyed by `nodeKey`). A group row with no children left is dropped:
    /// a group IS its children.
    public static func visibleRows(
        _ nodes: [Node], collapsed: Set<String> = []
    ) -> [FlatRow] {
        var out: [FlatRow] = []
        func walk(_ list: [Node], _ depth: Int) {
            for node in list {
                if node.sessionNode == nil, node.children.isEmpty { continue }
                let key = nodeKey(node)
                out.append(
                    FlatRow(
                        node: node, key: key, depth: depth, hasChildren: !node.children.isEmpty
                    )
                )
                if !collapsed.contains(key) { walk(node.children, depth + 1) }
            }
        }
        walk(nodes, 0)
        return out
    }

    /// Every session node of the tree, depth-first, groups flattened.
    public static func flatten(_ nodes: [Node]) -> [SessionNode] {
        var out: [SessionNode] = []
        func walk(_ list: [Node]) {
            for node in list {
                if let entry = node.sessionNode { out.append(entry) }
                walk(node.children)
            }
        }
        walk(nodes)
        return out
    }

    // MARK: - Grouping

    /// Rule 3: the sessions of ONE workflow under one group row. A node belongs
    /// to the workflow that lists its issue (or the node run itself), and the
    /// name comes from `context.workflows` — a workflow the caller did not sync
    /// leaves its runs ungrouped.
    private static func groupWorkflows(
        _ roots: [SessionNode], context: Context
    ) -> [Node] {
        let workflows = Dictionary(
            context.workflows.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a }
        )
        guard !workflows.isEmpty, !context.workflowNodes.isEmpty else {
            return roots.map { Node.session($0) }
        }
        var byIssue: [String: String] = [:]
        var bySession: [String: String] = [:]
        for entry in context.workflowNodes where workflows[entry.workflowId] != nil {
            if !entry.issueId.isEmpty, byIssue[entry.issueId] == nil {
                byIssue[entry.issueId] = entry.workflowId
            }
            if let sessionId = entry.sessionId, !sessionId.isEmpty, bySession[sessionId] == nil {
                bySession[sessionId] = entry.workflowId
            }
        }
        func workflowOf(_ node: SessionNode) -> String? {
            for row in node.chain {
                if let named = bySession[row.id] { return named }
                if let issueId = row.issueId, let named = byIssue[issueId] { return named }
                // A batch node run covers several workflow issues (EXP-978).
                for issueId in BatchRun.issueIds(row.batchIssueIds) {
                    if let covered = byIssue[issueId] { return covered }
                }
            }
            return nil
        }

        var out: [Node] = []
        // Group ids in the order they first appear, so the collected children
        // can be folded back in at the group's own slot.
        var order: [String] = []
        var members: [String: [SessionNode]] = [:]
        var slot: [String: Int] = [:]
        for node in roots {
            guard let workflowId = workflowOf(node), let workflow = workflows[workflowId] else {
                out.append(.session(node))
                continue
            }
            if members[workflowId] == nil {
                members[workflowId] = []
                order.append(workflowId)
                slot[workflowId] = out.count
                out.append(
                    .workflow(
                        WorkflowGroup(
                            workflowId: workflowId, name: workflow.name, children: [],
                            lastActivityAt: 0
                        )
                    )
                )
            }
            members[workflowId]?.append(node)
        }
        for workflowId in order {
            guard let index = slot[workflowId], let group = members[workflowId],
                  let workflow = workflows[workflowId]
            else { continue }
            // The node runs, newest first.
            let children = group
                .map { Node.session($0) }
                .sorted { a, b in
                    a.lastActivityAt != b.lastActivityAt
                        ? a.lastActivityAt > b.lastActivityAt
                        : nodeKey(a) < nodeKey(b)
                }
            out[index] = .workflow(
                WorkflowGroup(
                    workflowId: workflowId,
                    name: workflow.name,
                    children: children,
                    lastActivityAt: children.map(\.lastActivityAt).max() ?? 0
                )
            )
        }
        return out
    }

    /// Rule 4: a stack (`issues.pr_base_branch`) under one group row in LINEAR
    /// order, lowest first. A stack with only ONE of its runs listed is no
    /// group — the lone node stays where it was.
    private static func groupStacks(_ entries: [Node], context: Context) -> [Node] {
        let issues = context.issues
        guard !issues.isEmpty else { return entries }
        let byIssueId = Dictionary(issues.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        // Every top-level session node that names an issue, by issue id.
        var nodeOfIssue: [String: SessionNode] = [:]
        for entry in entries {
            guard let node = entry.sessionNode, let issueId = node.session.issueId,
                  byIssueId[issueId] != nil, nodeOfIssue[issueId] == nil
            else { continue }
            nodeOfIssue[issueId] = node
        }

        var out: [Node] = []
        var claimed: Set<String> = []
        for entry in entries {
            guard let node = entry.sessionNode else {
                out.append(entry)
                continue
            }
            // A node already pulled into a group below is gone from the top
            // level; one that names no issue (a chat, an action, a batch) can
            // be in no stack and simply stays where it was.
            if claimed.contains(node.session.id) { continue }
            let issueId = node.session.issueId
            guard let issue = issueId.flatMap({ $0.isEmpty ? nil : byIssueId[$0] }) else {
                out.append(entry)
                continue
            }
            let chain = PrStack.stackChain(
                issue, in: issues, id: { $0.id }, branch: { $0.branch },
                base: { $0.prBaseBranch }
            )
            let members = chain.compactMap { member -> SessionNode? in
                guard let listed = nodeOfIssue[member.id],
                      !claimed.contains(listed.session.id)
                else { return nil }
                return listed
            }
            guard chain.count >= 2, members.count >= 2, let root = chain.first else {
                out.append(entry)
                continue
            }
            for member in members { claimed.insert(member.session.id) }
            out.append(
                .stack(
                    StackGroup(
                        rootIssueId: root.id,
                        children: members.map { Node.session($0) },
                        lastActivityAt: members.map(\.lastActivityAt).max() ?? 0
                    )
                )
            )
        }
        return out
    }

    // MARK: - Helpers

    /// `createdAt`/`updatedAt` as a comparable instant — an unparseable wire
    /// stamp reads as 0, exactly like web's `stamp()`, so the id tie-break
    /// decides.
    private static func stamp(_ value: String?) -> TimeInterval {
        guard let value, !value.isEmpty else { return 0 }
        return WireTimestamps.parse(value)?.timeIntervalSince1970 ?? 0
    }

    /// `id`'s direct parent, or nil when it is already a root. A cycle in the
    /// walk above it returns nil, so the row stays where it is.
    private static func ancestor(
        _ id: String, parentOf: [String: String], nodeIds: Set<String>
    ) -> String? {
        guard let parent = parentOf[id], nodeIds.contains(parent), parent != id else { return nil }
        var seen: Set<String> = [id]
        var cursor: String? = parent
        while let current = cursor {
            if seen.contains(current) { return nil }
            seen.insert(current)
            cursor = parentOf[current]
        }
        return parent
    }
}
