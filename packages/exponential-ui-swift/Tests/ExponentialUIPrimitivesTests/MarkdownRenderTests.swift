import Foundation
import XCTest
@testable import ExponentialUIPrimitives

/// #VAPP-258: `fixtures/markdown-render.json`, the golden render fixture:
/// this parser's blocks in the shared normal form. A pending case must still
/// differ (the list only shrinks).
final class MarkdownRenderTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()  // Tests/ExponentialUIPrimitivesTests/
            .deletingLastPathComponent()  // Tests/
            .deletingLastPathComponent()  // packages/exponential-ui-swift/
            .deletingLastPathComponent()  // packages/
            .appendingPathComponent("exponential-ui/fixtures/markdown-render.json")
        return try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? [String: Any])
    }

    static func runs(_ inlines: [MarkdownInline]) -> [[String: Any]] {
        var out: [[String: Any]] = []
        var lastKey: String?
        for run in inlines where !run.text.isEmpty {
            let key = "\(run.bold)\(run.italic)\(run.code)\(run.strike)\(run.link ?? "\u{0}")"
            if key == lastKey, var last = out.popLast() {
                last["text"] = (last["text"] as! String) + run.text
                out.append(last)
                continue
            }
            var r: [String: Any] = ["text": run.text]
            if run.bold { r["bold"] = true }
            if run.italic { r["italic"] = true }
            if run.code { r["code"] = true }
            if run.strike { r["strike"] = true }
            if let link = run.link { r["link"] = link }
            out.append(r)
            lastKey = key
        }
        return out
    }

    static func normal(_ blocks: [MarkdownBlock]) -> [[String: Any]] {
        blocks.map { b -> [String: Any] in
            switch b.kind {
            case .paragraph: return ["kind": "paragraph", "runs": runs(b.inlines)]
            case .quote: return ["kind": "quote", "runs": runs(b.inlines)]
            case let .heading(level): return ["kind": "heading", "level": level, "runs": runs(b.inlines)]
            case let .listItem(marker, task):
                var o: [String: Any] = ["kind": "listItem", "marker": marker, "depth": b.depth, "runs": runs(b.inlines)]
                if let task { o["task"] = task }
                return o
            case .codeBlock:
                var o: [String: Any] = ["kind": "codeBlock", "text": b.inlines.map(\.text).joined()]
                if let lang = b.lang { o["lang"] = lang }
                return o
            case let .table(header, rows):
                return ["kind": "table", "header": header.map(runs), "rows": rows.map { $0.map(runs) }]
            case .rule: return ["kind": "rule"]
            case let .image(src, alt): return ["kind": "image", "src": src, "alt": alt]
            }
        }
    }

    func testEveryCaseMatchesTheNormalForm() throws {
        let f = try fixture()
        let pending = Set(((f["pending"] as? [String: [String]])?["swift"]) ?? [])
        let cases = try XCTUnwrap(f["cases"] as? [[String: Any]])
        XCTAssertGreaterThan(cases.count, 50)
        for c in cases {
            let name = c["name"] as! String
            let actual = Self.normal(Markdown.parse(c["text"] as! String)) as NSArray
            let expected = c["blocks"] as! NSArray
            if pending.contains(name) {
                XCTAssertNotEqual(actual, expected, "\(name) passes now: drop it from pending.swift")
            } else {
                XCTAssertEqual(actual, expected, name)
            }
        }
    }
}
