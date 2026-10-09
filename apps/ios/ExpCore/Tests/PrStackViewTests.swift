import Foundation
import XCTest
@testable import ExpCore

// EXP-1248: the tree/stack rule and the stack rail, replayed from
// `pr-stack-view.json` (web `pr-stack.test.ts`, desktop
// `stack_view_matches_the_fixture`, Android `PrStackViewTest`).
final class PrStackViewTests: XCTestCase {
    private struct Fixture: Decodable {
        struct Row: Decodable {
            let id: String
            let identifier: String
            let title: String
            let prNumber: Int?
            let branch: String?
            let prBaseBranch: String?
            let prState: String?
            let prUrl: String?
        }
        struct ViewRow: Decodable {
            let issueId: String
            let identifier: String
            let title: String
            let prNumber: Int?
            let isCurrent: Bool
        }
        struct View: Decodable {
            let rows: [ViewRow]
            let baseBranch: String?
        }
        struct Case: Decodable {
            let name: String
            let issue: String
            let issues: [Row]
            let shape: String
            let view: View?
        }
        let cases: [Case]
    }

    private func fixture() throws -> Fixture {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/pr-stack-view.json")
        return try JSONDecoder().decode(Fixture.self, from: try Data(contentsOf: url))
    }

    private func entity(_ row: Fixture.Row) -> IssueEntity {
        IssueEntity(
            id: row.id, boardId: "board", number: nil, identifier: row.identifier,
            title: row.title, description: nil, status: "in_review",
            priority: "none", assigneeId: nil, creatorId: nil, source: nil, dueDate: nil,
            sortOrder: nil, completedAt: nil, duplicateOfId: nil, prUrl: row.prUrl,
            prNumber: row.prNumber, prState: row.prState, branch: row.branch,
            prBaseBranch: row.prBaseBranch, prMergedAt: nil,
            createdAt: "2026-10-01T00:00:00Z", updatedAt: "2026-10-01T00:00:00Z"
        )
    }

    func testStackViewMatchesTheFixture() throws {
        let cases = try fixture().cases
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let issues = testCase.issues.map(entity)
            let issue = try XCTUnwrap(issues.first { $0.id == testCase.issue }, testCase.name)
            XCTAssertEqual(
                PrStack.openPrShape(issue, issues: issues).rawValue, testCase.shape, testCase.name
            )
            let expected = testCase.view.map { view in
                PrStack.StackView(
                    rows: view.rows.map {
                        PrStack.StackViewRow(
                            issueId: $0.issueId, identifier: $0.identifier, title: $0.title,
                            prNumber: $0.prNumber, isCurrent: $0.isCurrent
                        )
                    },
                    baseBranch: view.baseBranch
                )
            }
            XCTAssertEqual(PrStack.stackView(issue, issues: issues), expected, testCase.name)
        }
    }

    func testPrGraphShapeTellsAForkFromALine() {
        func row(_ id: String, base: String?) -> Fixture.Row {
            Fixture.Row(
                id: id, identifier: "EXP-\(id)", title: id, prNumber: nil,
                branch: "exp/\(id)", prBaseBranch: base, prState: "open", prUrl: nil
            )
        }
        let line = [row("1", base: "master"), row("2", base: "exp/1"), row("3", base: "exp/2")].map(entity)
        XCTAssertEqual(PrStack.prGraphShape(line), .stack)
        let fork = [row("1", base: "master"), row("2", base: "exp/1"), row("3", base: "exp/1")].map(entity)
        XCTAssertEqual(PrStack.prGraphShape(fork), .tree)
        XCTAssertEqual(PrStack.prGraphShape([line[0]]), .single)
        XCTAssertEqual(PrStack.prComponent(line[2], in: line + [row("9", base: "master")].map(entity)).map(\.id), ["1", "2", "3"])
    }
}
