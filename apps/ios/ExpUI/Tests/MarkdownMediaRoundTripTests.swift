import Foundation
import XCTest
import ExpCore
import ExpUI

// EXP-824: inline video/audio is stored as a PLAIN LINK on its own paragraph,
// `[clip.mp4](/api/attachments/{id})` — never the image form. Only a
// paragraph consisting solely of one link to an attachment URL becomes the
// media block; the same link inside running text (or a quote, list, cell)
// stays an ordinary link. Byte-locked on all four clients.
final class MarkdownMediaRoundTripTests: XCTestCase {
    private let base = URL(string: "https://app.example.com")!

    private func roundTrip(_ markdown: String, baseURL: URL? = nil) -> String {
        MarkdownConversion.blocksToMarkdown(MarkdownConversion.markdownToBlocks(markdown, baseURL: baseURL))
    }

    private func assertStable(_ markdown: String, file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertEqual(roundTrip(markdown), markdown, file: file, line: line)
    }

    private func mediaBlocks(_ markdown: String, baseURL: URL? = nil) -> [(url: String, label: String)] {
        MarkdownConversion.markdownToBlocks(markdown, baseURL: baseURL).compactMap { block in
            if case .attachmentLink(_, let url, let label) = block { return (url, label) }
            return nil
        }
    }

    // MARK: - Contract fixtures (byte-for-byte on every client)

    func testSoleAttachmentLinkFixtureRoundTrips() {
        assertStable("[clip.mp4](/api/attachments/abc123)")
    }

    func testAttachmentLinkBetweenParagraphsFixtureRoundTrips() {
        assertStable("before\n\n[clip.mp4](/api/attachments/abc)\n\nafter")
    }

    func testFixturesParseToAMediaBlock() {
        let solo = mediaBlocks("[clip.mp4](/api/attachments/abc123)")
        XCTAssertEqual(solo.count, 1)
        XCTAssertEqual(solo.first?.url, "/api/attachments/abc123")
        XCTAssertEqual(solo.first?.label, "clip.mp4")

        let blocks = MarkdownConversion.markdownToBlocks("before\n\n[clip.mp4](/api/attachments/abc)\n\nafter")
        XCTAssertEqual(blocks.count, 3)
        guard case .text(_, let before) = blocks[0],
              case .attachmentLink(_, let url, _) = blocks[1],
              case .text(_, let after) = blocks[2] else {
            return XCTFail("expected text / media / text, got \(blocks)")
        }
        XCTAssertEqual(before.string, "before")
        XCTAssertEqual(url, "/api/attachments/abc")
        XCTAssertEqual(after.string, "after")
    }

    // MARK: - What is (not) a media block

    func testLinkInsideRunningTextStaysInline() {
        let md = "see [clip.mp4](/api/attachments/abc) here"
        assertStable(md)
        XCTAssertTrue(mediaBlocks(md).isEmpty)
        let blocks = MarkdownConversion.markdownToBlocks(md)
        XCTAssertEqual(blocks.count, 1)
    }

    func testTwoLinksOnOneParagraphStayInline() {
        let md = "[a.mp4](/api/attachments/a) [b.mp4](/api/attachments/b)"
        assertStable(md)
        XCTAssertTrue(mediaBlocks(md).isEmpty)
    }

    func testNonAttachmentLinkStaysInline() {
        assertStable("[docs](https://example.com/api/attachments/abc)")
        XCTAssertTrue(mediaBlocks("[docs](https://example.com/api/attachments/abc)").isEmpty)
        XCTAssertTrue(mediaBlocks("[docs](https://example.com/guide)").isEmpty)
        XCTAssertTrue(mediaBlocks("[x](/api/attachmentsX/abc)").isEmpty)
        XCTAssertTrue(mediaBlocks("[x](/api/attachments/)").isEmpty)
        XCTAssertTrue(mediaBlocks("[x](/api/attachments/a/b)").isEmpty)
    }

    func testSameOriginAbsoluteUrlIsAMediaBlockAndKeepsItsBytes() {
        let md = "[clip.mp4](https://app.example.com/api/attachments/abc)"
        XCTAssertEqual(roundTrip(md, baseURL: base), md)
        XCTAssertEqual(mediaBlocks(md, baseURL: base).count, 1)
        // A foreign host — even with the same path — is never ours.
        XCTAssertTrue(mediaBlocks("[clip.mp4](https://other.example.com/api/attachments/abc)", baseURL: base).isEmpty)
        // And with no base URL an absolute link can't be same-origin.
        XCTAssertTrue(mediaBlocks(md).isEmpty)
    }

