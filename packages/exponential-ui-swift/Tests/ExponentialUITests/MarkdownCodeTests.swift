import XCTest
import SwiftUI
import ExponentialUICore
import ExponentialUIPrimitives
@testable import ExponentialUI

/// #VAPP-258 (fixtures/markdown-render.json): a `ts` fence paints CodeBlock's
/// token kinds (the catalog's ONE tokenizer) in the `CodeBlock/token {kind}`
/// recipe's colours, colour only; an unknown or empty language is plain.
@MainActor
final class MarkdownCodeTests: XCTestCase {
    func testATsFencePaintsCodeBlockTokenColours() throws {
        let m = try makeModel(theme: "neutral", mode: .light)
        let blocks = Markdown.parse("```ts\nconst a = 1 // one\n```")
        XCTAssertEqual(blocks[0].lang, "ts")
        let text = blocks[0].inlines.map(\.text).joined()
        let kinds = MarkdownCode.tokens(text, lang: "ts").map(\.kind)
        for kind in ["keyword", "number", "comment"] { XCTAssertTrue(kinds.contains(kind), "\(kind) in \(kinds)") }
        let spec = MarkdownTextSpec(size: 12, lineHeight: 20, weight: 400, family: nil)
        let base = TextShaper.runs(blocks[0].inlines, spec, mono: nil, ink: .black, link: .blue, codeBackground: nil)
        let painted = MarkdownCode.highlighted(base, lang: blocks[0].lang) { kind in m.codeTokenLook(kind).color.map { PlatformColor($0) } }
        XCTAssertEqual(painted.string, base.string)
        for (range, kind) in MarkdownCode.tokens(text, lang: "ts") where kind != "plain" {
            let want = try XCTUnwrap(m.codeTokenLook(kind).color, kind)
            XCTAssertEqual(painted.attribute(.foregroundColor, at: range.location, effectiveRange: nil) as? PlatformColor, PlatformColor(want), kind)
            // Colour only: the font is the block's.
            XCTAssertEqual(painted.attribute(.font, at: range.location, effectiveRange: nil) as? NSObject, base.attribute(.font, at: range.location, effectiveRange: nil) as? NSObject, kind)
        }
        for lang in [nil, "", "klingon", "plain", "ansi"] as [String?] {
            let plain = MarkdownCode.highlighted(base, lang: lang) { _ in .red }
            XCTAssertEqual(plain, base, lang ?? "nil")
        }
    }
}
