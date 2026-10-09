import Foundation
import XCTest
@testable import ExpCore

// EXP-1248: the ONE stack merge confirm, replayed from
// `stack-merge-choice.json` `confirm` (web `stackMergeConfirm`, desktop
// `stack_merge_confirm_matches_the_fixture`, Android `StackMergeConfirmTest`).
final class StackMergeConfirmTests: XCTestCase {
    private struct Fixture: Decodable {
        struct Labels: Decodable {
            let mergeStack: String
            let mergeThrough: String
            let cancel: String
        }
        struct Row: Decodable {
            let id: String
            let identifier: String?
            let branch: String?
            let prBaseBranch: String?
            let prState: String?
            let prUrl: String?
        }
        struct Input: Decodable {
            let issueId: String
            let mergeStack: Bool
        }
        struct Confirm: Decodable {
            let title: String
            let landing: [String]
            let staysOpen: [String]
            let body: String
            let input: Input
        }
        struct Case: Decodable {
            let name: String
            let issue: String
            let mode: String
            let issues: [Row]
            let confirm: Confirm?
        }
        struct Section: Decodable {
            let labels: Labels
            let cases: [Case]
        }
        let confirm: Section
    }

    private func fixture() throws -> Fixture.Section {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/stack-merge-choice.json")
        return try JSONDecoder().decode(Fixture.self, from: try Data(contentsOf: url)).confirm
    }

    private func entity(_ row: Fixture.Row) -> IssueEntity {
        IssueEntity(
            id: row.id, boardId: "board", number: nil, identifier: row.identifier,
            title: row.identifier ?? row.id, description: nil, status: "in_review",
            priority: "none", assigneeId: nil, creatorId: nil, source: nil, dueDate: nil,
            sortOrder: nil, completedAt: nil, duplicateOfId: nil, prUrl: row.prUrl,
            prNumber: nil, prState: row.prState, branch: row.branch,
            prBaseBranch: row.prBaseBranch, prMergedAt: nil,
            createdAt: "2026-10-01T00:00:00Z", updatedAt: "2026-10-01T00:00:00Z"
        )
    }

    func testLabelsMatchTheFixture() throws {
        let labels = try fixture().labels
        XCTAssertEqual(DomainContract.diffUiMergeStack, labels.mergeStack)
        XCTAssertEqual(PrStack.mergeThroughLabel, labels.mergeThrough)
        XCTAssertEqual(PrStack.stackConfirmCancelLabel, labels.cancel)
    }

    func testStackMergeConfirmMatchesTheFixture() throws {
        let cases = try fixture().cases
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let issues = testCase.issues.map(entity)
            let issue = try XCTUnwrap(issues.first { $0.id == testCase.issue }, testCase.name)
            let mode = try XCTUnwrap(PrStack.StackConfirmMode(rawValue: testCase.mode), testCase.name)
            let expected = testCase.confirm.map {
                PrStack.StackMergeConfirm(
                    title: $0.title, landing: $0.landing, staysOpen: $0.staysOpen,
                    body: $0.body, issueId: $0.input.issueId
                )
            }
            XCTAssertEqual(
                PrStack.stackMergeConfirm(issue, issues: issues, mode: mode), expected, testCase.name
            )
            if let confirm = testCase.confirm { XCTAssertTrue(confirm.input.mergeStack, testCase.name) }
        }
    }
}
