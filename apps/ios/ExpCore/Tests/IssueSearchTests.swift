import Foundation
import XCTest
@testable import ExpCore

// EXP-892: the issue-search engine, locked ×4 (web `issue-search.test.ts`,
// Android `IssueSearchTest`, desktop `domain::issue_search`) against the ONE
// contract fixture — same cases, same test names.
final class IssueSearchTests: XCTestCase {
    private struct FixtureCase: Decodable {
        let name: String
        let rows: [FixtureRow]
        let query: String
        let exclude: [String]?
        let limit: Int?
        let serverHits: [FixtureHit]?
        let allowUnsynced: Bool?
        let expected: [String]
    }

    private struct FixtureRow: Decodable {
        let id: String
        let identifier: String
        let title: String
        let description: String?
        let createdAt: String?
        let updatedAt: String?
        let status: String?

        var row: IssueSearch.Row {
            IssueSearch.Row(
                id: id,
                identifier: identifier,
                title: title,
                description: description,
                createdAt: createdAt,
                updatedAt: updatedAt,
                status: status
            )
        }
    }

    /// One relevance-ordered server hit — the slim projection `issues.search`
    /// returns, as the fixture spells it.
    private struct FixtureHit: Decodable {
        let id: String
        let identifier: String
        let title: String
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
            .appendingPathComponent("packages/domain-contract/fixtures/issue-search.json")
        return try JSONDecoder().decode([FixtureCase].self, from: try Data(contentsOf: url))
    }

    func testContractFixtureCases() throws {
        let cases = try cases()
        XCTAssertFalse(cases.isEmpty, "the issue-search fixture must not be empty")
        for testCase in cases {
            let rows = testCase.rows.map(\.row)
            let limit = testCase.limit ?? IssueSearch.defaultLimit
            let exclude = Set(testCase.exclude ?? [])
            let local = IssueSearch.rank(rows, query: testCase.query, limit: limit, exclude: exclude)

            let merged: [IssueSearch.Row]
            if let hits = testCase.serverHits {
                let byId = Dictionary(rows.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
                let allowUnsynced = testCase.allowUnsynced ?? false
                merged = IssueSearch.mergeServerHits(
                    local: local,
                    hits: hits,
                    limit: limit,
                    exclude: exclude,
                    localId: { $0.id },
                    hitId: { $0.id },
                    resolve: { hit in
                        if let row = byId[hit.id] { return row }
                        guard allowUnsynced else { return nil }
                        return IssueSearch.Row(
                            id: hit.id,
                            identifier: hit.identifier,
                            title: hit.title
                        )
                    }
                )
            } else {
                merged = local
            }

            XCTAssertEqual(merged.map(\.id), testCase.expected, testCase.name)
        }
    }

    func testNormalizesAQuery() {
        XCTAssertEqual(IssueSearch.normalizeQuery("  #EXP-87  "), "exp-87")
        XCTAssertEqual(IssueSearch.normalizeQuery("#"), "")
    }

    func testTokenizesOnWhitespaceAndDropsPerTokenHashes() {
        XCTAssertEqual(IssueSearch.tokens("Fix  #87 login"), ["fix", "87", "login"])
        XCTAssertEqual(IssueSearch.tokens("#"), [])
    }

    func testIsStableForEqualKeys() {
        let rows = [
            IssueSearch.Row(id: "a", identifier: "X-1", title: "same"),
            IssueSearch.Row(id: "b", identifier: "X-1", title: "same"),
        ]
        XCTAssertEqual(IssueSearch.rank(rows, query: "same").map(\.id), ["a", "b"])
    }

    /// The identifier tail every recency tie-break falls back on.
    func testIdentifierNumber() {
        XCTAssertEqual(IssueSearch.identifierNumber("EXP-87"), 87)
        XCTAssertEqual(IssueSearch.identifierNumber("EXP"), -1)
        XCTAssertEqual(IssueSearch.identifierNumber("EXP-8a"), -1)
    }
}
