import Foundation
import XCTest
import ExpUI

// Locks the GFM interchange contract for the iOS block editor (a mirror of
// apps/web/src/components/issue-editor/mention-tokens.test.ts): `@<email>`
// mentions and `#<IDENTIFIER>` issue references are PLAIN GFM TEXT — pills are
// render-only decorations and the autocomplete inserts plain text, so both
// tokens must round-trip byte-identically through the markdown ↔ blocks
// serialization.
final class MentionIssueRefRoundTripTests: XCTestCase {
    private func roundTrip(_ markdown: String) -> String {
        MarkdownConversion.blocksToMarkdown(MarkdownConversion.markdownToBlocks(markdown))
    }

    func testKeepsEmailMentionsAsPlainText() {
        let src = "Ping @ada@example.com about the rollout"
        XCTAssertEqual(roundTrip(src), src)
    }

    func testKeepsIssueRefsAsPlainText() {
        let src = "Duplicate of #EXP-42, see also #EXP-7"
        XCTAssertEqual(roundTrip(src), src)
    }

    func testRoundTripsTokensAtLineStart() {
        XCTAssertEqual(roundTrip("#EXP-42 needs a second look"), "#EXP-42 needs a second look")
        XCTAssertEqual(roundTrip("@ada@example.com owns this"), "@ada@example.com owns this")
    }

    func testRoundTripsBothTokensAcrossParagraphs() {
        let src = "Intro for @ada@example.com\n\nRelates to #EXP-42"
        XCTAssertEqual(roundTrip(src), src)
    }

    func testIsIdempotentAcrossASecondPass() {
        let once = roundTrip("Ping @ada@example.com about #EXP-42")
        XCTAssertEqual(roundTrip(once), once)
    }

    func testBareUrlsStayBare() {
        // No autolink at parse (web/Android parity): rewriting `https://x` to
        // `[https://x](https://x)` would break byte parity on every save.
        let src = "See https://example.com/docs for details"
        XCTAssertEqual(roundTrip(src), src)
    }
}

// The editor-model half of the contract: caret token detection (@query /
// #query), plain-text token insertion, and pill decoration staying
// serialization-invisible.
@MainActor
final class IssueEditorModelTokenTests: XCTestCase {
    func testReportsAnInProgressMentionAtTheCaret() {
        let match = IssueEditorModel.mentionMatch(beforeCaret: "Hello @ad")
        XCTAssertEqual(match?.query, "ad")
        XCTAssertEqual(match?.atOffset, 6)
    }

    func testReportsAnInProgressIssueRefAtTheCaret() {
        let match = IssueEditorModel.issueRefMatch(beforeCaret: "Fixes #EX")
        XCTAssertEqual(match?.query, "EX")
        XCTAssertEqual(match?.hashOffset, 6)
    }

    func testDoesNotTriggerMidWord() {
        XCTAssertNil(IssueEditorModel.mentionMatch(beforeCaret: "hello@ad"))
        XCTAssertNil(IssueEditorModel.issueRefMatch(beforeCaret: "Fixes x#EX"))
    }

    func testInsertsThePlainMentionInterchangeTextOnSelection() {
        let model = IssueEditorModel()
        model.mentionMembers = [MentionMember(name: "Ada Lovelace", email: "ada@example.com")]
        model.load(markdown: "Hello @ad", baseURL: nil)
        guard let first = model.blocks.first, case let .text(blockId, _) = first else {
            return XCTFail("expected a leading text block")
        }
        model.updateSelection(blockId: blockId, range: NSRange(location: 9, length: 0))
        XCTAssertEqual(model.mentionCandidates.map(\.email), ["ada@example.com"])
        // Mirrors the web insertToken: plain text, never a custom node.
        model.applyMention(model.mentionCandidates[0])
        XCTAssertEqual(model.currentMarkdown(), "Hello @ada@example.com")
        // The completed token no longer matches an in-progress trigger.
        XCTAssertTrue(model.mentionCandidates.isEmpty)
    }

    func testInsertsThePlainIssueRefInterchangeTextOnSelection() {
        let model = IssueEditorModel()
        model.issueRefSearch = { _ in [IssueRefCandidate(identifier: "EXP-42", title: "Some issue")] }
        // Resolve everything so the render-only pill decoration is active — it
        // must not leak into the serialized bytes.
        model.issueRefResolver = { _ in "issue-id" }
        model.load(markdown: "Fixes #EX", baseURL: nil)
        guard let first = model.blocks.first, case let .text(blockId, _) = first else {
            return XCTFail("expected a leading text block")
        }
        model.updateSelection(blockId: blockId, range: NSRange(location: 9, length: 0))
        XCTAssertEqual(model.issueRefCandidates.map(\.identifier), ["EXP-42"])
        model.applyIssueRef(model.issueRefCandidates[0])
        XCTAssertEqual(model.currentMarkdown(), "Fixes #EXP-42")
        XCTAssertTrue(model.issueRefCandidates.isEmpty)
    }

    func testPillDecorationNeverChangesSerialization() {
        let model = IssueEditorModel()
        model.issueRefResolver = { _ in "issue-id" }
        let src = "Duplicate of #EXP-42, see also #EXP-7"
        model.load(markdown: src, baseURL: nil)
        XCTAssertEqual(model.currentMarkdown(), src)
        XCTAssertFalse(model.isDirty)
    }
}

// `IssueRefs.matches` backs the render-only pill resolution; a thin mirror of
// apps/web/src/lib/issue-refs.test.ts (the regex source of truth) plus the
// iOS analog of the web's "not inside code blocks" assertion.
final class IssueRefsMatchesTests: XCTestCase {
    private func identifiers(in text: String) -> [String] {
        IssueRefs.matches(in: text).map(\.identifier)
    }

    func testExtractsAndUppercasesIdentifiers() {
        XCTAssertEqual(identifiers(in: "duplicate of #met-115, see #APP-1"), ["MET-115", "APP-1"])
    }

    func testIgnoresTokensGluedToAWordOrHash() {
        XCTAssertEqual(identifiers(in: "foo#MET-115"), [])
        XCTAssertEqual(identifiers(in: "##MET-115"), [])
    }

    func testIgnoresHeadingsAndNonIdentifierTokens() {
        XCTAssertEqual(identifiers(in: "# Heading"), [])
        XCTAssertEqual(identifiers(in: "#123"), [])
        XCTAssertEqual(identifiers(in: "#MET-"), [])
    }

    func testSkipsInlineCodeAndFencedBlocks() {
        XCTAssertEqual(identifiers(in: "see `#MET-115`"), [])
        XCTAssertEqual(identifiers(in: "```\n#MET-115\n```"), [])
    }
}
