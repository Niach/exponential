import Foundation
import XCTest
@testable import ExpCore

// EXP-980: the list-nesting rule, locked ×4 (web `issue-nesting.test.ts`,
// Android `IssueNestingTest`, desktop `domain::issue_nesting`) against the ONE
// contract fixture — same cases, same test names.
final class IssueNestingTests: XCTestCase {
    private struct FixtureCase: Decodable {
        let name: String
        let groups: [[String]]
        let identifiers: [String: String]
        let relations: [FixtureRelation]
        let expected: [[FixtureRow]]
    }

    private struct FixtureRelation: Decodable {
        let type: String
        let issueId: String
        let relatedIssueId: String

        var relation: IssueRelationEntity {
            IssueRelationEntity(
                id: "\(type)-\(issueId)-\(relatedIssueId)",
                issueId: issueId,
                relatedIssueId: relatedIssueId,
                type: type,
                source: "user",
                teamId: "t1",
                boardId: "b1",
                createdAt: "2026-09-19T09:00:00Z",
                updatedAt: "2026-09-19T09:00:00Z"
            )
        }
    }

    private struct FixtureRow: Decodable, Equatable {
        let id: String
        let depth: Int
    }

    /// The committed contract fixture, read through `#filePath` because the
    /// unit-test bundle carries no repo resources.
    private func cases() throws -> [FixtureCase] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/issue-nesting.json")
        return try JSONDecoder().decode([FixtureCase].self, from: try Data(contentsOf: url))
    }

    func testContractFixtureCases() throws {
        let cases = try cases()
        XCTAssertFalse(cases.isEmpty, "the issue-nesting fixture must not be empty")
        for testCase in cases {
            let rows = IssueNesting.nestIssueRows(
                groups: testCase.groups,
                relations: testCase.relations.map(\.relation),
                identifierOf: { testCase.identifiers[$0] ?? $0 }
            )
            let actual = rows.map { group in
                group.map { FixtureRow(id: $0.id, depth: $0.depth) }
            }
            XCTAssertEqual(actual, testCase.expected, testCase.name)
        }
    }
}
