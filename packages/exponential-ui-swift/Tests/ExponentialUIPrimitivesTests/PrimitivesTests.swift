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
