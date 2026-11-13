import XCTest
@testable import ExponentialUIPrimitives

/// #FEED-224: `fixtures/table-fit.json` (TS, the Rust core and Compose
/// replay the same cases) through `MarkdownTableFit`, and the layout's
/// tables fitted by it.
final class MarkdownTableFitTests: XCTestCase {
    private static let fixture = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent()  // Tests/ExponentialUIPrimitivesTests/
        .deletingLastPathComponent()  // Tests/
        .deletingLastPathComponent()  // packages/exponential-ui-swift/
        .deletingLastPathComponent()  // packages/
        .appendingPathComponent("exponential-ui/fixtures/table-fit.json")

    func testTheFixtureReplays() throws {
        let f = try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: Self.fixture)) as? [String: Any])
        XCTAssertEqual((f["wordCap"] as? NSNumber)?.doubleValue, MarkdownTableFit.wordCap)
        let cases = try XCTUnwrap(f["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let columns = (c["columns"] as? [[String: NSNumber]] ?? []).map { MarkdownTableFit.Column(min: $0["min"]?.doubleValue ?? 0, max: $0["max"]?.doubleValue ?? 0) }
            let want = (c["widths"] as? [NSNumber] ?? []).map(\.doubleValue)
            XCTAssertEqual(MarkdownTableFit.fit(columns, available: (c["available"] as? NSNumber)?.doubleValue ?? 0), want, c["name"] as? String ?? "")
        }
    }

    /// 8 px per character, words split at spaces.
    private struct Fixed: MarkdownTextMeasure {
        func height(_ inlines: [MarkdownInline], _ spec: MarkdownTextSpec, width: CGFloat?) -> CGFloat {
            let w = CGFloat(Markdown.plain(inlines).count) * 8
            return (width.map { max((w / $0).rounded(.up), 1) } ?? 1) * spec.lineHeight
        }
        func width(_ inlines: [MarkdownInline], _ spec: MarkdownTextSpec) -> CGFloat { CGFloat(Markdown.plain(inlines).count) * 8 }
        func widestWord(_ inlines: [MarkdownInline], _ spec: MarkdownTextSpec) -> CGFloat {
            CGFloat(Markdown.plain(inlines).split(separator: " ").map(\.count).max() ?? 0) * 8
        }
    }

    /// The screenshot's shape fits by the rule, not in equal thirds.
    func testATableFitsItsColumnsToTheirContent() {
        let s = MarkdownStyles(body: MarkdownTextSpec(size: 14, lineHeight: 20, weight: 400, family: nil))
        let md = "| PR | Issue | Stand |\n|---|---|---|\n| #2229 | MA-1756-Matty-Stufe-2-Vorgabe | Gründe hängt in der Merge-Warteschlange und wartet auf CI |"
        let lay = Markdown.layout(Markdown.parse(md, hardBreaks: false), s, width: 500 + 6 * s.cellPadH, text: Fixed())
        XCTAssertEqual(lay.blocks[0].cols, [40, 230, 230])
        let short = Markdown.layout(Markdown.parse("| a | b |\n|---|---|\n| x | yy |", hardBreaks: false), s, width: 600, text: Fixed())
        XCTAssertEqual(short.blocks[0].cols, [8, 16])
    }
}
