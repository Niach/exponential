import Foundation
import XCTest
@testable import ExpCore

// #NEW-133: the phrase grammar, locked ×4 (web `issue-search-phrase.test.ts`,
// Android `IssueSearchPhraseTest`, desktop `domain::issue_search_phrase`)
// against the ONE contract fixture — same cases, same names.
final class IssueSearchPhraseTests: XCTestCase {
    private struct Fixture: Decodable {
        let cases: [FixtureCase]
    }

    private struct FixtureCase: Decodable {
        let name: String
        let now: String
        let members: [FixtureMember]
        let phrase: String
        let expected: String?
    }

    private struct FixtureMember: Decodable {
        let name: String?
        let email: String?
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
            .appendingPathComponent("packages/domain-contract/fixtures/issue-search-phrase.json")
        return try JSONDecoder().decode(Fixture.self, from: try Data(contentsOf: url)).cases
    }

    func testContractFixtureCases() throws {
        let cases = try cases()
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let now = try XCTUnwrap(WireTimestamps.parse(c.now), c.name)
            let members = c.members.map { IssueSearchPhrase.Member(name: $0.name, email: $0.email) }
            XCTAssertEqual(
                IssueSearchPhrase.translate(c.phrase, members: members, now: now),
                c.expected,
                c.name
            )
            // The epoch-ms clock reads the same day.
            XCTAssertEqual(
                IssueSearchPhrase.translate(
                    c.phrase,
                    members: members,
                    nowMs: Int64((now.timeIntervalSince1970 * 1000).rounded(.down))
                ),
                c.expected,
                c.name
            )
        }
    }

    func testABlankTextAndNoMembers() {
        XCTAssertNil(IssueSearchPhrase.translate("", members: [], nowMs: 0))
        XCTAssertNil(IssueSearchPhrase.translate("  \n ", members: [], nowMs: 0))
        // The epoch's own week: 1970-01-01 was a Thursday.
        XCTAssertEqual(IssueSearchPhrase.translate("this week", members: [], nowMs: 0), "created:>=1969-12-29")
    }
}
