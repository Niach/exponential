import Foundation
import XCTest
@testable import ExpCore

// EXP-818 — the session tree's four rules, the same four tests web
// (`session-tree.test.ts`), Android (`SessionTreeTest`) and the desktop
// (`nest_sessions_*`) run.
final class SessionTreeTests: XCTestCase {
    private struct Row {
        let id: String
        let parent: String?
        var startedAt = "2026-09-10T10:00:00Z"
    }

    private func nest(_ rows: [Row]) -> [SessionTree.Row<Row>] {
        SessionTree.nest(rows, id: { $0.id }, parent: { $0.parent }, startedAt: { $0.startedAt })
    }

    private func shape(_ rows: [SessionTree.Row<Row>]) -> [String] {
        rows.map { "\($0.session.id)@\($0.depth)\($0.hasChildren ? "+" : "")" }
    }

    func testKeepsTheCallersRootOrder() {
        XCTAssertEqual(shape(nest([Row(id: "b", parent: nil), Row(id: "a", parent: nil), Row(id: "c", parent: nil)])), ["b@0", "a@0", "c@0"])
    }

    func testNestsAChildOnlyUnderAParentThatIsListed() {
        XCTAssertEqual(
            shape(nest([Row(id: "p", parent: nil), Row(id: "c", parent: "p"), Row(id: "orphan", parent: "gone")])),
            ["p@0+", "c@1", "orphan@0"]
        )
    }

    func testListsChildrenRightAfterTheirParentOldestFirstRecursively() {
        let rows = nest([
            Row(id: "late", parent: "p", startedAt: "2026-09-10T12:00:00Z"),
            Row(id: "p", parent: nil),
            Row(id: "grand", parent: "early", startedAt: "2026-09-10T13:00:00Z"),
            Row(id: "early", parent: "p", startedAt: "2026-09-10T11:00:00Z"),
            Row(id: "z", parent: nil),
        ])
        XCTAssertEqual(shape(rows), ["p@0+", "early@1+", "grand@2", "late@1", "z@0"])
        XCTAssertEqual(SessionTree.descendantIds(rows, of: "p", rowId: { $0.id }), ["early", "grand", "late"])
        XCTAssertEqual(SessionTree.descendantIds(rows, of: "early", rowId: { $0.id }), ["grand"])
        XCTAssertEqual(SessionTree.descendantIds(rows, of: "z", rowId: { $0.id }), [])
    }

    // EXP-897: a folded list hides a collapsed parent's WHOLE subtree, however
    // deep, and only a row with children can collapse.
    func testHidesRowsUnderACollapsedParent() {
        let rows = nest([
            Row(id: "p", parent: nil),
            Row(id: "child", parent: "p"),
            Row(id: "grand", parent: "child"),
            Row(id: "sibling", parent: nil),
        ])
        XCTAssertEqual(shape(rows), ["p@0+", "child@1+", "grand@2", "sibling@0"])
        XCTAssertEqual(
            visible(rows, ["p"]).map(\.session.id), ["p", "sibling"]
        )
        XCTAssertEqual(
            visible(rows, ["child"]).map(\.session.id), ["p", "child", "sibling"]
        )
        // A leaf carries no fold, so naming it changes nothing.
        XCTAssertEqual(
            visible(rows, ["grand", "sibling"]).map(\.session.id),
            ["p", "child", "grand", "sibling"]
        )
        XCTAssertEqual(visible(rows, []).map(\.session.id), ["p", "child", "grand", "sibling"])
    }

    private func visible(
        _ rows: [SessionTree.Row<Row>], _ collapsed: Set<String>
    ) -> [SessionTree.Row<Row>] {
        SessionTree.visibleRows(rows, collapsed: collapsed, rowId: { $0.id })
    }

    func testBreaksACycleWhereItFirstAppears() {
        XCTAssertEqual(
            shape(nest([Row(id: "a", parent: "b"), Row(id: "b", parent: "a"), Row(id: "self", parent: "self")])),
            ["self@0", "a@0+", "b@1"]
        )
    }

    // Children sort by the PARSED instant (web `startStamp`), never the raw
    // string: `.5Z` sorts before `Z` as text but after it in time, a Postgres
    // `+00` text form sorts before every ISO `T` form as text whatever its
    // instant, and an unparseable stamp reads as 0 so the id decides.
    func testSortsChildrenByTheParsedInstantNotTheRawString() {
        let rows = nest([
            Row(id: "p", parent: nil),
            Row(id: "half", parent: "p", startedAt: "2026-09-10T10:00:00.5Z"),
            Row(id: "whole", parent: "p", startedAt: "2026-09-10T10:00:00Z"),
            Row(id: "pg", parent: "p", startedAt: "2026-09-10 10:00:02+00"),
            Row(id: "iso", parent: "p", startedAt: "2026-09-10T10:00:01Z"),
        ])
        XCTAssertEqual(shape(rows), ["p@0+", "whole@1", "half@1", "iso@1", "pg@1"])
        let unparseable = nest([
            Row(id: "p", parent: nil),
            Row(id: "b", parent: "p", startedAt: "2026-09-10T10:00:00Z"),
            Row(id: "z", parent: "p", startedAt: "not a stamp"),
            Row(id: "a", parent: "p", startedAt: ""),
        ])
        XCTAssertEqual(shape(unparseable), ["p@0+", "a@1", "z@1", "b@1"])
    }
}

