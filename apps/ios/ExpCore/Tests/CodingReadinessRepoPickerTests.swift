import Foundation
import XCTest
@testable import ExpCore

// EXP-1121: the checklist's repository picker order + tags (Android's
// `CodingReadinessRepoPickerTest` pins the same rule).
final class CodingReadinessRepoPickerTests: XCTestCase {
    private typealias Picker = CodingReadinessRepoPicker

    private let repos: [Picker.Repo] = [
        Picker.Repo(id: "r1", fullName: "acme/api", boards: [Picker.BoardRef(id: "b2", name: "Backend")]),
        Picker.Repo(id: "r2", fullName: "acme/Web", boards: []),
        Picker.Repo(id: "r3", fullName: "acme/docs", boards: [Picker.BoardRef(id: "b1", name: "Web")]),
        Picker.Repo(id: "r4", fullName: "other/web-app", boards: []),
        Picker.Repo(id: "r5", fullName: "acme/frontend", boards: []),
    ]

    func testMatchesLeadThenOriginalOrder() {
        let rows = Picker.rows(repos: repos, boardId: "b1", boardName: "Web", boardSlug: "web", query: "")
        XCTAssertEqual(rows.map(\.id), ["r2", "r1", "r3", "r4", "r5"])
        XCTAssertEqual(rows[0].tag, CodingReadiness.Copy.pickerMatchesBoard)
        XCTAssertTrue(rows[0].matchesBoard)
    }

    func testUsedByNamesAnotherBoardNeverTheCurrentOne() {
        let rows = Picker.rows(repos: repos, boardId: "b1", boardName: "Web", boardSlug: "web", query: "")
        let byId = Dictionary(uniqueKeysWithValues: rows.map { ($0.id, $0) })
        XCTAssertEqual(byId["r1"]?.tag, "used by Backend")
        // r3 is only used by the picking board itself.
        XCTAssertNil(byId["r3"]?.tag)
        XCTAssertNil(byId["r4"]?.tag)
    }

    func testSlugMatchesToo() {
        let rows = Picker.rows(
            repos: repos, boardId: "b9", boardName: "Frontend team", boardSlug: "frontend", query: ""
        )
        XCTAssertEqual(rows.first?.id, "r5")
        XCTAssertEqual(rows.first?.tag, "matches board")
    }

    func testSearchFiltersByFullNameSubstringCaseInsensitive() {
        let rows = Picker.rows(repos: repos, boardId: "b1", boardName: "Web", boardSlug: "web", query: " WEB ")
        XCTAssertEqual(rows.map(\.id), ["r2", "r4"])
        let owner = Picker.rows(repos: repos, boardId: "b1", boardName: "Web", boardSlug: "web", query: "other/")
        XCTAssertEqual(owner.map(\.id), ["r4"])
    }

    func testEmpty() {
        XCTAssertEqual(Picker.rows(repos: [], boardId: "b1", boardName: "Web", boardSlug: "web", query: ""), [])
    }

    // MARK: - The shared contract fixture (`picker`, ×4 with web + desktop + Android)

    private struct PickerFixture: Decodable {
        struct BoardJSON: Decodable { let id: String; let name: String; let slug: String }
        struct RepoBoardJSON: Decodable { let id: String; let name: String }
        struct RepoJSON: Decodable { let id: String; let fullName: String; let boards: [RepoBoardJSON] }
        struct RowJSON: Decodable { let id: String; let tag: String? }
        struct Case: Decodable {
            let name: String
            let board: BoardJSON
            let repos: [RepoJSON]
            let query: String
            let expected: [RowJSON]
        }
        let picker: [Case]
    }

    func testMatchesTheSharedFixture() throws {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/coding-readiness.json")
        let fixture = try JSONDecoder().decode(PickerFixture.self, from: try Data(contentsOf: url))
        XCTAssertFalse(fixture.picker.isEmpty)
        for fixtureCase in fixture.picker {
            let repos = fixtureCase.repos.map { repo in
                Picker.Repo(
                    id: repo.id,
                    fullName: repo.fullName,
                    boards: repo.boards.map { Picker.BoardRef(id: $0.id, name: $0.name) }
                )
            }
            let rows = Picker.rows(
                repos: repos,
                boardId: fixtureCase.board.id,
                boardName: fixtureCase.board.name,
                boardSlug: fixtureCase.board.slug,
                query: fixtureCase.query
            )
            XCTAssertEqual(rows.map(\.id), fixtureCase.expected.map(\.id), fixtureCase.name)
            XCTAssertEqual(rows.map(\.tag), fixtureCase.expected.map(\.tag), fixtureCase.name)
        }
    }
}