    func testWidthQueryIsIgnoredForResolutionButKeptInTheBytes() {
        let md = "[clip.mp4](/api/attachments/abc?w=480)"
        assertStable(md)
        XCTAssertEqual(mediaBlocks(md).first?.url, "/api/attachments/abc?w=480")
        XCTAssertEqual(AttachmentLinks.attachmentId(fromUrl: "/api/attachments/ABC?w=480", baseURL: nil), "abc")
    }

    func testLinkWithTitleOrFormattingStaysInline() {
        // (Link titles are dropped by the inline serializer on every load —
        // pre-existing behaviour, not part of this contract — so only the
        // block decision is asserted for the titled form.)
        XCTAssertTrue(mediaBlocks("[clip.mp4](/api/attachments/abc \"title\")").isEmpty)
        assertStable("[**clip**](/api/attachments/abc)")
        XCTAssertTrue(mediaBlocks("[**clip**](/api/attachments/abc)").isEmpty)
    }

    func testLinkInQuoteListOrCellStaysInline() {
        assertStable("> [clip.mp4](/api/attachments/abc)")
        XCTAssertTrue(mediaBlocks("> [clip.mp4](/api/attachments/abc)").isEmpty)
        assertStable("- [clip.mp4](/api/attachments/abc)")
        XCTAssertTrue(mediaBlocks("- [clip.mp4](/api/attachments/abc)").isEmpty)
        let table = "| a |\n| --- |\n| [clip.mp4](/api/attachments/abc) |"
        assertStable(table)
        XCTAssertTrue(mediaBlocks(table).isEmpty)
    }

    func testImageFormIsNeverAMediaBlock() {
        let md = "![clip.mp4](/api/attachments/abc)"
        assertStable(md)
        XCTAssertTrue(mediaBlocks(md).isEmpty)
    }

    func testEscapedBracketsInTheLabelRoundTrip() {
        let md = "[clip \\[1\\].mp4](/api/attachments/abc)"
        assertStable(md)
        XCTAssertEqual(mediaBlocks(md).first?.label, "clip [1].mp4")
    }

    func testMediaBlockNeighboursAreSeparateParagraphs() {
        // Two media blocks back to back keep their own paragraphs, and the
        // normalize separator between them serializes to nothing.
        assertStable("[a.mp4](/api/attachments/a)\n\n[b.mp4](/api/attachments/b)")
        XCTAssertEqual(mediaBlocks("[a.mp4](/api/attachments/a)\n\n[b.mp4](/api/attachments/b)").count, 2)
    }

    func testMediaBlockIsIdempotent() {
        let once = roundTrip("intro\n\n[clip.mp4](/api/attachments/abc)")
        XCTAssertEqual(roundTrip(once), once)
    }

    // MARK: - AttachmentLinks

    func testAttachmentIdResolution() {
        XCTAssertEqual(AttachmentLinks.attachmentId(fromUrl: "/api/attachments/abc", baseURL: nil), "abc")
        XCTAssertEqual(
            AttachmentLinks.attachmentId(fromUrl: "https://app.example.com/api/attachments/abc", baseURL: base),
            "abc"
        )
        XCTAssertNil(AttachmentLinks.attachmentId(fromUrl: "https://evil.example.com/api/attachments/abc", baseURL: base))
        XCTAssertNil(AttachmentLinks.attachmentId(fromUrl: "draft://x", baseURL: base))
        XCTAssertNil(AttachmentLinks.attachmentId(fromUrl: "/api/attachments/", baseURL: nil))
        XCTAssertTrue(AttachmentLinks.isBlockURL("draft://x", baseURL: nil))
        XCTAssertEqual(AttachmentLinks.posterUrl(attachmentId: "abc"), "/api/attachments/abc?poster=1")
        XCTAssertEqual(AttachmentLinks.escapeLabel("a[b]\\c"), "a\\[b\\]\\\\c")
    }

    func testMediaInfoAspectRatioDefaultsTo16By9() {
        XCTAssertEqual(
            AttachmentMediaInfo(contentType: "video/mp4", width: nil, height: nil, durationMs: nil, hasPoster: false)
                .aspectRatio,
            16.0 / 9.0
        )
        XCTAssertEqual(
            AttachmentMediaInfo(contentType: "video/mp4", width: 1280, height: 720, durationMs: nil, hasPoster: false)
                .aspectRatio,
            1280.0 / 720.0
        )
    }
}
