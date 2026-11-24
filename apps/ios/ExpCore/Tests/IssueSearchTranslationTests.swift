import Foundation
import XCTest
@testable import ExpCore

// #NEW-136: the model fallback's pure half, locked on the clients that carry
// it (web `issue-search-translation.test.ts`, the reference; desktop
// `domain::issue_search_translation`) against the ONE contract fixture: same
// cases, same names.
final class IssueSearchTranslationTests: XCTestCase {
    private struct Hints: Decodable {
        let time: String
        let state: String
    }

    private struct Fixture: Decodable {
        let system: String
        let hints: Hints
        let limits: Limits
        let wantsCases: [WantsCase]
        let sanitizeCases: [SanitizeCase]
        let promptCases: [PromptCase]
    }

    private struct Limits: Decodable {
        let minWords: Int
        let maxTextLength: Int
        let maxTokens: Int
        let maxWords: Int
        let maxWordLength: Int
        let maxNames: Int
        let timeoutSeconds: Int
    }

    private struct FixtureMember: Decodable {
        let name: String?
        let email: String?
    }

    private struct WantsCase: Decodable {
        let name: String
        let now: String
        let members: [FixtureMember]
        let text: String
        let expected: Bool
    }

    private struct SanitizeCase: Decodable {
        let name: String
        let now: String
        let members: [FixtureMember]
        let text: String
        let output: String
        let expected: String?
    }

    private struct PromptCase: Decodable {
        let name: String
        let now: String
        let members: [FixtureMember]
        let text: String
        let expected: String
    }

    /// The committed contract fixture, read through `#filePath` because the
    /// unit-test bundle carries no repo resources.
    private func fixture() throws -> Fixture {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/issue-search-translation.json")
        return try JSONDecoder().decode(Fixture.self, from: try Data(contentsOf: url))
    }

    private func members(_ members: [FixtureMember]) -> [IssueSearchPhrase.Member] {
        members.map { IssueSearchPhrase.Member(name: $0.name, email: $0.email) }
    }

    func testTheConstantsAreTheFixtures() throws {
        let fixture = try fixture()
        XCTAssertEqual(fixture.system, IssueSearchTranslation.system)
        XCTAssertEqual(fixture.hints.time, IssueSearchTranslation.timeHints)
        XCTAssertEqual(fixture.hints.state, IssueSearchTranslation.stateHints)
        XCTAssertEqual(fixture.limits.minWords, IssueSearchTranslation.Limits.minWords)
        XCTAssertEqual(fixture.limits.maxTextLength, IssueSearchTranslation.Limits.maxTextLength)
        XCTAssertEqual(fixture.limits.maxTokens, IssueSearchTranslation.Limits.maxTokens)
        XCTAssertEqual(fixture.limits.maxWords, IssueSearchTranslation.Limits.maxWords)
        XCTAssertEqual(fixture.limits.maxWordLength, IssueSearchTranslation.Limits.maxWordLength)
        XCTAssertEqual(fixture.limits.maxNames, IssueSearchTranslation.Limits.maxNames)
        XCTAssertEqual(fixture.limits.timeoutSeconds, IssueSearchTranslation.Limits.timeoutSeconds)
    }

    func testWantsCases() throws {
        let cases = try fixture().wantsCases
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let now = try XCTUnwrap(WireTimestamps.parse(c.now), c.name)
            XCTAssertEqual(IssueSearchTranslation.wants(c.text, members: members(c.members), now: now), c.expected, c.name)
        }
    }

    func testSanitizeCases() throws {
        let cases = try fixture().sanitizeCases
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let now = try XCTUnwrap(WireTimestamps.parse(c.now), c.name)
            XCTAssertEqual(
                IssueSearchTranslation.sanitize(c.output, text: c.text, members: members(c.members), now: now),
                c.expected,
                c.name
            )
        }
    }

    func testPromptCases() throws {
        let cases = try fixture().promptCases
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let now = try XCTUnwrap(WireTimestamps.parse(c.now), c.name)
            XCTAssertEqual(IssueSearchTranslation.prompt(c.text, members: members(c.members), now: now), c.expected, c.name)
        }
    }
}
