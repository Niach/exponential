import Foundation
import XCTest
@testable import ExpCore

// EXP-1145: the stack merge dialog, replayed from the shared fixture web,
// desktop and Android run too.
final class StackMergeChoiceTests: XCTestCase {
    private struct Fixture: Decodable {
        struct Labels: Decodable {
            let title: String
            let mergeStack: String
            let mergeThis: String
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
        struct Choice: Decodable {
            let members: [String]
            let position: Int
            let bottomIssueId: String
            let topIssueId: String
            let listing: String
            let stackSentence: String
            let thisSentence: String
            let body: String
        }
        struct Case: Decodable {
            let name: String
            let issue: String
            let issues: [Row]
            let choice: Choice?
        }
        let labels: Labels
        let cases: [Case]
    }

    private func fixture() throws -> Fixture {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/stack-merge-choice.json")
        return try JSONDecoder().decode(Fixture.self, from: try Data(contentsOf: url))
    }

    private func entity(_ row: Fixture.Row) -> IssueEntity {
        IssueEntity(
            id: row.id, boardId: "board", number: nil, identifier: row.identifier,
            title: row.identifier ?? row.id, description: nil, status: "in_review",
            priority: "none", assigneeId: nil, creatorId: nil, source: nil, dueDate: nil,
            sortOrder: nil, completedAt: nil, duplicateOfId: nil, prUrl: row.prUrl,
            prNumber: nil, prState: row.prState, branch: row.branch,
            prBaseBranch: row.prBaseBranch, prMergedAt: nil,
            createdAt: "2026-09-29T00:00:00Z", updatedAt: "2026-09-29T00:00:00Z"
        )
    }

    func testLabelsMatchTheFixture() throws {
        let fixture = try fixture()
        XCTAssertEqual(PrStack.stackMergeChoiceTitle, fixture.labels.title)
        XCTAssertEqual(PrStack.mergeStackLabel, fixture.labels.mergeStack)
        XCTAssertEqual(PrStack.mergeThisPrLabel, fixture.labels.mergeThis)
        XCTAssertEqual(PrStack.stackMergeCancelLabel, fixture.labels.cancel)
    }

    func testEveryFixtureCaseMatches() throws {
        let fixture = try fixture()
        XCTAssertFalse(fixture.cases.isEmpty)
        for testCase in fixture.cases {
            let issues = testCase.issues.map(entity)
            guard let issue = issues.first(where: { $0.id == testCase.issue }) else {
                XCTFail("\(testCase.name): the merged issue is missing from its rows")
                continue
            }
            let expected = testCase.choice.map {
                PrStack.StackMergeChoice(
                    members: $0.members, position: $0.position, bottomIssueId: $0.bottomIssueId,
                    topIssueId: $0.topIssueId, listing: $0.listing,
                    stackSentence: $0.stackSentence, thisSentence: $0.thisSentence, body: $0.body
                )
            }
            XCTAssertEqual(PrStack.stackMergeChoice(issue, issues: issues), expected, testCase.name)
        }
    }
}
