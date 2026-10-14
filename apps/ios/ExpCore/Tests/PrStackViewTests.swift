import Foundation
import XCTest
@testable import ExpCore

// EXP-1248/1306: the stack rail, replayed from `pr-stack-view.json` over
// `pull_requests` rows (web `pr-stack.test.ts`, desktop
// `stack_view_matches_the_fixture`, Android `PullRequestStack.stackView`).
// The fixture is issue-shaped: rows sharing a `prUrl` are ONE pull request
// closing all of them (desktop's `World::build`); a `tree` or `single` shape
// draws no rail.
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
            let prNumber: Int
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

    private struct World {
        var pulls: [PullRequestEntity] = []
        var issueIds: [String: [String]] = [:]
        var issues: [IssueEntity] = []

        func pullRequest(of issueId: String) -> PullRequestEntity? {
            pulls.first { issueIds[$0.id]?.contains(issueId) == true }
        }
    }

    private func world(_ rows: [Fixture.Row]) -> World {
        var world = World()
        for row in rows {
            world.issues.append(IssueEntity(
                id: row.id, boardId: "board", number: nil, identifier: row.identifier,
                title: row.title, description: nil, status: "in_review", priority: "none",
                assigneeId: nil, creatorId: nil, source: nil, dueDate: nil, sortOrder: nil,
                completedAt: nil, duplicateOfId: nil,
                createdAt: "2026-10-01T00:00:00Z", updatedAt: "2026-10-01T00:00:00Z"
            ))
            guard let url = row.prUrl else { continue }
            if !world.pulls.contains(where: { $0.id == url }) {
                world.pulls.append(PullRequestEntity(
                    id: url, teamId: "team", repositoryId: "repo", number: row.prNumber ?? 0, url: url,
                    head: row.branch ?? "", base: row.prBaseBranch ?? "",
                    state: row.prState ?? DomainContract.prStateOpen,
                    createdAt: "2026-10-01T00:00:00Z", updatedAt: "2026-10-01T00:00:00Z"
                ))
            }
            world.issueIds[url, default: []].append(row.id)
        }
        return world
    }

    func testStackViewMatchesTheFixture() throws {
        let cases = try fixture().cases
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let world = world(testCase.issues)
            let view = world.pullRequest(of: testCase.issue).flatMap {
                PullRequestStack.stackView($0, in: world.pulls, issueIds: world.issueIds, issues: world.issues)
            }
            XCTAssertEqual(view != nil, testCase.shape == "stack", testCase.name)
            let expected = try testCase.view.map { view in
                PullRequestStack.StackView(
                    rows: try view.rows.map { row in
                        PullRequestStack.StackViewRow(
                            pullRequestId: try XCTUnwrap(world.pullRequest(of: row.issueId)).id,
                            issueId: row.issueId, identifier: row.identifier, title: row.title,
                            prNumber: row.prNumber, rootSessionId: nil, isCurrent: row.isCurrent
                        )
                    },
                    baseBranch: view.baseBranch
                )
            }
            XCTAssertEqual(view, expected, testCase.name)
        }
    }

    func testAnIssueLessMemberReadsItsNumberAndTitle() throws {
        func pr(_ id: String, _ number: Int, head: String, base: String, title: String? = nil) -> PullRequestEntity {
            PullRequestEntity(
                id: id, teamId: "team", repositoryId: "repo", number: number, url: "", title: title,
                head: head, base: base, createdAt: "", updatedAt: ""
            )
        }
        let all = [pr("b", 7, head: "exp/b", base: "main", title: "Bottom"), pr("t", 8, head: "exp/t", base: "exp/b")]
        let view = try XCTUnwrap(PullRequestStack.stackView(all[0], in: all, issueIds: [:], issues: []))
        XCTAssertEqual(view.rows.map(\.identifier), ["#8", "#7"])
        XCTAssertEqual(view.rows.map(\.title), ["", "Bottom"])
        XCTAssertEqual(view.rows.map(\.isCurrent), [false, true])
        XCTAssertEqual(view.baseBranch, "main")
    }
}