// EXP-996 (contract EXP-1029) — the NODE tree: `SessionTree.sessionTree`,
// `nodeKey`, `visibleRows`, `flatten`. Every case name mirrors an `it(...)` of
// web `lib/sessions/session-tree.test.ts`, which the desktop
// (`session_tree`) and Android (`SessionTreeNodeTest`) carry too.
final class SessionTreeNodeTests: XCTestCase {

    private func run(
        _ id: String,
        issueId: String? = nil,
        parent: String? = nil,
        resumedFrom: String? = nil,
        batchIssueIds: String? = nil,
        startedReason: String? = nil,
        status: String = "running",
        branch: String? = nil,
        workflowId: String? = nil,
        workflowNodeId: String? = nil,
        workflowRole: String? = nil,
        at stamp: String = "2026-09-01T10:00:00Z"
    ) -> CodingSessionEntity {
        CodingSessionEntity(
            id: id,
            issueId: issueId,
            teamId: "team-1",
            userId: "user-1",
            deviceLabel: "macbook",
            status: status,
            branch: branch,
            batchIssueIds: batchIssueIds,
            startedReason: startedReason,
            resumedFromId: resumedFrom,
            parentSessionId: parent,
            workflowId: workflowId,
            workflowNodeId: workflowNodeId,
            workflowRole: workflowRole,
            startedAt: stamp,
            endedAt: nil,
            createdAt: stamp,
            updatedAt: stamp
        )
    }

    /// EXP-1082 §1: a run stamped into workflow `w` (web's `member()`).
    private func member(
        _ id: String,
        _ nodeId: String?,
        role: String? = "author",
        issueId: String? = nil,
        parent: String? = nil,
        resumedFrom: String? = nil,
        status: String = "running",
        branch: String? = nil,
        at stamp: String = "2026-09-01T10:00:00Z"
    ) -> CodingSessionEntity {
        run(
            id, issueId: issueId, parent: parent, resumedFrom: resumedFrom, status: status,
            branch: branch, workflowId: "w", workflowNodeId: nodeId, workflowRole: role, at: stamp
        )
    }

    private func issue(_ id: String, _ branch: String?, _ base: String?) -> SessionTree.Context.StackIssue {
        SessionTree.Context.StackIssue(id: id, branch: branch, prBaseBranch: base)
    }

    /// The tree as one string: `workflow:w[n2 n1]` — the Swift reading of web's
    /// nested `ids()` helper.
    private func shape(_ nodes: [SessionTree.Node]) -> String {
        nodes.map { node in
            let label: String
            switch node {
            case let .session(entry): label = entry.session.id
            case let .workflow(group): label = "workflow:\(group.workflowId)"
            case let .stack(group): label = "stack:\(group.rootIssueId)"
            }
            return node.children.isEmpty ? label : "\(label)[\(shape(node.children))]"
        }
        .joined(separator: " ")
    }

    private func seconds(_ iso: String) -> TimeInterval {
        WireTimestamps.parse(iso)?.timeIntervalSince1970 ?? 0
    }

    private let listed = SessionTree.Context(
        workflows: [.init(id: "w", name: "Checkout rewrite", status: "running")]
    )

    private func group(_ tree: [SessionTree.Node]) -> SessionTree.WorkflowGroup? {
        if case let .workflow(group)? = tree.first { return group }
        return nil
    }

    private func sessionAt(_ tree: [SessionTree.Node], _ path: [Int]) -> SessionTree.SessionNode? {
        guard let first = path.first, tree.indices.contains(first) else { return nil }
        var cursor = tree[first]
        for index in path.dropFirst() {
            guard cursor.children.indices.contains(index) else { return nil }
            cursor = cursor.children[index]
        }
        return cursor.sessionNode
    }

    // MARK: - flatten

    func testWalksGroupsAndChildrenDepthFirstSessionsOnly() {
        let leaf = SessionTree.SessionNode(
            session: run("b"), chain: [run("b")], children: [], lastActivityAt: 0
        )
        let nodes: [SessionTree.Node] = [
            .workflow(
                SessionTree.WorkflowGroup(
                    workflowId: "w", name: "W", status: "running", liveRuns: 1,
                    children: [.session(leaf)], lastActivityAt: 0
                )
            ),
            .session(
                SessionTree.SessionNode(
                    session: run("a"), chain: [run("a")], children: [], lastActivityAt: 0
                )
            ),
        ]
        XCTAssertEqual(SessionTree.flatten(nodes).map(\.session.id), ["b", "a"])
    }

