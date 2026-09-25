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
            status: "running",
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

    // MARK: - flatten

    func testWalksGroupsAndChildrenDepthFirstSessionsOnly() {
        let leaf = SessionTree.SessionNode(
            session: run("b"), chain: [run("b")], children: [], lastActivityAt: 0
        )
        let nodes: [SessionTree.Node] = [
            .workflow(
                SessionTree.WorkflowGroup(
                    workflowId: "w", name: "W", children: [.session(leaf)], lastActivityAt: 0
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

    func testGroupsTheSessionsOfOneWorkflowUnderAWorkflowNode() {
        let n1 = run("n1", issueId: "i1", startedReason: "workflow")
        let n2 = run(
            "n2", issueId: "i2", startedReason: "workflow", at: "2026-09-01T11:00:00Z"
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
        let n1 = run("n1", issueId: "i1", at: "2026-09-01T10:00:00Z")
        let n2 = run("n2", issueId: "i2", at: "2026-09-01T12:00:00Z")
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

    // A batch node run covers several workflow issues (EXP-978), and it is
    // `batch_issue_ids` — not an `issue_id` it hasn't got — that names them.
    func testGroupsABatchNodeRunByTheIssuesItCovers() {
        let batch = run("b", batchIssueIds: "[\"i2\",\"i1\"]")
        let node = run("n1", issueId: "i1", at: "2026-09-01T11:00:00Z")
        let tree = SessionTree.sessionTree(
            [batch, node],
            context: SessionTree.Context(
                workflows: [.init(id: "w", name: "W", status: "running")],
                workflowNodes: [.init(workflowId: "w", issueId: "i1")]
            )
        )
        XCTAssertEqual(shape(tree), "workflow:w[n1 b]")
    }

    // EXP-996: `startedReason == "workflow"` alone never groups — the group
    // row's NAME comes from the workflows the caller synced.
    func testLeavesAWorkflowRunUngroupedWhenTheWorkflowIsNotListed() {
        let n1 = run("n1", issueId: "i1", startedReason: "workflow")
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
        let parent = run("p", issueId: "i1", at: "2026-09-01T11:00:00Z")
        let child = run("c", parent: "p", at: "2026-09-01T10:30:00Z")
        let other = run("n2", issueId: "i2", at: "2026-09-01T10:00:00Z")
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

    // MARK: - EXP-1082 workflow contract: grouping by the session's own
    // workflow membership (`workflow_id`/`workflow_node_id`/`workflow_role`).
    // Pending until EXP-1068 implements the rules (web
    // `lib/sessions/session-tree.test.ts` carries the same names).

    func testGroupsByWorkflowIdBeforeAnyHeuristic() throws {
        throw XCTSkip("EXP-1068")
    }

    func testNestsAReviewRunUnderItsNodesAuthorRow() throws {
        throw XCTSkip("EXP-1068")
    }

    func testKeepsASwitchedReviewerUnderItsNode() throws {
        throw XCTSkip("EXP-1068")
    }

    func testListsABaseMergeAsAChildOfTheGroup() throws {
        throw XCTSkip("EXP-1068")
    }

    func testNamesAPlanOnlyGroupAfterThePlan() throws {
        throw XCTSkip("EXP-1068")
    }

    func testKeepsAForeignChatThatResumedAWorkflowRunInsideTheGroup() throws {
        throw XCTSkip("EXP-1068")
    }

    func testGroupsAPersonsRunOnACompoundNodesSubIssue() throws {
        throw XCTSkip("EXP-1068")
    }

    func testFlagsANodeWithTwoLiveAuthorRuns() throws {
        throw XCTSkip("EXP-1068")
    }
}
