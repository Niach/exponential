import XCTest
import SwiftUI
import ExponentialUIPrimitives
@testable import ExponentialUI

#if canImport(UIKit)
import UIKit
#elseif canImport(AppKit)
import AppKit
#endif

/// #EDE-95: `fixtures/text-highlight.json` (TS, the Rust core and Compose
/// replay the same cases) through `TextHighlight`, and the painter's
/// mapping of matches onto typeset runs and shown lines.
@MainActor
final class TextHighlightTests: XCTestCase {
    override func setUp() async throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
    }

    private func slice(_ text: String, _ r: TextHighlightRange) -> String {
        (text as NSString).substring(with: r.nsRange)
    }

    func testTheCasesReplay() throws {
        let f = try Fixtures.json("text-highlight.json")
        let cases = f["cases"]?.array ?? []
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"]?.string ?? c.json
            let text = c["text"]?.string ?? ""
            let found = TextHighlight.ranges(text, query: c["query"]?.string ?? "")
            let expected = (c["ranges"]?.array ?? []).map { pair in
                TextHighlightRange(start: Int(pair.array?[0].number ?? -1), end: Int(pair.array?[1].number ?? -1))
            }
            XCTAssertEqual(found, expected, name)
            // Compared unit by unit (Swift's `==` on String is canonical equivalence).
            XCTAssertEqual(found.map { Array(slice(text, $0).unicodeScalars) }, (c["matches"]?.array ?? []).map { Array(($0.string ?? "").unicodeScalars) }, name)
            XCTAssertEqual(TextHighlight.count(text, query: c["query"]?.string ?? ""), expected.count, name)
        }
    }

    func testTheCurrentCasesReplay() throws {
        let f = try Fixtures.json("text-highlight.json")
        let cases = f["currentCases"]?.array ?? []
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"]?.string ?? c.json
            let spec = try XCTUnwrap(TextHighlightSpec(c["highlight"]), name)
            XCTAssertEqual(spec.localCurrent, Int(c["local"]?.number ?? -99), name)
        }
    }

    func testTheMarkdownCasesReplay() throws {
        let f = try Fixtures.json("text-highlight.json")
        let cases = f["markdownCases"]?.array ?? []
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"]?.string ?? c.json
            let text = c["text"]?.string ?? ""
            let query = c["query"]?.string ?? ""
            XCTAssertEqual(TextHighlight.markdownCount(text, query: query), Int(c["count"]?.number ?? -1), name)
            let matches = TextHighlight.markdownTexts(Markdown.parse(text)).flatMap { block in
                TextHighlight.ranges(block, query: query).map { Array(slice(block, $0).unicodeScalars) }
            }
            XCTAssertEqual(matches, (c["matches"]?.array ?? []).map { Array(($0.string ?? "").unicodeScalars) }, name)
        }
    }

    /// The prop: a non-object, a missing or empty-folding query = none.
    func testTheSpecNeedsAQuery() {
        XCTAssertNil(TextHighlightSpec(nil))
        XCTAssertNil(TextHighlightSpec(.string("foo")))
        XCTAssertNil(TextHighlightSpec(.object(["query": .string("")])))
        XCTAssertNil(TextHighlightSpec(.object(["query": .string("\u{301}")])))
        XCTAssertNil(TextHighlightSpec(.object(["query": .number(3)])))
        let spec = TextHighlightSpec(.object(["query": .string("a"), "current": .number(2.7), "before": .string("")]))
        XCTAssertEqual(spec, TextHighlightSpec(query: "a", current: 2, before: nil))
    }

    /// The node count a host numbers matches with (`nodeHighlightCount`).
    func testNodeCounts() {
        let h: JSONValue = .object(["query": .string("foo")])
        XCTAssertEqual(TextHighlight.nodeCount("Text", props: ["text": .string("foo Foo"), "highlight": h]), 2)
        XCTAssertEqual(TextHighlight.nodeCount("Row", props: ["title": .string("foo"), "subtitle": .string("a foo"), "highlight": h]), 2)
        XCTAssertEqual(TextHighlight.nodeCount("MessageBubble", props: ["text": .string("```\nfoo\n```\n\nfoo"), "highlight": h]), 1)
        XCTAssertEqual(TextHighlight.nodeCount("Text", props: ["text": .string("foo")]), 0)
    }

    /// Typeset runs search as the normal form: inline code's padding and
    /// glue and a chip's joiners drop out, a chip is ONE barrier; the
    /// fills land on the runs' own offsets.
    func testRunsSearchAsTheNormalForm() {
        let spec = MarkdownTextSpec(size: 14, lineHeight: 20, weight: 400, family: nil)
        let style = MarkdownChipStyle()
        let inlines = [
            MarkdownInline(text: "see "),
            MarkdownInline(text: "foo()", code: true),
            MarkdownInline(text: " and "),
            MarkdownInline(text: "#A-1", link: "https://x.y/a", chip: MarkdownChip(kind: "issue", label: "foo")),
            MarkdownInline(text: "foo"),
        ]
        let runs = TextShaper.runs(inlines, spec, mono: nil, ink: .black, link: .black, codeBackground: .black, chip: style, wrap: 400)
        let (text, map) = HighlightPainter.searchText(of: runs)
        XCTAssertEqual(Array(text.unicodeScalars), Array("see foo() and \u{FFFC}foo".unicodeScalars))
        XCTAssertEqual(map.count, (text as NSString).length)
        let marked = HighlightPainter.marked(runs, query: "foo", current: 1, match: .black, currentFill: .white)
        XCTAssertEqual(marked.count, 2, "the chip's label never matches")
        var filled: [String] = []
        marked.text.enumerateAttribute(.backgroundColor, in: NSRange(location: 0, length: marked.text.length)) { v, r, _ in
            if v != nil { filled.append((marked.text.string as NSString).substring(with: r)) }
        }
        XCTAssertEqual(filled.filter { !$0.isEmpty }, ["foo", "foo"])
        XCTAssertEqual(HighlightPainter.marked(runs, query: "zebra", current: 0, match: .black, currentFill: .white).count, 0)
    }

    /// A Text's matches land on its shown lines: wrapped, a clamp's joined
    /// line and a middle cut.
    func testLineMarks() {
        let spec = TextHighlightSpec(query: "foo bar", current: 1)
        let text = "a foo bar foo bar"
        // Wrapped after "a foo " and "bar foo " (the spaces hang).
        let marks = HighlightPainter.lineMarks(text, lines: ["a foo", "bar foo", "bar"], spec: spec)
        XCTAssertEqual(marks.map { $0.map(\.range) }, [[NSRange(location: 2, length: 3)], [NSRange(location: 0, length: 3), NSRange(location: 4, length: 3)], [NSRange(location: 0, length: 3)]])
        XCTAssertEqual(marks.map { $0.map(\.current) }, [[false], [false, true], [true]])
        // One clamped line: a hard break shows as a space.
        let one = HighlightPainter.lineMarks("a foo bar\nfoo bar", lines: ["a foo bar foo bar"], spec: spec)
        XCTAssertEqual(one.map { $0.map(\.range) }, [[NSRange(location: 2, length: 7), NSRange(location: 10, length: 7)]])
        // A middle cut: the start and the end map, the `…` does not.
        let cut = HighlightPainter.lineMarks("foo bar baz foo bar", lines: ["foo b…o bar"], spec: TextHighlightSpec(query: "foo bar"), middle: "foo bar baz foo bar")
        XCTAssertEqual(cut.map { $0.map(\.range) }, [[NSRange(location: 0, length: 5), NSRange(location: 6, length: 5)]])
    }
}
