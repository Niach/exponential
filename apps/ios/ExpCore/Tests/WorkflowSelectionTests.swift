import XCTest
@testable import ExpCore

// EXP-1086: the workflow page's chip selection — All first, one node at a
// time, stepping in the strip's DAG order.
final class WorkflowSelectionTests: XCTestCase {
    private let strip = WorkflowView.nodeStrip(
        nodes: [
            StripNodeInput(id: "d", identifier: "EXP-4", state: "blocked", wave: 2, lane: 0),
            StripNodeInput(id: "b", identifier: "EXP-3", state: "running", wave: 1, lane: 1),
            StripNodeInput(id: "a", identifier: "EXP-2", state: "running", wave: 1, lane: 0),
            StripNodeInput(id: "c", identifier: "EXP-1", state: "landed", wave: 0, lane: 0),
        ],
        edges: []
    )

    func testTheOrderIsTheStripsDagOrder() {
        XCTAssertEqual(WorkflowSelection.order(strip), ["c", "a", "b", "d"])
    }

    func testItStartsOnAll() {
        let selection = WorkflowSelection()
        XCTAssertTrue(selection.isAll)
        XCTAssertEqual(selection.position(in: WorkflowSelection.order(strip)), 0)
    }

    func testSelectPicksOneNodeAndNilIsAll() {
        var selection = WorkflowSelection()
        selection.select("a")
        XCTAssertEqual(selection.nodeId, "a")
        selection.select("b")
        XCTAssertEqual(selection.nodeId, "b")
        selection.select(nil)
        XCTAssertTrue(selection.isAll)
    }

    func testTappingTheSelectedChipGoesBackToAll() {
        var selection = WorkflowSelection()
        selection.toggle("a")
        XCTAssertEqual(selection.nodeId, "a")
        selection.toggle("b")
        XCTAssertEqual(selection.nodeId, "b")
        selection.toggle("b")
        XCTAssertNil(selection.nodeId)
    }

    func testSteppingWalksTheDagWithAllAtZeroAndClamps() {
        let order = WorkflowSelection.order(strip)
        var selection = WorkflowSelection()
        selection.step(1, in: order)
        XCTAssertEqual(selection.nodeId, "c")
        XCTAssertEqual(selection.position(in: order), 1)
        selection.step(2, in: order)
        XCTAssertEqual(selection.nodeId, "b")
        selection.step(10, in: order)
        XCTAssertEqual(selection.nodeId, "d")
        selection.step(-1, in: order)
        XCTAssertEqual(selection.nodeId, "b")
        selection.step(-10, in: order)
        XCTAssertTrue(selection.isAll)
        selection.step(-1, in: order)
        XCTAssertTrue(selection.isAll)
        // An empty strip has only All.
        selection.step(1, in: [])
        XCTAssertTrue(selection.isAll)
    }

    func testANodeThatLeftTheWorkflowReadsAsAll() {
        var selection = WorkflowSelection(nodeId: "gone")
        let order = WorkflowSelection.order(strip)
        XCTAssertEqual(selection.position(in: order), 0)
        selection.step(1, in: order)
        XCTAssertEqual(selection.nodeId, "c")

        selection = WorkflowSelection(nodeId: "gone")
        selection.reconcile(with: order)
        XCTAssertTrue(selection.isAll)
        selection.select("a")
        selection.reconcile(with: order)
        XCTAssertEqual(selection.nodeId, "a")
    }
}