    // MARK: - sessionTree

    func testListsUnrelatedSessionsAtTopLevelNewestActivityFirst() {
        let a = run("a", at: "2026-09-01T10:00:00Z")
        let b = run("b", at: "2026-09-01T11:00:00Z")
        XCTAssertEqual(shape(SessionTree.sessionTree([a, b])), "b a")
    }

    func testNestsAChildUnderItsParentSessionId() {
        let parent = run("p")
        let child = run("c", parent: "p", at: "2026-09-01T10:30:00Z")
        XCTAssertEqual(shape(SessionTree.sessionTree([child, parent])), "p[c]")
    }

    func testCollapsesAResumeSuccessionIntoOneNodeKeyedByItsNewestRow() {
        let first = run("r1", at: "2026-09-01T10:00:00Z")
        let second = run("r2", resumedFrom: "r1", at: "2026-09-01T12:00:00Z")
        let tree = SessionTree.sessionTree([first, second])
        XCTAssertEqual(shape(tree), "r2")
        XCTAssertEqual(tree.first?.sessionNode?.chain.map(\.id), ["r1", "r2"])
    }

    func testFollowsAChildToItsParentsResumeSuccession() {
        let p1 = run("p1")
        let p2 = run("p2", resumedFrom: "p1", at: "2026-09-01T11:00:00Z")
        let child = run("c", parent: "p1")
        XCTAssertEqual(shape(SessionTree.sessionTree([p1, p2, child])), "p2[c]")
    }

    // EXP-1068: the rows' own `workflowId` groups them; the `workflow_nodes`
    // rows only feed the caption.
    func testGroupsTheSessionsOfOneWorkflowUnderAWorkflowNode() {
        let n1 = run(
            "n1", issueId: "i1", startedReason: "workflow",
            workflowId: "w", workflowNodeId: "n1", workflowRole: "author"
        )
        let n2 = run(
            "n2", issueId: "i2", startedReason: "workflow",
            workflowId: "w", workflowNodeId: "n2", workflowRole: "author",
            at: "2026-09-01T11:00:00Z"
        )
        let tree = SessionTree.sessionTree(
            [n1, n2],
            context: SessionTree.Context(
                workflows: [.init(id: "w", name: "EXP-996 +5", status: "running")],
                workflowNodes: [
                    .init(workflowId: "w", issueId: "i1"),
                    .init(workflowId: "w", issueId: "i2"),
                ]
            )
        )
        XCTAssertEqual(shape(tree), "workflow:w[n2 n1]")
    }

    func testGroupsAStackUnderItsLowestIssueInLinearOrder() {
        let low = run("s-low", issueId: "i-low")
        let mid = run("s-mid", issueId: "i-mid", at: "2026-09-01T11:00:00Z")
        let top = run("s-top", issueId: "i-top", at: "2026-09-01T12:00:00Z")
        let tree = SessionTree.sessionTree(
            [top, low, mid],
            context: SessionTree.Context(
                issues: [
                    issue("i-low", "exp/APP-1", nil),
                    issue("i-mid", "exp/APP-2", "exp/APP-1"),
                    issue("i-top", "exp/APP-3", "exp/APP-2"),
                ]
            )
        )
        XCTAssertEqual(shape(tree), "stack:i-low[s-low s-mid s-top]")
    }

    // A run that names no issue (a chat, an action, a batch) is in no stack —
    // and is neither swallowed by the group beside it nor lost.
    func testKeepsAnIssueLessRunBesideAStackGroup() {
        let chat = run("chat", at: "2026-09-01T10:00:00Z")
        let low = run("s-low", issueId: "i-low", at: "2026-09-01T11:00:00Z")
        let top = run("s-top", issueId: "i-top", at: "2026-09-01T12:00:00Z")
        let tree = SessionTree.sessionTree(
            [chat, low, top],
            context: SessionTree.Context(
                issues: [
                    issue("i-low", "exp/APP-1", nil),
                    issue("i-top", "exp/APP-2", "exp/APP-1"),
                ]
            )
        )
        XCTAssertEqual(shape(tree), "stack:i-low[s-low s-top] chat")
    }

    func testSortsGroupsByTheirLastActivityAmongTheTopLevelNodes() {
        let lone = run("lone", at: "2026-09-01T11:30:00Z")
        let n1 = member("n1", "n1", issueId: "i1", at: "2026-09-01T10:00:00Z")
        let n2 = member("n2", "n2", issueId: "i2", at: "2026-09-01T12:00:00Z")
        let tree = SessionTree.sessionTree(
            [lone, n1, n2],
            context: SessionTree.Context(
                workflows: [.init(id: "w", name: "W", status: "running")],
                workflowNodes: [
                    .init(workflowId: "w", issueId: "i1"),
                    .init(workflowId: "w", issueId: "i2"),
                ]
            )
        )
        XCTAssertEqual(tree.map(\.key), ["workflow:w", "lone"])
        XCTAssertEqual(tree.first?.lastActivityAt, seconds("2026-09-01T12:00:00Z"))
    }

