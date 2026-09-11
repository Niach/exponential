import Foundation
import XCTest
import ExpUI

// EXP-824: the plain-link twins of the image draft helpers, so a failed video
// upload can never leak a `[clip.mp4](draft://…)` into a saved body.
final class MarkdownLinkUtilsTests: XCTestCase {
    func testLinkOccurrencesSkipImages() {
        let md = "![shot](/api/attachments/i) and [clip.mp4](/api/attachments/v) and [docs](https://x.example \"t\")"
        let links = MarkdownImageUtils.linkOccurrences(in: md)
        XCTAssertEqual(links.map(\.url), ["/api/attachments/v", "https://x.example"])
        XCTAssertEqual(links.map(\.label), ["clip.mp4", "docs"])
        XCTAssertEqual(MarkdownImageUtils.extractImageUrls(from: md), ["/api/attachments/i"])
    }

    func testHasDraftLinksAndReferences() {
        XCTAssertTrue(MarkdownImageUtils.hasDraftLinks("[clip.mp4](draft://a)"))
        XCTAssertFalse(MarkdownImageUtils.hasDraftLinks("![x](draft://a)"))
        XCTAssertFalse(MarkdownImageUtils.hasDraftLinks("[clip.mp4](/api/attachments/a)"))
        XCTAssertTrue(MarkdownImageUtils.hasDraftReferences("![x](draft://a)"))
        XCTAssertTrue(MarkdownImageUtils.hasDraftReferences("[x](draft://a)"))
        XCTAssertFalse(MarkdownImageUtils.hasDraftReferences("[x](/api/attachments/a) ![y](/api/attachments/b)"))
    }

    func testReplaceLinkUrlKeepsLabelAndTitleAndTargetsExactUrl() {
        let md = "[clip.mp4](draft://a) [other](draft://ab) [t](draft://a \"title\")"
        let out = MarkdownImageUtils.replaceLinkUrl(in: md, from: "draft://a", to: "/api/attachments/real")
        XCTAssertEqual(
            out,
            "[clip.mp4](/api/attachments/real) [other](draft://ab) [t](/api/attachments/real \"title\")"
        )
    }

    func testStripUnknownDraftLinksLeavesRealLinksAlone() {
        let md = "a\n\n[clip.mp4](draft://gone)\n\n[keep.mp4](draft://keep)\n\n[real](/api/attachments/x)"
        let out = MarkdownImageUtils.stripUnknownDraftLinks(md, keep: ["draft://keep"])
        XCTAssertEqual(out, "a\n\n\n\n[keep.mp4](draft://keep)\n\n[real](/api/attachments/x)")
    }

    func testStripUnknownDraftsCoversBothForms() {
        let md = "![i](draft://i) [v](draft://v) [r](/api/attachments/r)"
        XCTAssertEqual(MarkdownImageUtils.stripUnknownDrafts(md, keep: []), "  [r](/api/attachments/r)")
    }
}
