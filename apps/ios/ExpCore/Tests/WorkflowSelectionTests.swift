import XCTest
@testable import ExpCore

// EXP-1084/1086: the workflow page's picker, locked ×4 against the contract
// fixture `workflow-view.json` `selection` (web `workflow-selection.ts`).
final class WorkflowSelectionTests: XCTestCase {
    private struct Fixture: Decodable {
        let selection: [SelectionCase]
    }

    private struct State: Decodable {
        let ids: [String]
        let anchor: String?
        let cursor: String?

        var selection: WorkflowSelection {
            WorkflowSelection(ids: ids, anchor: anchor, cursor: cursor)
        }
    }

    private struct Op: Decodable {
        let kind: String
        let id: String?
        let delta: Int?
    }

    private struct SelectionCase: Decodable {
        let name: String
        let order: [String]
        let current: State
        let op: Op
        let expected: State
    }

    private func fixture() throws -> Fixture {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/workflow-view.json")
        return try JSONDecoder().decode(Fixture.self, from: try Data(contentsOf: url))
    }

    func testSelectionFixtureCases() throws {
        let cases = try fixture().selection
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            var selection = testCase.current.selection
            switch testCase.op.kind {
            case "click": selection.click(testCase.op.id, in: testCase.order)
            case "toggle": selection.toggle(testCase.op.id ?? "", in: testCase.order)
            case "extend": selection.extend(to: testCase.op.id ?? "", in: testCase.order)
            case "step": selection.step(testCase.op.delta ?? 0, in: testCase.order)
            case "prune": selection.prune(with: testCase.order)
            default: XCTFail("unknown op \(testCase.op.kind)")
            }
            XCTAssertEqual(selection, testCase.expected.selection, testCase.name)
        }
    }

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

    // The page shows ONE node: the cursor, and its position in the stepper.
    func testThePageReadsTheCursorAsItsNode() {
        let order = WorkflowSelection.order(strip)
        var selection = WorkflowSelection()
        XCTAssertTrue(selection.isAll)
        XCTAssertNil(selection.nodeId)
        XCTAssertEqual(selection.position(in: order), 0)
        selection.click("b", in: order)
        XCTAssertEqual(selection.nodeId, "b")
        XCTAssertEqual(selection.position(in: order), 3)
        selection.click("b", in: order)
        XCTAssertEqual(selection.nodeId, "b")
        selection.step(1, in: order)
        XCTAssertEqual(selection.nodeId, "d")
        XCTAssertEqual(selection.position(in: order), 4)
    }
}
