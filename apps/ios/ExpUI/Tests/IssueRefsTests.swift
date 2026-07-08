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
