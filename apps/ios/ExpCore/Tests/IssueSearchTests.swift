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
        /// #NEW-132: the clock relative ranges count back from.
        let now: String?
        let exclude: [String]?
        let limit: Int?
        let serverHits: [FixtureHit]?
        let allowUnsynced: Bool?
        let expected: [String]

        /// The case's clock (`now`, parsed like any wire timestamp), else the
        /// real one.
        var clock: Date {
            now.flatMap(WireTimestamps.parse) ?? Date()
        }
    }

    private struct FixtureRow: Decodable {
        let id: String
        let identifier: String
        let title: String
        let description: String?
        let createdAt: String?
        let updatedAt: String?
        let status: String?
        let creatorName: String?
        let creatorEmail: String?
        let creatorAgentName: String?
        let statusName: String?

        var row: IssueSearch.Row {
            IssueSearch.Row(
                id: id,
                identifier: identifier,
                title: title,
                description: description,
                createdAt: createdAt,
                updatedAt: updatedAt,
                status: status,
                creatorName: creatorName,
                creatorEmail: creatorEmail,
                creatorAgentName: creatorAgentName,
                statusName: statusName
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
            let now = testCase.clock
            let local = IssueSearch.rank(rows, query: testCase.query, limit: limit, exclude: exclude, now: now)

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

    /// #EXP-1382: the prepared pool (the phones' Search, ranked off the main
    /// actor) orders every fixture case exactly like `rank` — fresh, and
    /// rebuilt from a previous snapshot (reused entries carry the new order).
    func testPreparedIndexReplaysTheFixture() throws {
        for testCase in try cases() {
            let rows = testCase.rows.map(\.row)
            let limit = testCase.limit ?? IssueSearch.defaultLimit
            let exclude = Set(testCase.exclude ?? [])
            let now = testCase.clock
            let expected = IssueSearch.rank(rows, query: testCase.query, limit: limit, exclude: exclude, now: now).map(\.id)
            let index = IssueSearch.Index(rows, projection: { $0 })
            XCTAssertEqual(index.rank(query: testCase.query, limit: limit, exclude: exclude, now: now).map(\.id), expected, testCase.name)
            let rebuilt = IssueSearch.Index(rows, reusing: IssueSearch.Index(rows.reversed(), projection: { $0 }), projection: { $0 })
            XCTAssertEqual(rebuilt.rank(query: testCase.query, limit: limit, exclude: exclude, now: now).map(\.id), expected, testCase.name)
        }
    }

    func testPreparedIndexReusesOnlyUnchangedRows() {
        let before = [
            IssueSearch.Row(id: "a", identifier: "X-1", title: "alpha"),
            IssueSearch.Row(id: "b", identifier: "X-2", title: "beta"),
        ]
        let previous = IssueSearch.Index(before, projection: { $0 })
        // `b` renamed: the stale prepared title must not survive the rebuild.
        let after = [before[0], IssueSearch.Row(id: "b", identifier: "X-2", title: "gamma")]
        let index = IssueSearch.Index(after, reusing: previous, projection: { $0 })
        XCTAssertEqual(index.rank(query: "beta").map(\.id), [])
        XCTAssertEqual(index.rank(query: "gamma").map(\.id), ["b"])
        XCTAssertEqual(index.value(id: "b")?.title, "gamma")
        XCTAssertEqual(index.count, 2)
    }

    /// #NEW-132: `+HH:MM` offsets and fractional seconds still parse to the
    /// same instant the engine filters on.
    func testParsesRangeValues() {
        let now = WireTimestamps.parse("2026-11-14T12:00:00Z")!
        let day = IssueSearch.parseRange("2026-11-03", now: now)
        XCTAssertEqual(day, IssueSearch.DateRange(from: 1_793_664_000, to: 1_793_750_400))
        XCTAssertEqual(IssueSearch.parseRange("2028-02", now: now)?.to, IssueSearch.parseRange("2028-03-01", now: now)?.from)
        XCTAssertEqual(IssueSearch.parseRange("1d", now: now)?.from, now.timeIntervalSince1970 - 86_400)
        XCTAssertNil(IssueSearch.parseRange("0d", now: now))
        XCTAssertNil(IssueSearch.parseRange("2026-13", now: now))
        XCTAssertNil(IssueSearch.parseRange("2026-02-29", now: now))
        let rows = [
            IssueSearch.Row(id: "a", identifier: "X-1", title: "a", createdAt: "2026-11-04T00:30:00.250+01:00"),
            IssueSearch.Row(id: "b", identifier: "X-2", title: "b", createdAt: "2026-11-04 00:00:00.000001+00"),
        ]
        XCTAssertEqual(IssueSearch.rank(rows, query: "created:2026-11-03").map(\.id), ["a"])
        XCTAssertEqual(IssueSearch.rank(rows, query: "created:2026-11-04").map(\.id), ["b"])
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
