import Foundation
import XCTest

@testable import ExpCore

// EXP-1154: `issues.prDescription` feeds the Results face's fallback while an
// open PR has no run report. Every field is nullable (no PR, no repo).
final class PrDescriptionDecodingTests: XCTestCase {
    private func decode(_ json: String) throws -> PrDescription {
        try JSONDecoder().decode(PrDescription.self, from: Data(json.utf8))
    }

    func testDecodesAPullRequest() throws {
        let description = try decode("""
        {"repo":"acme/app","prNumber":42,"url":"https://github.com/acme/app/pull/42",
        "title":"APP-14: tidy the nav","body":"## Summary\\nDid it","state":"open"}
        """)
        XCTAssertEqual(
            description,
            PrDescription(
                repo: "acme/app",
                prNumber: 42,
                url: "https://github.com/acme/app/pull/42",
                title: "APP-14: tidy the nav",
                body: "## Summary\nDid it",
                state: "open"
            )
        )
    }

    func testDecodesTheNoPullRequestAnswer() throws {
        let description = try decode("""
        {"repo":null,"prNumber":null,"url":null,"title":null,"body":null,"state":null}
        """)
        XCTAssertEqual(description, PrDescription())
    }
}
