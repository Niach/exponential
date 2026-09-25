import Foundation
import XCTest
@testable import ExpCore

// EXP-1072: a workflow's final PR is named the same on every client —
// byte-identical to apps/web/src/lib/workflow-final-pr-identity.ts.
final class WorkflowFinalPrTests: XCTestCase {
    func testIdentifierNamesTheWorkflow() {
        XCTAssertEqual(WorkflowFinalPr.identifier(name: "EXP-996 +5"), "Workflow: EXP-996 +5")
    }

    func testPickLabelLeadsWithThePrNumberWhenKnown() {
        XCTAssertEqual(
            WorkflowFinalPr.pickLabel(number: 829, name: "EXP-996 +5"),
            "#829 · Workflow: EXP-996 +5"
        )
        XCTAssertEqual(
            WorkflowFinalPr.pickLabel(number: nil, name: "EXP-996 +5"),
            "Workflow: EXP-996 +5"
        )
    }

    func testReviewKeyIsNamespaced() {
        XCTAssertEqual(WorkflowFinalPr.reviewKey(workflowId: "wf-1"), "workflow:wf-1")
    }
}
