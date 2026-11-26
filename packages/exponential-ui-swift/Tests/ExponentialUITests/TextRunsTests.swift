import XCTest
import SwiftUI
@testable import ExponentialUI

/// Round 4 group B: `fixtures/text-runs.json` (TS, the Rust core and
/// Compose replay the same cases) through `TextRuns`, and a Text with runs
/// or a drop cap measured as the styled string it paints.
@MainActor
final class TextRunsTests: XCTestCase {
    override func setUp() async throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
    }

    private func run(_ v: JSONValue) -> TextRun {
        TextRun(text: v["text"]?.string ?? "", weight: v["weight"]?.string, tone: v["tone"]?.string, mark: v["mark"]?.string, link: v["link"]?.string)
    }

    func testTheDropCapNumbers() throws {
        let d = try XCTUnwrap(try Fixtures.json("text-runs.json")["dropCap"])
        XCTAssertEqual(d["min"]?.number, Double(TextRuns.capMin))
        XCTAssertEqual(d["max"]?.number, Double(TextRuns.capMax))
        XCTAssertEqual(d["lines"]?.number, Double(TextRuns.capLines))
        XCTAssertEqual(d["ratio"]?.number, Double(TextRuns.capRatio))
        XCTAssertEqual(d["gap"]?.string, "$spacing.xs")
    }

    func testTheNormalCasesReplay() throws {
        let cases = try Fixtures.json("text-runs.json")["normalCases"]?.array ?? []
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"]?.string ?? c.json
            let out = TextRuns.normalize(c["runs"])
            if case .null? = c["out"] {
                XCTAssertNil(out, name)
                continue
            }
            XCTAssertEqual(out, (c["out"]?.array ?? []).map(run), name)
            XCTAssertEqual(out.map(TextRuns.text), c["text"]?.string, name)
        }
    }

    func testTheLinesCasesReplay() throws {
        let cases = try Fixtures.json("text-runs.json")["linesCases"]?.array ?? []
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            XCTAssertEqual(TextRuns.dropCapLines(c["props"]?.object ?? [:]), Int(c["lines"]?.number ?? -1), c["name"]?.string ?? c.json)
        }
    }

    func testTheSplitCasesReplay() throws {
        let cases = try Fixtures.json("text-runs.json")["splitCases"]?.array ?? []
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"]?.string ?? c.json
            let split = TextRuns.dropCapSplit((c["runs"]?.array ?? []).map(run))
            guard let want = c["split"], case .object = want else {
                XCTAssertNil(split, name)
                continue
            }
            let got = try XCTUnwrap(split, name)
            // Compared scalar by scalar (Swift's `==` is canonical equivalence).
            XCTAssertEqual(Array(got.cap.text.unicodeScalars), Array((want["cap"]?["text"]?.string ?? "").unicodeScalars), name)
            XCTAssertEqual(got.cap, run(want["cap"] ?? .null), name)
            XCTAssertEqual(got.rest, (want["rest"]?.array ?? []).map(run), name)
            XCTAssertEqual(got.rest.map { Array($0.text.unicodeScalars) }, (want["rest"]?.array ?? []).map { Array(($0["text"]?.string ?? "").unicodeScalars) }, name)
        }
    }

    func testTheSizeCasesReplay() throws {
        let cases = try Fixtures.json("text-runs.json")["sizeCases"]?.array ?? []
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let fs = CGFloat(c["fontSize"]?.number ?? 0), lh = CGFloat(c["lineHeight"]?.number ?? 0)
            let lines = Int(c["lines"]?.number ?? 0)
            XCTAssertEqual(TextRuns.dropCapSize(fontSize: fs, lineHeight: lh, lines: lines), CGFloat(c["size"]?.number ?? -1), c.json)
            let look = TextRunsLook(style: TextStyle(fontSize: fs, fontWeight: 400, lineHeight: lh, fontFamily: nil))
            XCTAssertEqual(look.capStyle(TextRun(text: "O"), lines: lines).lineHeight, CGFloat(c["box"]?.number ?? -1), c.json)
        }
    }

    // MARK: measure = paint

    private func model(_ texts: [JSONValue]) throws -> SurfaceModel {
        let m = try makeModel("runs", theme: "exponential")
        let children = texts.map { JSONValue.object(["id": $0["id"] ?? .null, "component": .string("Text"), "props": $0["props"] ?? .null, "style": $0["style"] ?? .object([:])]) }
        try m.setNested(json: JSONValue.object(["id": .string("col"), "component": .string("Stack"), "props": .object(["align": .string("start")]), "children": .array(children)]).json)
        return m
    }

    private func text(_ id: String, _ props: [String: JSONValue], style: [String: JSONValue] = [:]) -> JSONValue {
        .object(["id": .string(id), "props": .object(props), "style": .object(style)])
    }

    /// The core hands the natives the normal form, the joined text and the
    /// drop cap's lines.
    func testTheCoreNormalisesTheProps() throws {
        let m = try model([
            text("a", ["runs": .array([.string("Hel"), .string("lo"), .object(["text": .string("!"), "weight": .string("bold")])]), "text": .string("ignored")]),
            text("b", ["dropCap": .bool(true), "text": .string("Once")]),
            text("c", ["dropCap": .number(3), "lines": .number(2), "text": .string("Once")]),
        ])
        let a = try XCTUnwrap(m.node(try XCTUnwrap(m.index(of: "a"))))
        XCTAssertEqual(a.props.str("text"), "Hello!")
        XCTAssertEqual(TextRuns.normalize(a.props["runs"]), [TextRun(text: "Hello"), TextRun(text: "!", weight: "bold")])
        let b = try XCTUnwrap(m.node(try XCTUnwrap(m.index(of: "b"))))
        XCTAssertEqual(b.props["dropCap"]?.number, 3)
        let c = try XCTUnwrap(m.node(try XCTUnwrap(m.index(of: "c"))))
        XCTAssertNil(c.props["dropCap"])
    }

    /// Weights and inline code change widths: a bold run is wider than the
    /// same text plain; inline code reserves its padding.
    func testRunsMeasureAsTheirStyledString() throws {
        let m = try model([
            text("plain", ["text": .string("Ship it today")]),
            text("bold", ["runs": .array([.object(["text": .string("Ship it today"), "weight": .string("bold")])])]),
            text("code", ["runs": .array([.object(["text": .string("Ship it today"), "mark": .string("code")])])]),
            text("same", ["runs": .array([.string("Ship it today")])]),
        ])
        func w(_ id: String) throws -> CGFloat { m.frame(try XCTUnwrap(m.index(of: id))).width }
        XCTAssertGreaterThan(try w("bold"), try w("plain") + 1)
        XCTAssertGreaterThan(try w("code"), try w("plain"))
        XCTAssertEqual(try w("same"), try w("plain"), accuracy: 0.05)
    }

    /// A drop cap spans its lines: the box is `lines × lineHeight` tall, the
    /// rest wraps beside it, and a narrow Text is never shorter than the box.
    func testADropCapSpansItsLines() throws {
        let long = "Once the build turns green, the merge train picks the pull request up, rebases it on master and lands it within minutes, so nobody waits on a button."
        let m = try model([
            text("cap", ["dropCap": .number(3), "text": .string(long)], style: ["width": .number(320)]),
            text("plain", ["text": .string(long)], style: ["width": .number(320)]),
            text("short", ["dropCap": .number(3), "text": .string("Hi")]),
        ])
        let i = try XCTUnwrap(m.index(of: "cap"))
        let lh = m.textStyle(i).lineHeight
        let cap = m.frame(i), plain = m.frame(try XCTUnwrap(m.index(of: "plain")))
        XCTAssertGreaterThanOrEqual(cap.height, 3 * lh)
        XCTAssertGreaterThanOrEqual(cap.height, plain.height, "the lines beside the cap are shorter")
        XCTAssertEqual(cap.height.truncatingRemainder(dividingBy: lh), 0, accuracy: 0.01)
        let short = m.frame(try XCTUnwrap(m.index(of: "short")))
        XCTAssertEqual(short.height, 3 * lh, accuracy: 0.01)
        // max-content = the cap box + the rest's max-content.
        let ts = m.textStyle(try XCTUnwrap(m.index(of: "short")))
        let look = TextRunsLook(style: ts, gap: m.spacing("xs"))
        let capW = TextShaper.lineWidth(look.capAttributed(TextRun(text: "H"), lines: 3)) + m.spacing("xs")
        XCTAssertEqual(short.width, capW + TextShaper.width("i", ts), accuracy: 0.1)
    }

    /// The layout: the first `lines` rows sit beside the cap at the
    /// narrower width, the rest at the full width; a word that does not fit
    /// beside the cap starts below the box.
    func testTheLayoutWrapsBesideThenBelow() {
        let ts = TextStyle(fontSize: 14, fontWeight: 400, lineHeight: 20, fontFamily: nil)
        let look = TextRunsLook(style: ts)
        let rest = look.attributed([TextRun(text: "nce upon a time there was a little prince who lived on a planet")])
        let cap = look.capAttributed(TextRun(text: "O"), lines: 3)
        let layout = TextRunsLayout(text: rest, cap: cap, capLines: 3, gap: 4, lineHeight: 20, width: 160)
        XCTAssertGreaterThan(layout.lines.count, 3)
        for line in layout.lines {
            XCTAssertEqual(line.indent, line.row < 3 ? layout.capBox.width : 0, "row \(line.row)")
            XCTAssertEqual(line.width, line.row < 3 ? 160 - layout.capBox.width : 160, "row \(line.row)")
        }
        XCTAssertEqual(layout.capBox.height, 60)
        // A word wider than the room beside the cap goes below it.
        let wide = look.attributed([TextRun(text: "Supercalifragilistic word")])
        let below = TextRunsLayout(text: wide, cap: cap, capLines: 3, gap: 4, lineHeight: 20, width: layout.capBox.width + TextShaper.lineWidth(wide.attributedSubstring(from: NSRange(location: 0, length: 10))))
        XCTAssertEqual(below.lines.first?.row, 3)
        XCTAssertEqual(below.lines.first?.indent, 0)
    }

    /// The marks' paint: weight, tone colour, the highlight fill,
    /// underline, strike, a policed link (denied = plain).
    func testTheRunsPaint() throws {
        let m = try makeModel("paint", theme: "exponential")
        let ts = TextStyle(fontSize: 14, fontWeight: 400, lineHeight: 20, fontFamily: nil)
        var look = TextRunsLook(style: ts)
        look.weight = { m.theme?.fontWeight($0) }
        look.tone = { $0 == "success" ? .green : nil }
        look.match = .yellow
        look.href = { $0.hasPrefix("https://") ? $0 : nil }
        let a = look.attributed([
            TextRun(text: "b", weight: "bold"), TextRun(text: "t", tone: "success"), TextRun(text: "h", mark: "highlight"),
            TextRun(text: "u", mark: "underline"), TextRun(text: "s", mark: "strike"), TextRun(text: "l", link: "https://x.example"),
            TextRun(text: "d", link: "javascript:alert(1)"),
        ])
        XCTAssertEqual(a.string, "bthusld")
        func attr(_ i: Int, _ k: NSAttributedString.Key) -> Any? { a.attribute(k, at: i, effectiveRange: nil) }
        XCTAssertEqual(m.theme?.fontWeight("bold"), 700)
        XCTAssertNotEqual((attr(0, .font) as? PlatformFont)?.fontName, (attr(1, .font) as? PlatformFont)?.fontName)
        XCTAssertEqual(attr(1, .foregroundColor) as? PlatformColor, .green)
        XCTAssertEqual(attr(2, .backgroundColor) as? PlatformColor, .yellow)
        XCTAssertNotNil(attr(3, .underlineStyle))
        XCTAssertNotNil(attr(4, .strikethroughStyle))
        XCTAssertEqual(attr(5, TextShaper.linkKey) as? String, "https://x.example")
        XCTAssertNotNil(attr(5, .underlineStyle))
        XCTAssertNil(attr(6, TextShaper.linkKey))
        XCTAssertNil(attr(6, .underlineStyle))
    }
}
