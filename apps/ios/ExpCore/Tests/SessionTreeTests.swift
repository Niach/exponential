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

// EXP-996 — the NODE tree: `SessionTree.sessionTree`, `nodeKey`,
// `visibleRows`, `flatten`. Every case name mirrors an `it(...)` of web
// `lib/sessions/session-tree.test.ts`, which the desktop (`session_tree`) and
// Android (`SessionTreeNodeTest`) carry too.
final class SessionTreeNodeTests: XCTestCase {

    private func run(
        _ id: String,
        issueId: String? = nil,
        parent: String? = nil,
        resumedFrom: String? = nil,
        batchIssueIds: String? = nil,
        status: String = "running",
        at stamp: String = "2026-09-01T10:00:00Z"
    ) -> CodingSessionEntity {
        CodingSessionEntity(
            id: id,
            issueId: issueId,
            teamId: "team-1",
            userId: "user-1",
            deviceLabel: "macbook",
            status: status,
            batchIssueIds: batchIssueIds,
            resumedFromId: resumedFrom,
            parentSessionId: parent,
            startedAt: stamp,
            endedAt: nil,
            createdAt: stamp,
            updatedAt: stamp
        )
    }

    /// The tree as one string: `p[c1 c2]` — the Swift reading of web's nested
    /// `ids()` helper.
    private func shape(_ nodes: [SessionTree.SessionNode]) -> String {
        nodes.map { node in
            let label = node.session.id
            return node.children.isEmpty ? label : "\(label)[\(shape(node.children))]"
        }
        .joined(separator: " ")
    }

    private func seconds(_ iso: String) -> TimeInterval {
        WireTimestamps.parse(iso)?.timeIntervalSince1970 ?? 0
    }

    // MARK: - flatten

    func testWalksChildrenDepthFirst() {
        let leaf = SessionTree.SessionNode(
            session: run("b"), chain: [run("b")], children: [], lastActivityAt: 0
        )
        let nodes = [
            SessionTree.SessionNode(
                session: run("p"), chain: [run("p")], children: [leaf], lastActivityAt: 0
            ),
            SessionTree.SessionNode(
                session: run("a"), chain: [run("a")], children: [], lastActivityAt: 0
            ),
        ]
        XCTAssertEqual(SessionTree.flatten(nodes).map(\.session.id), ["p", "b", "a"])
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
        XCTAssertEqual(tree.first?.chain.map(\.id), ["r1", "r2"])
    }

    func testFollowsAChildToItsParentsResumeSuccession() {
        let p1 = run("p1")
        let p2 = run("p2", resumedFrom: "p1", at: "2026-09-01T11:00:00Z")
        let child = run("c", parent: "p1")
        XCTAssertEqual(shape(SessionTree.sessionTree([p1, p2, child])), "p2[c]")
    }

    func testCountsTheSubtreeInAParentsLastActivity() {
        let parent = run("p", at: "2026-09-01T10:00:00Z")
        let child = run("c", parent: "p", at: "2026-09-01T12:00:00Z")
        let lone = run("lone", at: "2026-09-01T11:00:00Z")
        let tree = SessionTree.sessionTree([lone, parent, child])
        XCTAssertEqual(tree.map(\.key), ["p", "lone"])
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

    // Issue and batch runs never group: only `parentSessionId` nests.
    func testNeverGroupsRunsByTheirIssues() {
        let a = run("a", issueId: "i1", at: "2026-09-01T10:00:00Z")
        let b = run("b", batchIssueIds: "[\"i1\",\"i2\"]", at: "2026-09-01T11:00:00Z")
        XCTAssertEqual(shape(SessionTree.sessionTree([a, b])), "b a")
    }

    // MARK: - visibleRows (EXP-996)

    // The flattening every client paints the EXP-965 connector over.
    private func nestedTree() -> [SessionTree.SessionNode] {
        let parent = run("p", at: "2026-09-01T11:00:00Z")
        let child = run("c", parent: "p", at: "2026-09-01T10:30:00Z")
        let grandchild = run("g", parent: "c", at: "2026-09-01T10:40:00Z")
        let other = run("o", at: "2026-09-01T10:00:00Z")
        return SessionTree.sessionTree([parent, child, grandchild, other])
    }

    func testFlattensChildrenWithTheirDepths() {
        XCTAssertEqual(
            SessionTree.visibleRows(nestedTree()).map { "\($0.key)@\($0.depth)" },
            ["p@0", "c@1", "g@2", "o@0"]
        )
    }

    func testHidesEverythingUnderACollapsedNode() {
        let rows = SessionTree.visibleRows(nestedTree(), collapsed: ["p"])
        XCTAssertEqual(rows.map(\.key), ["p", "o"])
        XCTAssertEqual(rows.first { $0.key == "p" }?.hasChildren, true)
    }

    func testKeysASessionByItsNewestRowsId() {
        XCTAssertEqual(nestedTree().first.map(SessionTree.nodeKey), "p")
    }

    // MARK: - EXP-1108: row marks, replayed from the shared fixture

    private struct MarksFixture: Decodable {
        struct NeedsYouCase: Decodable {
            let name: String
            let status: String
            let pendingQuestion: [String: String]?
            let needsYou: Bool
        }
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
