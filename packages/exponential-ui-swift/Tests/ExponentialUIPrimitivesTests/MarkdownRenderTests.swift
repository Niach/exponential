import Foundation
import XCTest
@testable import ExponentialUIPrimitives

/// #VAPP-258: `fixtures/markdown-render.json`, the golden render fixture:
/// this parser's blocks in the shared normal form. A pending case must still
/// differ (the list only shrinks).
final class MarkdownRenderTests: XCTestCase {
    /// #VAPP-258 (`fixtures/markdown-host.json` `imageBoxes`): the block image box.
    func testImageBoxes() throws {
        let url = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().appendingPathComponent("exponential-ui/fixtures/markdown-host.json")
        let f = try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? [String: Any])
        let boxes = try XCTUnwrap(f["imageBoxes"] as? [[String: Any]])
        XCTAssertFalse(boxes.isEmpty)
        for b in boxes {
            let name = b["name"] as! String
            let size = (b["size"] as? [String: Double]).map { CGSize(width: $0["width"]!, height: $0["height"]!) }
            let box = Markdown.imageBox(size: size, available: CGFloat(b["available"] as! Double), maxHeight: 160)
            let expected = b["box"] as! [String: Double]
            XCTAssertEqual(box.width, expected["width"]!, accuracy: 0.01, name)
            XCTAssertEqual(box.height, expected["height"]!, accuracy: 0.01, name)
        }
    }

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
            // #VAPP-258: a chip run never merges with a neighbour.
            if run.chip == nil, key == lastKey, var last = out.popLast() {
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
            if let chip = run.chip {
                var c: [String: Any] = ["kind": chip.kind, "label": chip.label]
                if let icon = chip.icon { c["icon"] = icon }
                r["chip"] = c
            }
            out.append(r)
            lastKey = run.chip == nil ? key : nil
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
            case let .image(src, alt):
                var o: [String: Any] = ["kind": "image", "src": src, "alt": alt]
                if let size = b.imageSize {
                    o["width"] = Double(size.width)
                    o["height"] = Double(size.height)
                }
                return o
            case let .attachment(type, name, href): return ["kind": "attachment", "type": type.rawValue, "name": name, "href": href]
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

    /// #VAPP-258 step 5: host options (`breaks`) and documents streamed in
    /// prefix by prefix, in order (the memo) and cold.
    func testOptionCases() throws {
        let f = try fixture()
        for c in try XCTUnwrap(f["optionCases"] as? [[String: Any]]) {
            let hard = (c["options"] as? [String: Any])?["breaks"] as? String == "hard"
            XCTAssertEqual(Self.normal(Markdown.parse(c["text"] as! String, hardBreaks: hard)) as NSArray, c["blocks"] as! NSArray, c["name"] as! String)
        }
    }

    func testStreamingCases() throws {
        let f = try fixture()
        let cases = try XCTUnwrap(f["streamingCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let text = c["text"] as! String
            let hard = (c["options"] as? [String: Any])?["breaks"] as? String == "hard"
            let steps = c["steps"] as! [NSArray]
            for cold in [false, true] {
                Markdown.resetStreamMemo()
                for (i, expected) in steps.enumerated() {
                    if cold { Markdown.resetStreamMemo() }
                    let prefix = String(text.prefix(i + 1))
                    let actual = Self.normal(Markdown.parse(prefix, hardBreaks: hard, streaming: true)) as NSArray
                    XCTAssertEqual(actual, expected, "\(c["name"] as! String) \(prefix.debugDescription) cold \(cold)")
                    if actual != expected { break }
                }
            }
        }
    }

    /// A streamed text's later list is its own group (a gap between lists).
    func testStreamedListsKeepTheirGroups() {
        Markdown.resetStreamMemo()
        _ = Markdown.parse("- a\n- b\n\n", hardBreaks: false, streaming: true)
        let blocks = Markdown.parse("- a\n- b\n\n- c", hardBreaks: false, streaming: true)
        XCTAssertEqual(blocks[0].listGroup, blocks[1].listGroup)
        XCTAssertNotEqual(blocks[1].listGroup, blocks[2].listGroup)
    }
}