    func testPutsAnOrphanChildWhoseParentIsGoneAtTopLevel() {
        XCTAssertEqual(shape(SessionTree.sessionTree([run("o", parent: "gone")])), "o")
    }

    func testKeepsChildrenInCreationOrderUnderTheirParent() {
        let parent = run("p")
        let c1 = run("c1", parent: "p", at: "2026-09-01T10:10:00Z")
        let c2 = run("c2", parent: "p", at: "2026-09-01T10:20:00Z")
        XCTAssertEqual(shape(SessionTree.sessionTree([c2, parent, c1])), "p[c1 c2]")
    }

    // A batch node run is stamped `author` of its compound node like any other
    // node run (EXP-1082): the stamp groups it, never its `batch_issue_ids`.
    func testGroupsABatchNodeRunByTheIssuesItCovers() {
        let batch = run(
            "b", batchIssueIds: "[\"i2\",\"i1\"]",
            workflowId: "w", workflowNodeId: "n2", workflowRole: "author"
        )
        let node = member("n1", "n1", issueId: "i1", at: "2026-09-01T11:00:00Z")
        let unstamped = run("x", batchIssueIds: "[\"i1\"]", at: "2026-09-01T09:00:00Z")
        let tree = SessionTree.sessionTree(
            [batch, node, unstamped],
            context: SessionTree.Context(
                workflows: [.init(id: "w", name: "W", status: "running")],
                workflowNodes: [.init(workflowId: "w", issueId: "i1")]
            )
        )
        XCTAssertEqual(shape(tree), "workflow:w[n1 b] x")
    }

    // EXP-996: `startedReason == "workflow"` alone never groups — the group
    // row's NAME comes from the workflows the caller synced.
    func testLeavesAWorkflowRunUngroupedWhenTheWorkflowIsNotListed() {
        let n1 = run(
            "n1", issueId: "i1", startedReason: "workflow",
            workflowId: "w", workflowNodeId: "n1", workflowRole: "author"
        )
        let tree = SessionTree.sessionTree(
            [n1],
            context: SessionTree.Context(
                workflowNodes: [.init(workflowId: "w", issueId: "i1")]
            )
        )
        XCTAssertEqual(shape(tree), "n1")
    }

    // MARK: - visibleRows (EXP-996)

    // The flattening every client paints the EXP-965 connector over.
    private func workflowTree() -> [SessionTree.Node] {
        let parent = member("p", "n1", issueId: "i1", at: "2026-09-01T11:00:00Z")
        let child = run("c", parent: "p", at: "2026-09-01T10:30:00Z")
        let other = member("n2", "n2", issueId: "i2", at: "2026-09-01T10:00:00Z")
        return SessionTree.sessionTree(
            [parent, child, other],
            context: SessionTree.Context(
                workflows: [.init(id: "w", name: "W", status: "running")],
                workflowNodes: [
                    .init(workflowId: "w", issueId: "i1"),
                    .init(workflowId: "w", issueId: "i2"),
                ]
            )
        )
    }

    func testFlattensGroupsAndChildrenWithTheirDepths() {
        XCTAssertEqual(
            SessionTree.visibleRows(workflowTree()).map { "\($0.key)@\($0.depth)" },
            ["workflow:w@0", "p@1", "c@2", "n2@1"]
        )
    }

    func testHidesEverythingUnderACollapsedNode() {
        let rows = SessionTree.visibleRows(workflowTree(), collapsed: ["p"])
        XCTAssertEqual(rows.map(\.key), ["workflow:w", "p", "n2"])
        XCTAssertEqual(rows.first { $0.key == "p" }?.hasChildren, true)
    }

    func testFoldsAWholeGroupAway() {
        XCTAssertEqual(
            SessionTree.visibleRows(workflowTree(), collapsed: ["workflow:w"]).map(\.key),
            ["workflow:w"]
        )
    }

    func testKeysASessionByItsIdAndAGroupByItsKind() {
        XCTAssertEqual(workflowTree().first.map(SessionTree.nodeKey), "workflow:w")
        XCTAssertEqual(SessionTree.visibleRows(workflowTree())[1].key, "p")
        XCTAssertEqual(
            SessionTree.nodeKey(
                .stack(
                    SessionTree.StackGroup(rootIssueId: "i-low", children: [], lastActivityAt: 0)
                )
            ),
            "stack:i-low"
        )
    }

    // A group is its children: a group row with none left is not drawn at all.
    func testDropsAChildlessGroupRow() {
        let nodes: [SessionTree.Node] = [
            .workflow(
                SessionTree.WorkflowGroup(
                    workflowId: "w", name: "W", children: [], lastActivityAt: 0
                )
            )
        ]
        XCTAssertEqual(SessionTree.visibleRows(nodes).count, 0)
    }

