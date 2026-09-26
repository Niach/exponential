import Foundation
import XCTest
@testable import ExpCore

// EXP-1094: the Reviews row's ONE merge control, replayed from the shared
// fixture web, desktop and Android run too.
final class ReviewsMergeTests: XCTestCase {
    private struct Fixture: Decodable {
        struct Labels: Decodable {
            let merge: String
            let mergeStack: String
        }
        struct Reasons: Decodable {
            let mergesWithStack: String
            let mergesThroughWorkflow: String
        }
        struct Case: Decodable {
            let name: String
            let input: ReviewsMerge.Input
            let action: ReviewsMerge.Action
            let disabledReason: String?
        }
        let labels: Labels
        let reasons: Reasons
        let cases: [Case]
    }

    private func fixture() throws -> Fixture {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/reviews-merge.json")
        return try JSONDecoder().decode(Fixture.self, from: try Data(contentsOf: url))
    }

    func testLabelsAndReasonsMatchTheFixture() throws {
        let fixture = try fixture()
        XCTAssertEqual(ReviewsMerge.mergeLabel, fixture.labels.merge)
        XCTAssertEqual(ReviewsMerge.mergeStackLabel, fixture.labels.mergeStack)
        XCTAssertEqual(ReviewsMerge.mergesWithStack, fixture.reasons.mergesWithStack)
        XCTAssertEqual(ReviewsMerge.mergesThroughWorkflow, fixture.reasons.mergesThroughWorkflow)
    }

    func testContractFixtureCases() throws {
        let fixture = try fixture()
        XCTAssertFalse(fixture.cases.isEmpty)
        for testCase in fixture.cases {
            XCTAssertEqual(ReviewsMerge.reviewRowMergeAction(testCase.input), testCase.action, testCase.name)
            XCTAssertEqual(
                ReviewsMerge.reviewsMergeDisabledReason(testCase.input), testCase.disabledReason, testCase.name
            )
        }
    }

    func testALiveWorkflowWinsOverAFinishedOneOnTheSameIssue() {
        let byIssue = ReviewsMerge.workflowStatusByIssue(
            workflows: [(id: "w1", status: "done"), (id: "w2", status: "running")],
            nodes: [
                (workflowId: "w1", issueId: "i1", memberIssueIds: []),
                (workflowId: "w2", issueId: "i9", memberIssueIds: ["i1"]),
            ]
        )
        XCTAssertEqual(byIssue["i1"], "running")
        XCTAssertEqual(byIssue["i9"], "running")
        XCTAssertEqual(ReviewsMerge.reviewWorkflowStatus(issueIds: ["i0", "i1"], byIssue: byIssue), "running")
        XCTAssertNil(ReviewsMerge.reviewWorkflowStatus(issueIds: ["i0"], byIssue: byIssue))
    }
}
