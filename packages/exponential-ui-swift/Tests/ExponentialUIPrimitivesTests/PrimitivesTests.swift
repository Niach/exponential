import XCTest
@testable import ExponentialUIPrimitives

final class MarkdownParseTests: XCTestCase {
    func testParsesBlocks() {
        let blocks = Markdown.parse("# Title\n\nHello **bold** and `code`.\n\n- a\n- [x] b\n\n| a | b |\n| --- | --- |\n| 1 | 2 |")
        XCTAssertEqual(blocks.count, 5)
        XCTAssertEqual(blocks[0].kind, .heading(1))
        XCTAssertEqual(blocks[1].inlines.map(\.text), ["Hello ", "bold", " and ", "code", "."])
        XCTAssertTrue(blocks[1].inlines[1].bold)
        XCTAssertTrue(blocks[1].inlines[3].code)
        XCTAssertEqual(blocks[3].kind, .listItem(marker: "•", task: true))
        XCTAssertEqual(blocks[2].listGroup, blocks[3].listGroup)
        if case let .table(header, rows) = blocks[4].kind {
            XCTAssertEqual(header.count, 2)
            XCTAssertEqual(rows.count, 1)
        } else {
            XCTFail("expected a table")
        }
    }

    func testInlineRules() {
        XCTAssertEqual(Markdown.parseInline("snake_case_name").map(\.text), ["snake_case_name"])
        let link = Markdown.parseInline("see [docs](https://x.y) now")
        XCTAssertEqual(link[1].link, "https://x.y")
        XCTAssertEqual(Markdown.plainText("# A\n\nb *c*"), "A\nb c")
    }

    /// VAPP-103: destinations balance parentheses like CommonMark and hold
    /// no whitespace.
    func testLinkDestinationsBalanceParentheses() {
        XCTAssertEqual(Markdown.parseInline("[a](https://x/A_(b))").first?.link, "https://x/A_(b)")
        XCTAssertEqual(Markdown.parseInline("[a](https://x/A_(b)) tail").map(\.text), ["a", " tail"])
        XCTAssertEqual(Markdown.parseInline("[a](https://x/\\(y) z").first?.link, "https://x/(y")
        XCTAssertEqual(Markdown.parseInline("[a](https://x y)").compactMap(\.link), [])
        XCTAssertEqual(Markdown.parseInline("[a](https://x/(open").compactMap(\.link), [], "unbalanced = no link")
        XCTAssertEqual(Markdown.parseInline("[a [b] c](u)").first?.text, "a [b] c")
        let blocks = Markdown.parse("![i](javascript:alert(3))")
        XCTAssertEqual(blocks.map(\.kind), [.image(src: "javascript:alert(3)", alt: "i")])
        // Inside a line an image paints its alt text.
        XCTAssertEqual(Markdown.plain(Markdown.parseInline("x ![pic](https://a/(b)) y")), "x pic y")
    }

    func testNestedListsAndTasks() {
        let blocks = Markdown.parse("- a\n  - b\n    1. c\n    2. [ ] d\n  - [x] e\n- f\n\n1. g")
        XCTAssertEqual(blocks.map(\.depth), [0, 1, 2, 2, 1, 0, 0])
        XCTAssertEqual(blocks[2].kind, .listItem(marker: "1.", task: nil))
        XCTAssertEqual(blocks[3].kind, .listItem(marker: "2.", task: false))
        XCTAssertEqual(blocks[4].kind, .listItem(marker: "•", task: true))
        XCTAssertEqual(Markdown.plain(blocks[4].inlines), "e")
        XCTAssertEqual(Set(blocks[0...5].map(\.listGroup)).count, 1, "one list, no gaps inside")
        XCTAssertNotEqual(blocks[6].listGroup, blocks[5].listGroup)
        // One column more is no nesting.
        XCTAssertEqual(Markdown.parse("- a\n - b").map(\.depth), [0, 0])
        // Nested items indent one listIndent per level.
        let s = MarkdownStyles(body: MarkdownTextSpec(size: 14, lineHeight: 20, weight: 400, family: nil))
        XCTAssertEqual(s.listInset(blocks[2]), s.listIndent * 3)
    }

    func testImagesResolveAgainstThePolicy() {
        let blocks = Markdown.parse("![ok](https://a/p.png)\n\n![no](file:///etc/passwd)\n\n![](javascript:x)")
        let resolved = Markdown.resolveImages(blocks) { $0.hasPrefix("https:") }
        XCTAssertEqual(resolved.map(\.kind), [.image(src: "https://a/p.png", alt: "ok"), .paragraph])
        XCTAssertEqual(Markdown.plain(resolved[1].inlines), "no")
        let linked = Markdown.mapLinks(Markdown.parse("[a](https://ok) [b](javascript:x)")) { $0.hasPrefix("https:") ? $0 : nil }
        XCTAssertEqual(linked[0].inlines.compactMap(\.link), ["https://ok"])
        XCTAssertEqual(Markdown.plain(linked[0].inlines), "a b")
    }

    func testAvatarRule() {
        XCTAssertEqual(AvatarFallback.initials("Alex Chen"), "AC")
        XCTAssertEqual(AvatarFallback.initials("ada lovelace byron"), "AL")
        XCTAssertEqual(AvatarFallback.seedHue("Alex Chen"), AvatarFallback.seedHue("Alex Chen"))
        XCTAssertLessThan(AvatarFallback.seedHue("x"), 360)
        XCTAssertEqual(RGBA(hex: "#ff0000")?.r, 1)
        XCTAssertEqual(RGBA(hex: "#0f0")?.g, 1)
        XCTAssertNil(RGBA(hex: "red"))
    }
}