    // MARK: - Workflow membership (EXP-1082 → EXP-1068)

    // No `workflow_nodes` row names the run or its issue: the row's own
    // `workflowId` is what folds it under the group.
    func testGroupsByWorkflowIdBeforeAnyHeuristic() {
        let a = member("a", "n1", issueId: "i9", at: "2026-09-01T11:00:00Z")
        let x = run("x", issueId: "i-other")
        XCTAssertEqual(shape(SessionTree.sessionTree([a, x], context: listed)), "workflow:w[a] x")
    }

    // The heuristics are gone: a `workflow_nodes` row naming the issue (or
    // the session) does not group a row that carries no `workflowId`.
    func testNeverGroupsAnUnstampedRowWhateverWorkflowNodesSay() {
        let n1 = run("n1", issueId: "i1", startedReason: "workflow")
        let tree = SessionTree.sessionTree(
            [n1],
            context: SessionTree.Context(
                workflows: listed.workflows,
                workflowNodes: [.init(workflowId: "w", issueId: "i1", sessionId: "n1")]
            )
        )
        XCTAssertEqual(shape(tree), "n1")
    }

    func testLeavesAStampedRunUngroupedWhenTheWorkflowIsNotListed() {
        let a = member("a", "n1", issueId: "i1")
        XCTAssertEqual(shape(SessionTree.sessionTree([a], context: SessionTree.Context())), "a")
    }

    func testNestsAReviewRunUnderItsNodesAuthorRow() {
        let a = member("a", "n1", issueId: "i1", at: "2026-09-01T10:00:00Z")
        let r = member(
            "r", "n1", role: "review", branch: "exp/wf-2f353e88-review-EXP-1068-r1",
            at: "2026-09-01T11:00:00Z"
        )
        let b = member("b", "n2", issueId: "i2", at: "2026-09-01T09:00:00Z")
        let tree = SessionTree.sessionTree([a, r, b], context: listed)
        XCTAssertEqual(shape(tree), "workflow:w[a[r] b]")
        XCTAssertEqual(sessionAt(tree, [0, 0, 0])?.reviewRound, 1)
        XCTAssertNil(sessionAt(tree, [0, 0])?.reviewRound)
        XCTAssertEqual(sessionAt(tree, [0, 0])?.duplicateLive, false)
    }

    // An account-switch resume of the reviewer (a new row, same membership)
    // collapses into the same chain under n1's author.
    func testKeepsASwitchedReviewerUnderItsNode() {
        let a = member("a", "n1", issueId: "i1", at: "2026-09-01T10:00:00Z")
        let r1 = member(
            "r1", "n1", role: "review", status: "ended",
            branch: "exp/wf-2f353e88-review-EXP-1068-r2", at: "2026-09-01T11:00:00Z"
        )
        let r2 = member(
            "r2", "n1", role: "review", resumedFrom: "r1",
            branch: "exp/wf-2f353e88-review-EXP-1068-r2", at: "2026-09-01T12:00:00Z"
        )
        let tree = SessionTree.sessionTree([a, r1, r2], context: listed)
        XCTAssertEqual(shape(tree), "workflow:w[a[r2]]")
        let reviewer = sessionAt(tree, [0, 0, 0])
        XCTAssertEqual(reviewer?.chain.map(\.id), ["r1", "r2"])
        XCTAssertEqual(reviewer?.reviewRound, 2)
        // ONE live reviewer: no duplicate flag on the node.
        XCTAssertEqual(sessionAt(tree, [0, 0])?.duplicateLive, false)
    }

    func testNestsAReviewUnderTheLiveAuthorNotAnEndedOne() {
        let dead = member("a0", "n1", issueId: "i1", status: "ended", at: "2026-09-01T12:00:00Z")
        let live = member("a1", "n1", issueId: "i1", at: "2026-09-01T10:00:00Z")
        let r = member("r", "n1", role: "review", at: "2026-09-01T11:00:00Z")
        XCTAssertEqual(
            shape(SessionTree.sessionTree([dead, live, r], context: listed)),
            "workflow:w[a0 a1[r]]"
        )
    }

    func testListsAReviewWhoseNodeHasNoAuthorAsAChildOfTheGroup() {
        let r = member("r", "n1", role: "review")
        XCTAssertEqual(shape(SessionTree.sessionTree([r], context: listed)), "workflow:w[r]")
    }

    // `base_merge`, `plan` and `replan` name no node: each is a plain child of
    // the group, never under a node's author run. Newest activity first.
    func testListsABaseMergeAsAChildOfTheGroup() {
        let a = member("a", "n1", issueId: "i1", at: "2026-09-01T10:00:00Z")
        let m = member("m", nil, role: "base_merge", at: "2026-09-01T11:00:00Z")
        let p = member("p", nil, role: "plan", at: "2026-09-01T08:00:00Z")
        let rp = member("rp", nil, role: "replan", at: "2026-09-01T12:00:00Z")
        XCTAssertEqual(
            shape(SessionTree.sessionTree([a, m, p, rp], context: listed)),
            "workflow:w[rp m a p]"
        )
    }

