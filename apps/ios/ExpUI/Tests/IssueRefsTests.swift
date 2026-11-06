import Foundation
import XCTest
import ExpUI

// Regression gate for the comment-render crash: `linkifyForDisplay` produced
// NSRanges against a masked copy of the text and applied them to the original
// via `NSString.substring`, which raises an uncatchable NSRangeException on any
// UTF-16-width divergence — permanently un-openable issue. These feed it the
// crash-shaped inputs (code spans/fences, CRLF, multi-scalar graphemes) and
// assert it decorates the right tokens and never crashes.
final class IssueRefsLinkifyTests: XCTestCase {
    // Resolves EXP-1 / EXP-12 (uppercase-normalized); everything else unknown.
    private let resolver: (String) -> String? = { id in
        ["EXP-1": "id-1", "EXP-12": "id-12"][id]
    }

    private func linkify(_ markdown: String) -> String {
        IssueRefs.linkifyForDisplay(markdown, resolver: resolver)
    }

    // 1 — inline code span: the ref inside backticks is left alone.
    func testInlineCodeSpanRefNotLinkified() {
        let out = linkify("run `#EXP-1` then #EXP-12")
        XCTAssertEqual(out, "run `#EXP-1` then [#EXP-12](exp-issue://id-12)")
    }

    // 2 — fenced code block (closed): refs inside stay plain, refs outside link.
    func testFencedBlockRefNotLinkified() {
        let out = linkify("a\n```\n#EXP-1\n```\n#EXP-12")
        XCTAssertTrue(out.contains("[#EXP-12](exp-issue://id-12)"))
        XCTAssertTrue(out.contains("#EXP-1"), "the fenced ref stays as plain text")
        XCTAssertFalse(out.contains("[#EXP-1]"), "the fenced ref must not be linkified")
    }

    // 2b — unclosed fence: everything after the fence is code → nothing links,
    // and it must not crash.
    func testUnclosedFenceDoesNotCrash() {
        XCTAssertEqual(linkify("```\n#EXP-1"), "```\n#EXP-1")
    }

    // 3 — CRLF line endings must keep masked/original UTF-16 widths aligned.
    func testCRLFInputDoesNotCrash() {
        let input = "a\r\n#EXP-1\r\n```\r\n#EXP-2\r\n```"
        let out = linkify(input)
        XCTAssertTrue(out.contains("[#EXP-1](exp-issue://id-1)") || out == input)
    }

    // 4 — multi-scalar graphemes around and inside a code span.
    func testMultiScalarGraphemesDoNotCrash() {
        let out = linkify("👨‍👩‍👧‍👦 `x👍y` #EXP-1")
        XCTAssertTrue(out.contains("[#EXP-1](exp-issue://id-1)"))
        XCTAssertTrue(out.contains("`x👍y`"), "the code span is untouched")
    }

    // 5 — boundary rules: trailing punctuation matches; glued-to-word/hash doesn't.
    func testTokenBoundaries() {
        XCTAssertEqual(linkify("#EXP-1."), "[#EXP-1](exp-issue://id-1).")
        XCTAssertEqual(linkify("##EXP-1"), "##EXP-1")
        XCTAssertEqual(linkify("word#EXP-1"), "word#EXP-1")
    }

    // 6 — an unresolved ref leaves the text byte-for-byte identical.
    func testUnresolvedRefIsUnchanged() {
        XCTAssertEqual(linkify("see #EXP-99 later"), "see #EXP-99 later")
    }

    // 7 — the link text preserves the original case (carried from the match,
    // not re-derived from the uppercased identifier).
    func testCasePreservedInLinkText() {
        XCTAssertEqual(linkify("#exp-1"), "[#exp-1](exp-issue://id-1)")
    }
}

// Locks the `extractInlineMarkdown` list-prefix fix: the prefix length must be a
// UTF-16 count (clamped), not a Character distance, so multi-scalar content
// right after a list marker can't feed `enumerateAttributes` an out-of-bounds
// range (NSRangeException). Driven through the public serialization pipeline.
final class MarkdownConversionListPrefixTests: XCTestCase {
    private func roundTrip(_ markdown: String) -> String {
        MarkdownConversion.blocksToMarkdown(MarkdownConversion.markdownToBlocks(markdown))
    }

    func testBulletWithEmoji() {
        XCTAssertEqual(roundTrip("- 👍 item"), "- 👍 item")
    }

    func testBulletWithBoldAndEmoji() {
        XCTAssertEqual(roundTrip("- **bold** 👍"), "- **bold** 👍")
    }

