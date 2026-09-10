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

    func testBreaksACycleWhereItFirstAppears() {
        XCTAssertEqual(
            shape(nest([Row(id: "a", parent: "b"), Row(id: "b", parent: "a"), Row(id: "self", parent: "self")])),
            ["self@0", "a@0+", "b@1"]
        )
    }
}