    // A draft whose only row is its `plan` run still draws a group, named
    // after the plan's working name (the workflow row's name).
    func testNamesAPlanOnlyGroupAfterThePlan() {
        let tree = SessionTree.sessionTree(
            [member("p", nil, role: "plan")],
            context: SessionTree.Context(
                workflows: [.init(id: "w", name: "Checkout rewrite", status: "draft")]
            )
        )
        XCTAssertEqual(shape(tree), "workflow:w[p]")
        XCTAssertEqual(group(tree)?.name, "Checkout rewrite")
        XCTAssertEqual(group(tree)?.status, "draft")
    }

    // A resume performed from a chat names the chat as its parent but keeps
    // the workflow membership (EXP-906), so it stays in the group.
    func testKeepsAForeignChatThatResumedAWorkflowRunInsideTheGroup() {
        let c = run("c", at: "2026-09-01T11:00:00Z")
        let a = member("a", "n1", issueId: "i1", at: "2026-09-01T10:00:00Z")
        let a2 = member(
            "a2", "n1", issueId: "i1", parent: "c", resumedFrom: "a", at: "2026-09-01T12:00:00Z"
        )
        XCTAssertEqual(
            shape(SessionTree.sessionTree([c, a, a2], context: listed)), "workflow:w[a2] c"
        )
    }

    // A `sessions_start` child inherits the membership and nests under its
    // parent; a child with NO workflow of its own nests too.
    func testNestsAChildOfANodeRunUnderItInsideTheGroup() {
        let a = member("a", "n1", issueId: "i1", at: "2026-09-01T10:00:00Z")
        let kid = member("kid", "n1", role: nil, parent: "a", at: "2026-09-01T10:30:00Z")
        let plain = run("plain", parent: "a", at: "2026-09-01T10:40:00Z")
        XCTAssertEqual(
            shape(SessionTree.sessionTree([a, kid, plain], context: listed)),
            "workflow:w[a[kid plain]]"
        )
    }

    // Compound node n1 = parent i1 + sub-issue i2; only the parent has a
    // `workflow_nodes` row. A person's run on i2, stamped `author` of n1 by
    // the server (EXP-1062), joins the node's group.
    func testGroupsAPersonsRunOnACompoundNodesSubIssue() {
        let mine = member("mine", "n1", issueId: "i2", at: "2026-09-01T11:00:00Z")
        let b = member("b", "n2", issueId: "i3", at: "2026-09-01T10:00:00Z")
        let tree = SessionTree.sessionTree(
            [mine, b],
            context: SessionTree.Context(
                workflows: listed.workflows,
                workflowNodes: [.init(id: "n1", workflowId: "w", issueId: "i1", state: "running")]
            )
        )
        XCTAssertEqual(shape(tree), "workflow:w[mine b]")
    }

    // Two live `author` rows on one node (a double start): BOTH are listed,
    // neither nested under the other, and both carry the warning.
    func testFlagsANodeWithTwoLiveAuthorRuns() {
        let a1 = member("a1", "n1", issueId: "i1", at: "2026-09-01T10:00:00Z")
        let a2 = member("a2", "n1", issueId: "i1", at: "2026-09-01T11:00:00Z")
        let tree = SessionTree.sessionTree([a1, a2], context: listed)
        XCTAssertEqual(shape(tree), "workflow:w[a2 a1]")
        XCTAssertEqual(sessionAt(tree, [0, 0])?.duplicateLive, true)
        XCTAssertEqual(sessionAt(tree, [0, 1])?.duplicateLive, true)
    }

    func testFlagsANodeWithTwoLiveReviewersOnItsAuthorRow() {
        let a = member("a", "n1", issueId: "i1", at: "2026-09-01T10:00:00Z")
        let r1 = member("r1", "n1", role: "review", at: "2026-09-01T11:00:00Z")
        let r2 = member("r2", "n1", role: "review", at: "2026-09-01T11:30:00Z")
        let tree = SessionTree.sessionTree([a, r1, r2], context: listed)
        XCTAssertEqual(shape(tree), "workflow:w[a[r1 r2]]")
        XCTAssertEqual(sessionAt(tree, [0, 0])?.duplicateLive, true)
        XCTAssertEqual(sessionAt(tree, [0, 0, 0])?.duplicateLive, false)
    }

    func testDoesNotFlagANodeWhoseSecondAuthorRunHasEnded() {
        let a1 = member("a1", "n1", issueId: "i1", status: "ended", at: "2026-09-01T10:00:00Z")
        let a2 = member("a2", "n1", issueId: "i1", at: "2026-09-01T11:00:00Z")
        let tree = SessionTree.sessionTree([a1, a2], context: listed)
        XCTAssertEqual(sessionAt(tree, [0, 0])?.duplicateLive, false)
    }

