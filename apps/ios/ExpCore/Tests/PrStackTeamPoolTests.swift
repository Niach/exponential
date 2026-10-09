import Foundation
import XCTest
@testable import ExpCore

// `PrStack.teamPool`: a stack never spans teams (two teams may own the same
// `exp/EXP-1` branch name), so every stack read runs on the team's rows.
final class PrStackTeamPoolTests: XCTestCase {
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
        XCTAssertEqual(PrStack.stackMergeConfirm(mine, issues: store, mode: .stack)?.issueId, "b-2")

        let pool = PrStack.teamPool(of: mine, issues: store, boards: boards)
        XCTAssertEqual(pool.map(\.id), ["a-1"])
        XCTAssertNil(PrStack.stackMergeConfirm(mine, issues: pool, mode: .stack))

        // Team B still gets its own stack, and only its own rows.
        let theirPool = PrStack.teamPool(of: theirs, issues: store, boards: boards)
        XCTAssertEqual(theirPool.map(\.id), ["b-1", "b-2"])
        XCTAssertEqual(
            PrStack.stackView(theirs, issues: theirPool)?.rows.map(\.issueId), ["b-2", "b-1"]
        )
        XCTAssertEqual(PrStack.stackMergeConfirm(theirs, issues: theirPool, mode: .stack)?.issueId, "b-2")
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
