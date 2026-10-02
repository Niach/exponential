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

    // MARK: - Team scope

    private func board(_ id: String, team: String) -> BoardEntity {
        BoardEntity(
            id: id, teamId: team, name: id, slug: id, prefix: "EXP", color: nil,
            sortOrder: nil, repositoryId: nil,
            createdAt: "2026-09-29T00:00:00Z", updatedAt: "2026-09-29T00:00:00Z"
        )
    }

    private func issue(
        _ id: String, board: String, identifier: String, base: String? = nil
    ) -> IssueEntity {
        IssueEntity(
            id: id, boardId: board, number: nil, identifier: identifier,
            title: identifier, description: nil, status: "in_review",
            priority: "none", assigneeId: nil, creatorId: nil, source: nil, dueDate: nil,
            sortOrder: nil, completedAt: nil, duplicateOfId: nil,
            prUrl: "https://github.com/acme/\(board)/pull/\(id)",
            prNumber: nil, prState: "open", branch: "exp/\(identifier)",
            prBaseBranch: base, prMergedAt: nil,
            createdAt: "2026-09-29T00:00:00Z", updatedAt: "2026-09-29T00:00:00Z"
        )
    }

    // Two teams file the same identifier, so both own a branch `exp/EXP-1`.
    // Team A's is a plain pull request; team B's carries a stack. The store
    // holds both, and the first row must never borrow the second one's chain.
    func testAnotherTeamsStackNeverJoinsAPlainPullRequest() {
        let boards = [board("board-a", team: "team-a"), board("board-b", team: "team-b")]
        let mine = issue("a-1", board: "board-a", identifier: "EXP-1")
        let theirs = issue("b-1", board: "board-b", identifier: "EXP-1")
        let theirTop = issue("b-2", board: "board-b", identifier: "EXP-2", base: "exp/EXP-1")
        let store = [theirs, theirTop, mine]

        // The defect: read off the whole store, team B's top stacks on mine.
        XCTAssertEqual(PrStack.stackMergeChoice(mine, issues: store)?.topIssueId, "b-2")

        let pool = PrStack.teamPool(of: mine, issues: store, boards: boards)
        XCTAssertEqual(pool.map(\.id), ["a-1"])
        XCTAssertNil(PrStack.stackMergeChoice(mine, issues: pool))

        // Team B still gets its own stack, and only its own rows.
        let theirPool = PrStack.teamPool(of: theirs, issues: store, boards: boards)
        XCTAssertEqual(theirPool.map(\.id), ["b-1", "b-2"])
        let choice = PrStack.stackMergeChoice(theirs, issues: theirPool)
        XCTAssertEqual(choice?.bottomIssueId, "b-1")
        XCTAssertEqual(choice?.topIssueId, "b-2")
    }

    // A team's stack may span its boards; a session subject names the team.
    func testTheTeamPoolSpansTheTeamsBoards() {
        let boards = [
            board("board-a", team: "team-a"), board("board-a2", team: "team-a"),
            board("board-b", team: "team-b"),
        ]
        let bottom = issue("a-1", board: "board-a", identifier: "EXP-1")
        let top = issue("a-2", board: "board-a2", identifier: "APP-2", base: "exp/EXP-1")
        let other = issue("b-1", board: "board-b", identifier: "EXP-1")
        let store = [other, bottom, top]

        XCTAssertEqual(
            PrStack.teamPool(of: top, issues: store, boards: boards).map(\.id), ["a-1", "a-2"]
        )
        XCTAssertEqual(
            PrStack.teamPool(store, teamId: "team-a", boards: boards).map(\.id), ["a-1", "a-2"]
        )
        XCTAssertEqual(PrStack.teamPool(store, teamId: "team-b", boards: boards).map(\.id), ["b-1"])
    }

    // A board that has not synced yet: the issue's own board alone.
    func testAnUnsyncedBoardKeepsTheIssuesOwnBoard() {
        let mine = issue("a-1", board: "board-a", identifier: "EXP-1")
        let theirTop = issue("b-2", board: "board-b", identifier: "EXP-2", base: "exp/EXP-1")
        let pool = PrStack.teamPool(of: mine, issues: [theirTop, mine], boards: [])
        XCTAssertEqual(pool.map(\.id), ["a-1"])
    }
}