    func testCountsTheGroupsLiveRunsAndLandedNodes() {
        let a = member("a", "n1", issueId: "i1", at: "2026-09-01T10:00:00Z")
        let r = member("r", "n1", role: "review", at: "2026-09-01T11:00:00Z")
        let done = member("d", "n2", issueId: "i2", status: "ended", at: "2026-09-01T09:00:00Z")
        let tree = SessionTree.sessionTree(
            [a, r, done],
            context: SessionTree.Context(
                workflows: listed.workflows,
                workflowNodes: [
                    .init(id: "n1", workflowId: "w", issueId: "i1", state: "running"),
                    .init(id: "n2", workflowId: "w", issueId: "i2", state: "landed"),
                    .init(id: "n3", workflowId: "w", issueId: "i3", state: "blocked"),
                    .init(id: "other", workflowId: "w2", issueId: "i4", state: "landed"),
                ]
            )
        )
        let node = group(tree)
        XCTAssertEqual(node?.liveRuns, 2)
        XCTAssertEqual(node?.nodesDone, 1)
        XCTAssertEqual(node?.nodesTotal, 3)
        XCTAssertEqual(
            node.map {
                SessionTree.workflowGroupCaption(
                    liveRuns: $0.liveRuns, nodesDone: $0.nodesDone, nodesTotal: $0.nodesTotal
                )
            },
            "2 running · 1 of 3 done"
        )
    }

    func testStillGroupsAStackBesideAWorkflowFromTheLeftoverTopLevel() {
        let a = member("a", "n1", issueId: "i1")
        let low = run("s-low", issueId: "i-low")
        let top = run("s-top", issueId: "i-top", at: "2026-09-01T11:00:00Z")
        let tree = SessionTree.sessionTree(
            [a, low, top],
            context: SessionTree.Context(
                workflows: listed.workflows,
                issues: [
                    issue("i-low", "exp/APP-1", nil),
                    issue("i-top", "exp/APP-2", "exp/APP-1"),
                ]
            )
        )
        XCTAssertEqual(shape(tree), "stack:i-low[s-low s-top] workflow:w[a]")
    }

    // MARK: - The strings (EXP-1068, byte-identical ×4)

    private func caption(_ live: Int, _ done: Int, _ total: Int) -> String {
        SessionTree.workflowGroupCaption(liveRuns: live, nodesDone: done, nodesTotal: total)
    }

    func testSaysRunningAndDone() {
        XCTAssertEqual(caption(3, 5, 8), "3 running · 5 of 8 done")
    }

    func testDropsTheRunningPartWithNothingLive() {
        XCTAssertEqual(caption(0, 5, 8), "5 of 8 done")
    }

    func testDropsTheDonePartBeforeTheNodesSynced() {
        XCTAssertEqual(caption(1, 0, 0), "1 running")
    }

    func testIsEmptyWithNeither() {
        XCTAssertEqual(caption(0, 0, 0), "")
    }

    func testReadsTheRoundOffAReviewBranch() {
        XCTAssertEqual(SessionTree.reviewBranchRound("exp/wf-2f353e88-review-EXP-1068-r3"), 3)
        XCTAssertEqual(SessionTree.reviewBranchRound("exp/wf-2f353e88-review-EXP-10-r12"), 12)
    }

    func testIsNilForEveryOtherBranch() {
        XCTAssertNil(SessionTree.reviewBranchRound("exp/EXP-1068"))
        XCTAssertNil(SessionTree.reviewBranchRound("exp/wf-2f353e88-review-EXP-1068-r"))
        XCTAssertNil(SessionTree.reviewBranchRound("exp/wf-2f353e88-review-EXP-1068-r0"))
        XCTAssertNil(SessionTree.reviewBranchRound(nil))
        XCTAssertNil(SessionTree.reviewBranchRound(""))
    }

    /// web's `node = { reviewRound: 2, review: { round: 2, verdict } }`.
    private func verdict(
        _ round: Int?, node: Int? = 2, latest: Int? = 2, _ word: String? = "request_changes"
    ) -> SessionTree.ReviewRowVerdict {
        SessionTree.reviewRoundVerdict(
            round: round, nodeReviewRound: node, latestRound: latest, latestVerdict: word
        )
    }

    func testReadsTheLatestVerdictForItsRound() {
        XCTAssertEqual(verdict(2), .changesRequested)
        XCTAssertEqual(verdict(2, "approve"), .approved)
    }

    func testCallsAnOlderSubmittedRoundSubmitted() {
        XCTAssertEqual(verdict(1), .submitted)
    }

    func testHasNoVerdictForThePendingRoundOrWithoutANode() {
        XCTAssertEqual(verdict(3), .none)
        XCTAssertEqual(verdict(nil), .none)
        XCTAssertEqual(verdict(1, node: nil, latest: nil, nil), .none)
        XCTAssertEqual(verdict(1, node: 0, latest: nil, nil), .none)
    }

