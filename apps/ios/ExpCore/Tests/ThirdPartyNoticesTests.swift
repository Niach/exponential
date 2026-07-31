import Foundation
import XCTest
@testable import ExpCore

// EXP-262: the NOTICES.txt splitter. The renderer's format contract
// (packages/licenses/src/render.ts): headings are rule sandwiches — a
// full-width rule (78 `=` or `-`), one or two non-empty title lines, a
// matching closing rule. Licence bodies are verbatim, so anything else —
// short dashes, unmatched rules, markdown `##` — must stay body text.
final class ThirdPartyNoticesTests: XCTestCase {
    private let eq = String(repeating: "=", count: 78)
    private let dash = String(repeating: "-", count: 78)

    func testPreambleOnlyBecomesOneTitlelessSection() {
        let sections = ThirdPartyNotices.parse("Some preamble text.\n\nMore preamble.\n")
        XCTAssertEqual(sections.count, 1)
        XCTAssertEqual(sections[0].title, "")
        XCTAssertEqual(sections[0].body, "Some preamble text.\n\nMore preamble.")
    }

    func testSplitsOnRuleSandwiches() {
        let text = """
        \(eq)
        EXPONENTIAL — THIRD-PARTY NOTICES
        iOS application
        \(eq)

        Preamble prose.

        \(eq)
        1. Open-source components
        \(eq)

        Intro.

        \(dash)
        MIT
        \(dash)

        MIT License text.
        """
        let sections = ThirdPartyNotices.parse(text)
        XCTAssertEqual(sections.count, 3)
        XCTAssertEqual(sections[0].title, "EXPONENTIAL — THIRD-PARTY NOTICES — iOS application")
        XCTAssertEqual(sections[0].body, "Preamble prose.")
        XCTAssertEqual(sections[1].title, "1. Open-source components")
        XCTAssertEqual(sections[1].body, "Intro.")
        XCTAssertEqual(sections[2].title, "MIT")
        XCTAssertEqual(sections[2].body, "MIT License text.")
    }

    func testShortOrUnmatchedRulesStayBodyText() {
        let text = """
        \(dash)
        Apache-2.0
        \(dash)

        A licence body with markdown-ish content:

        ## not a heading
        ---
        --------
        \(eq)
        (an unmatched full-width rule stays body text too)
        """
        let sections = ThirdPartyNotices.parse(text)
        XCTAssertEqual(sections.count, 1)
        XCTAssertEqual(sections[0].title, "Apache-2.0")
        XCTAssertTrue(sections[0].body.contains("## not a heading"))
        XCTAssertTrue(sections[0].body.contains("--------"))
        XCTAssertTrue(sections[0].body.contains(eq))
    }

    func testMismatchedSandwichCharactersDoNotSplit() {
        // An `=` rule closed by a `-` rule is not a heading.
        let text = "\(eq)\nnot a title\n\(dash)\nbody"
        let sections = ThirdPartyNotices.parse(text)
        XCTAssertEqual(sections.count, 1)
        XCTAssertEqual(sections[0].title, "")
        XCTAssertTrue(sections[0].body.contains("not a title"))
    }

    func testToleratesCrlfLineEndings() {
        let text = "Preamble.\r\n\(dash)\r\nMIT\r\n\(dash)\r\nLicence body.\r\n"
        let sections = ThirdPartyNotices.parse(text)
        XCTAssertEqual(sections.count, 2)
        XCTAssertEqual(sections[0].body, "Preamble.")
        XCTAssertEqual(sections[1].title, "MIT")
        XCTAssertEqual(sections[1].body, "Licence body.")
    }
}
