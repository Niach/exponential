import Foundation
import XCTest
@testable import ExpCore

// EXP-1244: the Reviews queue, replayed from the shared fixture web, desktop
// and Android run too.
final class ReviewsQueueTests: XCTestCase {
    private struct Fixture: Decodable {
        struct Labels: Decodable { let repoBandCaption: String; let runBandCaption: String }
        struct Team: Decodable { let id: String }
        struct Board: Decodable {
            let id: String
            let teamId: String
            let name: String
            let sortOrder: Double?
        }
        struct Issue: Decodable {
            let id: String
            let boardId: String
            let createdAt: String
            let prUrl: String?
            let prState: String?
        }
        struct Session: Decodable {
            let id: String
            let teamId: String
            let issueId: String?
            let prUrl: String?
            let prState: String?
            let createdAt: String
        }
        struct Pull: Decodable {
            let number: Int
            let url: String
        }
        struct PullRepo: Decodable {
            let teamId: String
            let repositoryId: String
            let fullName: String
            let pulls: [Pull]
        }
        struct Input: Decodable {
            let teams: [Team]
            let boards: [Board]
            let issues: [Issue]
            let sessions: [Session]
            let pulls: [PullRepo]
        }
        struct ExpectedEntry: Decodable, Equatable {
            let key: String
            let issueIds: [String]
        }
        struct ExpectedBoardGroup: Decodable, Equatable {
            let boardId: String
            let entries: [ExpectedEntry]
        }
        struct ExpectedRunGroup: Decodable, Equatable {
            let teamId: String
            let sessionIds: [String]
        }
        struct ExpectedRepoGroup: Decodable, Equatable {
            let teamId: String
            let repositoryId: String
            let pullNumbers: [Int]
        }
        struct Expected: Decodable {
            let boardGroups: [ExpectedBoardGroup]
            let runGroups: [ExpectedRunGroup]
            let repoGroups: [ExpectedRepoGroup]
            let count: Int
        }
        struct Case: Decodable {
            let name: String
            let input: Input
            let expected: Expected
        }
        let labels: Labels
        let cases: [Case]
        let navCases: [NavCase]
    }

    private struct NavCase: Decodable {
        struct Input: Decodable { let yolo: [Bool]; let count: Int }
        struct Expected: Decodable { let dot: Bool; let shows: Bool }
        let name: String
        let input: Input
        let expected: Expected
    }

    private func fixture() throws -> Fixture {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/reviews-queue.json")
        return try JSONDecoder().decode(Fixture.self, from: try Data(contentsOf: url))
    }

    private func board(_ row: Fixture.Board) -> BoardEntity {
        BoardEntity(
            id: row.id, teamId: row.teamId, name: row.name, slug: row.id, prefix: "EXP",
            color: nil, sortOrder: row.sortOrder, repositoryId: nil,
            createdAt: "2026-10-01T00:00:00Z", updatedAt: "2026-10-01T00:00:00Z"
        )
    }

    private func issue(_ row: Fixture.Issue) -> IssueEntity {
        IssueEntity(
            id: row.id, boardId: row.boardId, number: nil, identifier: row.id,
            title: row.id, description: nil, status: "in_review",
            priority: "none", assigneeId: nil, creatorId: nil, source: nil, dueDate: nil,
            sortOrder: nil, completedAt: nil, duplicateOfId: nil, prUrl: row.prUrl,
            prNumber: nil, prState: row.prState, branch: nil,
            prBaseBranch: nil, prMergedAt: nil,
            createdAt: row.createdAt, updatedAt: row.createdAt
        )
    }

    private func session(_ row: Fixture.Session) -> CodingSessionEntity {
        // startedAt deliberately disagrees with createdAt: the queue orders
        // runs by createdAt only.
        CodingSessionEntity(
            id: row.id, issueId: row.issueId, teamId: row.teamId, userId: "u1",
            deviceLabel: nil, status: "in_review",
            startedAt: "2020-01-01T00:00:00Z", endedAt: nil,
            createdAt: row.createdAt, updatedAt: row.createdAt,
            prUrl: row.prUrl, prNumber: nil, prState: row.prState
        )
    }

    func testLabelsMatchTheFixture() throws {
        XCTAssertEqual(ReviewsQueue.repoBandCaption, try fixture().labels.repoBandCaption)
        XCTAssertEqual(ReviewsQueue.runBandCaption, try fixture().labels.runBandCaption)
    }

    func testEveryFixtureCaseMatches() throws {
        let fixture = try fixture()
        XCTAssertFalse(fixture.cases.isEmpty)
        for testCase in fixture.cases {
            let input = testCase.input
            let result = ReviewsQueue.build(
                teamIds: input.teams.map(\.id),
                boards: input.boards.map(board),
                issues: input.issues.map(issue),
                sessions: input.sessions.map(session),
                pulls: input.pulls.map { repo in
                    ReviewsQueue.PullRepo(
                        teamId: repo.teamId, repositoryId: repo.repositoryId,
                        fullName: repo.fullName,
                        pulls: repo.pulls.map { OpenPull(number: $0.number, url: $0.url) }
                    )
                }
            )
            XCTAssertEqual(
                result.boardGroups.map { group in
                    Fixture.ExpectedBoardGroup(
                        boardId: group.board.id,
                        entries: group.entries.map {
                            Fixture.ExpectedEntry(key: $0.key, issueIds: $0.issues.map(\.id))
                        }
                    )
                },
                testCase.expected.boardGroups, testCase.name
            )
            XCTAssertEqual(
                result.runGroups.map {
                    Fixture.ExpectedRunGroup(teamId: $0.teamId, sessionIds: $0.sessions.map(\.id))
                },
                testCase.expected.runGroups, testCase.name
            )
            XCTAssertEqual(
                result.repoGroups.map {
                    Fixture.ExpectedRepoGroup(
                        teamId: $0.teamId, repositoryId: $0.repositoryId,
                        pullNumbers: $0.pulls.map(\.number)
                    )
                },
                testCase.expected.repoGroups, testCase.name
            )
            XCTAssertEqual(result.count, testCase.expected.count, testCase.name)
        }
    }

    func testEveryNavCaseMatches() throws {
        let cases = try fixture().navCases
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            XCTAssertEqual(
                ReviewsQueue.nav(yolo: testCase.input.yolo, count: testCase.input.count),
                ReviewsQueue.Nav(dot: testCase.expected.dot, shows: testCase.expected.shows),
                testCase.name
            )
        }
    }

    func testPostgresAndIsoTimestampsCompareAsInstants() {
        let board = board(.init(id: "b1", teamId: "t1", name: "Web", sortOrder: 1))
        let older = issue(.init(
            id: "a", boardId: "b1", createdAt: "2026-10-01T10:00:00Z",
            prUrl: "https://github.com/acme/web/pull/1", prState: "open"
        ))
        // Postgres text, one hour LATER as an instant.
        let newer = issue(.init(
            id: "b", boardId: "b1", createdAt: "2026-10-01 11:00:00.5+00",
            prUrl: "https://github.com/acme/web/pull/2", prState: "open"
        ))
        let result = ReviewsQueue.build(
            teamIds: ["t1"], boards: [board], issues: [older, newer], sessions: [], pulls: []
        )
        XCTAssertEqual(result.boardGroups.first?.entries.map(\.key), [
            "https://github.com/acme/web/pull/2", "https://github.com/acme/web/pull/1",
        ])
    }
}