    func testNamesTheRoundAndTheVerdict() {
        XCTAssertEqual(
            SessionTree.reviewRowCaption(round: 2, verdict: .approved, live: false),
            "Review r2 · approved"
        )
        XCTAssertEqual(
            SessionTree.reviewRowCaption(round: 2, verdict: .changesRequested, live: false),
            "Review r2 · changes requested"
        )
        XCTAssertEqual(
            SessionTree.reviewRowCaption(round: 1, verdict: .submitted, live: false),
            "Review r1 · submitted"
        )
    }

    func testSaysNoVerdictOnlyOnceTheRunEnded() {
        XCTAssertEqual(SessionTree.reviewRowCaption(round: 3, verdict: .none, live: true), "Review r3")
        XCTAssertEqual(
            SessionTree.reviewRowCaption(round: 3, verdict: .none, live: false),
            "Review r3 · no verdict"
        )
    }

    func testFallsBackToABareReviewWithoutARound() {
        XCTAssertEqual(SessionTree.reviewRowCaption(round: nil, verdict: .none, live: true), "Review")
        XCTAssertEqual(
            SessionTree.reviewRowCaption(round: nil, verdict: .none, live: false),
            "Review · no verdict"
        )
    }

    // MARK: - EXP-1108: row marks, replayed from the shared fixture

    private struct MarksFixture: Decodable {
        struct AccountCase: Decodable {
            let name: String
            let session: SessionTree.MarkSession
            let devices: [SessionTree.MarkDevice]
            let caption: String?
        }
        struct NeedsYouCase: Decodable {
            let name: String
            let status: String
            let pendingQuestion: [String: String]?
            let needsYou: Bool
        }
        let accountCaptions: [AccountCase]
        let needsYou: [NeedsYouCase]
    }

    private func marksFixture() throws -> MarksFixture {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/session-tree-marks.json")
        return try JSONDecoder().decode(MarksFixture.self, from: try Data(contentsOf: url))
    }

    func testAccountCaptionFixtureCases() throws {
        let fixture = try marksFixture()
        XCTAssertFalse(fixture.accountCaptions.isEmpty)
        for testCase in fixture.accountCaptions {
            XCTAssertEqual(
                SessionTree.workflowRunAccountCaption(session: testCase.session, devices: testCase.devices),
                testCase.caption,
                testCase.name
            )
        }
    }

    /// The rule prefers the row whose `userId` is the session owner's over an
    /// earlier row with the same device id. The `devices` shape stamps an
    /// owner ONLY on a teammate's shared row, so the UI adapter names the
    /// caller's own rows with the signed-in user (`RunningSessionRowMarks`):
    /// a nil `userId` there can never win and falls to the first match.
    func testOwnRowNamedWithTheSignedInUserWinsOverATeammatesSharedCopy() {
        let session = SessionTree.MarkSession(
            agent: "claude", agentAccount: "p2", deviceId: "d1", userId: "u1", workflowId: "w1"
        )
        let teammateCopy = SessionTree.MarkDevice(
            deviceId: "d1",
            userId: "u2",
            launchDefaults: SessionTree.MarkLaunchDefaults(defaultAgent: "claude", defaultAccount: "p2"),
            agentAccounts: nil
        )
        func ownRow(userId: String?) -> SessionTree.MarkDevice {
            SessionTree.MarkDevice(
                deviceId: "d1",
                userId: userId,
                launchDefaults: SessionTree.MarkLaunchDefaults(defaultAgent: "claude", defaultAccount: "system"),
                agentAccounts: [
                    "claude": SessionTree.MarkAgentAccount(profiles: [
                        SessionTree.MarkProfile(id: "system", label: "Default", active: true),
                        SessionTree.MarkProfile(id: "p2", label: "Work", active: false),
                    ]),
                ]
            )
        }
        // Stamped with the signed-in user, the own row wins even listed last.
        XCTAssertEqual(
            SessionTree.workflowRunAccountCaption(
                session: session, devices: [teammateCopy, ownRow(userId: "u1")]
            ),
            "account Work"
        )
        // Left nil, the teammate's copy is read instead and its defaults
        // hide the caption: the reason the adapter fills the id in.
        XCTAssertNil(
            SessionTree.workflowRunAccountCaption(
                session: session, devices: [teammateCopy, ownRow(userId: nil)]
            )
        )
    }

    func testNeedsYouFixtureCases() throws {
        let fixture = try marksFixture()
        XCTAssertFalse(fixture.needsYou.isEmpty)
        for testCase in fixture.needsYou {
            XCTAssertEqual(
                SessionTree.sessionNeedsYou(
                    status: testCase.status, hasPendingQuestion: testCase.pendingQuestion != nil
                ),
                testCase.needsYou,
                testCase.name
            )
        }
    }
}