    func testUncheckedTaskWithEmoji() {
        XCTAssertEqual(roundTrip("- [ ] task 👍"), "- [ ] task 👍")
    }

    func testCheckedTaskWithFamilyEmoji() {
        XCTAssertEqual(roundTrip("- [x] 👨‍👩‍👧‍👦 done"), "- [x] 👨‍👩‍👧‍👦 done")
    }
}

// EXP-760 — the steering feeds also chip identifiers written WITHOUT a `#`,
// because that is how agents narrate them. Display-only and opt-in: these lock
// the bare contract byte-for-byte against the web source of truth
// (`apps/web/src/lib/issue-refs.ts`, `bare identifiers` describe block) and
// against Android's `IssueRefsTest`.
final class IssueRefsBareModeTests: XCTestCase {
    private let resolver: (String) -> String? = { id in
        ["EXP-1": "id-1", "EXP-758": "id-758", "APP-33": "id-33", "MET-2": "id-2"][id]
    }

    private func identifiers(_ text: String, bare: Bool) -> [String] {
        IssueRefs.matches(in: text, bare: bare).map(\.identifier)
    }

    func testBareIdentifierMatchesOnlyInBareMode() {
        XCTAssertEqual(identifiers("Filed EXP-758 for the follow-up", bare: true), ["EXP-758"])
        XCTAssertEqual(identifiers("Filed EXP-758 for the follow-up", bare: false), [])
    }

    func testHashFormStaysOneMatchInBareMode() {
        let found = IssueRefs.matches(in: "see #EXP-1 now", bare: true)
        XCTAssertEqual(found.count, 1)
        XCTAssertEqual(found.first?.token, "#EXP-1")
        XCTAssertEqual(found.first?.identifier, "EXP-1")
        XCTAssertEqual(found.first?.isBare, false)
    }

    func testLowercaseAndGluedBareTokensAreIgnored() {
        XCTAssertEqual(identifiers("utf-8 and x86-64 and exp-758", bare: true), [])
        XCTAssertEqual(identifiers("foo-EXP-1 fooEXP-1 #EXP-1abc", bare: true), [])
    }

    func testBranchMentionChips() {
        let found = IssueRefs.matches(in: "pushed exp/EXP-758", bare: true)
        XCTAssertEqual(found.map(\.identifier), ["EXP-758"])
        XCTAssertEqual(found.first?.isBare, true)
    }

    func testMixedTextMatchesEveryForm() {
        XCTAssertEqual(
            identifiers("EXP-1, then #MET-2 (utf-8) exp/APP-33.", bare: true),
            ["EXP-1", "MET-2", "APP-33"]
        )
    }

    func testBareTokensInsideCodeStayPlain() {
        XCTAssertEqual(identifiers("run `EXP-1` but EXP-758 links", bare: true), ["EXP-758"])
    }

    func testLinkifyOnlyTouchesBareTokensInBareMode() {
        let text = "pushed exp/EXP-758 for #EXP-1"
        XCTAssertEqual(
            IssueRefs.linkifyForDisplay(text, resolver: resolver, bare: true),
            "pushed exp/[EXP-758](exp-issue://id-758) for [#EXP-1](exp-issue://id-1)"
        )
        XCTAssertEqual(
            IssueRefs.linkifyForDisplay(text, resolver: resolver),
            "pushed exp/EXP-758 for [#EXP-1](exp-issue://id-1)"
        )
    }

    func testUnresolvedBareTokenStaysPlainText() {
        XCTAssertEqual(
            IssueRefs.linkifyForDisplay("see EXP-999 later", resolver: resolver, bare: true),
            "see EXP-999 later"
        )
    }

}

// #EXP-1674: THE scan, replayed ×4 against fixtures/issue-refs.json (web
// `issue-refs.test.ts`, desktop `markdown/editor.rs`, Android `IssueRefsTest`).
final class IssueRefsFixtureTests: XCTestCase {
    func testEveryCaseScansTheSame() throws {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpUI/Tests/
            .deletingLastPathComponent()          // ExpUI/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/issue-refs.json")
        let json = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? [String: Any]
        )
        let cases = try XCTUnwrap(json["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let text = try XCTUnwrap(testCase["text"] as? String)
            XCTAssertEqual(IssueRefs.matches(in: text).map(\.token), testCase["hash"] as? [String], name)
            XCTAssertEqual(IssueRefs.matches(in: text, bare: true).map(\.token), testCase["bare"] as? [String], name)
        }
    }
}
