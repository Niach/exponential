import Foundation
import CoreText
import ExponentialUIPrimitives

#if canImport(UIKit)
import UIKit
#elseif canImport(AppKit)
import AppKit
#endif

/// Round 4 group B (`fixtures/text-runs.json`): a Text's `runs` (inline
/// spans) and its `dropCap`, the rule packages/exponential-ui/src/text-runs.ts
/// implements (eui-core `text_runs`, Kotlin `TextRuns` replay it too).
/// `runs` set (an array) REPLACES `text`: one paragraph of spans wrapping
/// together. A run = a string (`{text}`) or `{text, weight?, tone?, mark?,
/// link?}`; `text` a string, number or boolean (its display string); an
/// unknown weight / tone / mark or an empty link drops the attribute; empty
/// runs drop; adjacent runs with equal attributes merge. The core hands the
/// painter the normal form (`props.runs`), the joined text (`props.text`)
/// and `dropCap` as lines, so this only re-normalises what it already is.
public struct TextRun: Equatable, Hashable, Sendable {
    public var text: String
    public var weight: String?
    public var tone: String?
    public var mark: String?
    public var link: String?

    public init(text: String, weight: String? = nil, tone: String? = nil, mark: String? = nil, link: String? = nil) {
        self.text = text
        self.weight = weight
        self.tone = tone
        self.mark = mark
        self.link = link
    }

    func same(_ o: TextRun) -> Bool { weight == o.weight && tone == o.tone && mark == o.mark && link == o.link }

    /// `$type.weight.<w>` when the theme has none (unset = nil).
    var fallbackWeight: Int? {
        switch weight {
        case nil: nil
        case "medium": 500
        case "semibold": 600
        case "bold": 700
        default: 400
        }
    }
}

public enum TextRuns {
    public static let weights: Set<String> = ["regular", "medium", "semibold", "bold"]
    public static let marks: Set<String> = ["highlight", "code", "strike", "underline"]
    public static let tones: Set<String> = ["neutral", "primary", "success", "warning", "danger", "info"]

    /// The drop cap's fixed numbers (the fixture's `dropCap`).
    public static let capMin = 2
    public static let capMax = 4
    public static let capLines = 3
    public static let capRatio: CGFloat = 1.375

    private static func runText(_ v: JSONValue?) -> String? {
        switch v {
        case let .string(s)?: s
        case let .number(n)?: JSONValue.number(n).displayText
        case let .bool(b)?: b ? "true" : "false"
        default: nil
        }
    }

    /// `runs` normalised (nil = not an array: the Text shows `text`).
    public static func normalize(_ value: JSONValue?) -> [TextRun]? {
        guard case let .array(entries)? = value else { return nil }
        var out: [TextRun] = []
        for entry in entries {
            var run: TextRun?
            switch entry {
            case let .string(s):
                run = TextRun(text: s)
            case let .object(o):
                if let text = runText(o["text"]) {
                    var r = TextRun(text: text)
                    if case let .string(w)? = o["weight"], weights.contains(w) { r.weight = w }
                    if case let .string(t)? = o["tone"], tones.contains(t) { r.tone = t }
                    if case let .string(m)? = o["mark"], marks.contains(m) { r.mark = m }
                    if case let .string(l)? = o["link"], !l.isEmpty { r.link = l }
                    run = r
                }
            default:
                break
            }
            guard let run, !run.text.isEmpty else { continue }
            if let last = out.last, last.same(run) {
                out[out.count - 1].text += run.text
            } else {
                out.append(run)
            }
        }
        return out
    }

    /// The runs' text joined: what measures, searches and is announced.
    public static func text(_ runs: [TextRun]) -> String { runs.map(\.text).joined() }

    /// `dropCap` as lines (0 = none) for a Text's props.
    public static func dropCapLines(_ props: Props) -> Int {
        if props["shimmer"]?.bool == true { return 0 }
        if case let .string(t)? = props["truncate"], t == "end" || t == "middle" { return 0 }
        if case let .number(l)? = props["lines"], l > 0 { return 0 }
        switch props["dropCap"] {
        case .bool(true)?:
            return capLines
        case let .number(n)? where n.isFinite:
            let r = n.rounded(.toNearestOrAwayFromZero)
            return r < Double(capMin) ? 0 : Int(min(r, Double(capMax)))
        default:
            return 0
        }
    }

    static func isJoiner(_ v: UInt32) -> Bool {
        TextHighlight.isMark(v) || v == 0xFE0E || v == 0xFE0F
    }

    /// The drop cap off the runs: leading ASCII spaces, tabs and line
    /// breaks drop; the cap = the first scalar plus the combining marks and
    /// variation selectors right after it, in the weight and tone of the run
    /// holding it (no mark, no link); `rest` = the runs after it. Nil = no
    /// letter.
    public static func dropCapSplit(_ runs: [TextRun]) -> (cap: TextRun, rest: [TextRun])? {
        var rest = runs
        while let first = rest.first {
            let trimmed = String(first.text.unicodeScalars.drop { $0 == " " || $0 == "\t" || $0 == "\n" || $0 == "\r" })
            if !trimmed.isEmpty {
                rest[0].text = trimmed
                break
            }
            rest.removeFirst()
        }
        guard let first = rest.first else { return nil }
        let scalars = Array(first.text.unicodeScalars)
        var n = 1
        while n < scalars.count, isJoiner(scalars[n].value) { n += 1 }
        var capText = String.UnicodeScalarView()
        capText.append(contentsOf: scalars[0..<n])
        let cap = TextRun(text: String(capText), weight: first.weight, tone: first.tone)
        var tail = String.UnicodeScalarView()
        tail.append(contentsOf: scalars[n...])
        if tail.isEmpty {
            rest.removeFirst()
        } else {
            rest[0].text = String(tail)
        }
        return (cap, rest)
    }

    /// The cap's font size: `fontSize + (lines - 1) × lineHeight × 1.375`,
    /// rounded to a whole px (CSS rounds halves up).
    public static func dropCapSize(fontSize: CGFloat, lineHeight: CGFloat, lines: Int) -> CGFloat {
        (fontSize + CGFloat(max(lines - 1, 0)) * lineHeight * capRatio).rounded(.toNearestOrAwayFromZero)
    }

    /// A Text's spans: its `runs`, else `text` as one run, whether it had
    /// runs, and the drop cap lines.
    public static func spans(_ props: Props) -> (runs: [TextRun], rich: Bool, dropCap: Int) {
        let runs = normalize(props["runs"])
        let text = runText(props["text"]) ?? ""
        return (runs ?? (text.isEmpty ? [] : [TextRun(text: text)]), runs != nil, dropCapLines(props))
    }

    /// Does this Text paint through the runs painter (runs not under
    /// `shimmer`, or a drop cap)?
    public static func paintsRich(_ props: Props) -> Bool {
        let s = spans(props)
        return (s.rich && props["shimmer"]?.bool != true) || s.dropCap > 0
    }
}

/// One run's own paint (`TextShaper.runs` `looks`): its weight, colour,
/// underline and the `highlight` mark's fill.
public struct TextRunLook {
    public var weight: Int?
    public var color: PlatformColor?
    public var underline = false
    public var fill: PlatformColor?

    func apply(_ attrs: inout [NSAttributedString.Key: Any]) {
        if underline { attrs[.underlineStyle] = NSUnderlineStyle.single.rawValue }
        if let fill { attrs[.backgroundColor] = fill }
    }
}

/// What a Text's runs paint with, resolved once per leaf: the Text's type,
/// the `Markdown/code` recipe (inline code), the theme's weights, the tone
/// colours, the URL policy and the `match` fill. The measurer builds it
/// with no colours (geometry only).
struct TextRunsLook {
    var style: TextStyle
    var align: NSTextAlignment = .natural
    var mono: String?
    var chip: MarkdownChipStyle = MarkdownChipStyle()
    var codeBackground: PlatformColor?
    var ink: PlatformColor = .black
    var weight: (String) -> Int? = { _ in nil }
    var tone: (String) -> PlatformColor? = { _ in nil }
    var href: (String) -> String? = { $0 }
    var match: PlatformColor?
    /// `$spacing.xs`: the drop cap's gap.
    var gap: CGFloat = 4

    /// The runs as ONE styled string (the Markdown inline machinery).
    func attributed(_ runs: [TextRun]) -> NSAttributedString {
        let spec = MarkdownTextSpec(size: style.fontSize, lineHeight: style.lineHeight, weight: style.fontWeight, family: style.fontFamily)
        var inlines: [MarkdownInline] = []
        var looks: [TextRunLook] = []
        for run in runs {
            let link = run.link.flatMap(href)
            inlines.append(MarkdownInline(text: style.shown(run.text), code: run.mark == "code", strike: run.mark == "strike", link: link))
            looks.append(TextRunLook(
                weight: run.weight.map { weight($0) ?? run.fallbackWeight ?? 400 },
                color: run.tone.flatMap(tone),
                underline: run.mark == "underline",
                fill: run.mark == "highlight" ? match : nil
            ))
        }
        return TextShaper.runs(inlines, spec, mono: mono, ink: ink, link: ink, codeBackground: codeBackground, align: align, chip: chip, looks: looks, base: style)
    }

    /// The cap's type: the Text's family at the cap size, the run's weight.
    func capStyle(_ cap: TextRun, lines: Int) -> TextStyle {
        var ts = style
        ts.fontSize = TextRuns.dropCapSize(fontSize: style.fontSize, lineHeight: style.lineHeight, lines: lines)
        ts.lineHeight = CGFloat(lines) * style.lineHeight
        if let w = cap.weight { ts.fontWeight = weight(w) ?? cap.fallbackWeight ?? ts.fontWeight }
        return ts
    }

    func capAttributed(_ cap: TextRun, lines: Int) -> NSAttributedString {
        let ts = capStyle(cap, lines: lines)
        var attrs = TextShaper.attributes(ts, align: .natural, lineBreak: .byClipping)
        attrs[.foregroundColor] = cap.tone.flatMap(tone) ?? ink
        // CSS half-leading in the box, NEGATIVE when the cap's glyph box is
        // taller than the box (the baseline then sits lower than TextKit's).
        if let font = attrs[.font] as? PlatformFont {
            attrs[.init("ExponentialUI.halfLeading")] = (ts.lineHeight - (font.ascender - font.descender)) / 2
        }
        return NSAttributedString(string: style.shown(cap.text), attributes: attrs)
    }
}

/// A Text's runs laid out at one width: each painted line (its range in
/// `text`, its row in line boxes, its inline-start indent and width), and
/// the drop cap's box.
final class TextRunsLayout {
    struct Line {
        var range: NSRange
        var row: Int
        var indent: CGFloat
        var width: CGFloat
    }

    let text: NSAttributedString
    let cap: NSAttributedString?
    /// The cap's box: `lines × lineHeight` tall, its advance + gap wide.
    let capBox: CGSize
    let capLines: Int
    let lineHeight: CGFloat
    let width: CGFloat
    let lines: [Line]
    let typesetter: CTTypesetter

    var rows: Int { max(lines.map { $0.row + 1 }.max() ?? 0, capLines) }
    var height: CGFloat { CGFloat(max(rows, text.length > 0 || cap != nil ? 1 : 0)) * lineHeight }

    init(text: NSAttributedString, cap: NSAttributedString?, capLines: Int, gap: CGFloat, lineHeight: CGFloat, width: CGFloat) {
        self.text = text
        self.cap = cap
        self.lineHeight = lineHeight
        self.width = width
        self.capLines = cap == nil ? 0 : capLines
        let advance = cap.map { TextShaper.lineWidth($0) } ?? 0
        capBox = cap == nil ? .zero : CGSize(width: advance + gap, height: CGFloat(capLines) * lineHeight)
        typesetter = CTTypesetterCreateWithAttributedString(text)
        lines = Self.breakLines(text, typesetter: typesetter, width: width, capBox: capBox.width, capLines: self.capLines)
    }

    /// The text's break opportunities (UAX #14 segment starts, UTF-16).
    static func segmentStarts(_ s: NSString) -> [Int] {
        guard s.length > 0 else { return [] }
        let tokenizer = CFStringTokenizerCreate(nil, s, CFRange(location: 0, length: s.length), kCFStringTokenizerUnitLineBreak, nil)
        var starts: [Int] = []
        while CFStringTokenizerAdvanceToNextToken(tokenizer).rawValue != 0 {
            starts.append(CFStringTokenizerGetCurrentTokenRange(tokenizer).location)
        }
        if starts.first != 0 { starts.insert(0, at: 0) }
        return starts
    }

    /// Greedy lines (CoreText's typesetter, hard breaks honoured): the
    /// first `capLines` rows at `width − capBox` beside the cap, then the
    /// full width; a line whose first unbreakable segment does not fit
    /// beside the cap starts below the box (the CSS float).
    static func breakLines(_ text: NSAttributedString, typesetter: CTTypesetter, width: CGFloat, capBox: CGFloat, capLines: Int) -> [Line] {
        let ns = text.string as NSString
        guard ns.length > 0 else { return [] }
        let starts = capLines > 0 ? segmentStarts(ns) : []
        func firstSegmentWidth(_ pos: Int, _ end: Int) -> CGFloat {
            let next = starts.first { $0 > pos } ?? end
            let len = min(next, end) - pos
            guard len > 0 else { return 0 }
            return TextShaper.lineWidth(text.attributedSubstring(from: NSRange(location: pos, length: len)))
        }
        var out: [Line] = []
        var row = 0
        var start = 0
        while start <= ns.length {
            let nl = ns.range(of: "\n", options: [], range: NSRange(location: start, length: ns.length - start))
            let end = nl.location == NSNotFound ? ns.length : nl.location
            var pos = start
            repeat {
                var beside = row < capLines
                if beside, end > pos, firstSegmentWidth(pos, end) > width - capBox + 0.5 {
                    row = capLines
                    beside = false
                }
                let w = beside ? max(width - capBox, 1) : max(width, 1)
                let raw = end > pos ? max(CTTypesetterSuggestLineBreak(typesetter, pos, Double(w + 0.5)), 1) : 0
                let n = min(raw, end - pos)
                out.append(Line(range: NSRange(location: pos, length: n), row: row, indent: beside ? capBox : 0, width: w))
                pos += n
                row += 1
            } while pos < end
            if nl.location == NSNotFound { break }
            start = end + 1
        }
        return out
    }

    /// The link href under `point` (nil = none): the line's row, then the
    /// glyph at x within it (`flush` = the line's alignment in its box,
    /// `rtl` = the cap on the right).
    func link(at point: CGPoint, rtl: Bool, flush: CGFloat) -> String? {
        guard point.y >= 0 else { return nil }
        let row = Int(point.y / max(lineHeight, 1))
        guard let line = lines.first(where: { $0.row == row }), line.range.length > 0 else { return nil }
        let ct = CTTypesetterCreateLine(typesetter, CFRange(location: line.range.location, length: line.range.length))
        let lineW = CGFloat(CTLineGetTypographicBounds(ct, nil, nil, nil))
        let pen = CGFloat(CTLineGetPenOffsetForFlush(ct, flush, Double(line.width)))
        let x0 = (rtl ? width - line.indent - line.width : line.indent) + pen
        let x = point.x - x0
        guard x >= 0, x <= lineW else { return nil }
        let idx = CTLineGetStringIndexForPosition(ct, CGPoint(x: x, y: 0))
        let i = min(max(idx == kCFNotFound ? line.range.location : idx, line.range.location), NSMaxRange(line.range) - 1)
        let probe = i > line.range.location && CTLineGetOffsetForStringIndex(ct, i, nil) > x ? i - 1 : i
        return text.attribute(TextShaper.linkKey, at: probe, effectiveRange: nil) as? String
    }
}

extension TextRuns {
    /// The Text's paint parts: the runs after the drop cap (all of them
    /// without one) as one styled string, the cap's, and its lines.
    static func parts(_ props: Props, _ look: TextRunsLook) -> (text: NSAttributedString, cap: NSAttributedString?, capLines: Int) {
        let s = spans(props)
        if s.dropCap > 0, let split = dropCapSplit(s.runs) {
            return (look.attributed(split.rest), look.capAttributed(split.cap, lines: s.dropCap), s.dropCap)
        }
        return (look.attributed(s.runs), nil, 0)
    }

    /// The max-content width of a styled string: its widest hard line.
    static func maxContent(_ a: NSAttributedString) -> CGFloat {
        let ns = a.string as NSString
        var widest: CGFloat = 0
        var start = 0
        while start <= ns.length {
            let nl = ns.range(of: "\n", options: [], range: NSRange(location: start, length: ns.length - start))
            let end = nl.location == NSNotFound ? ns.length : nl.location
            if end > start { widest = max(widest, TextShaper.lineWidth(a.attributedSubstring(from: NSRange(location: start, length: end - start)))) }
            if nl.location == NSNotFound { break }
            start = end + 1
        }
        return widest
    }

    /// The min-content width: the widest unbreakable segment (spaces hanging).
    static func minContent(_ a: NSAttributedString) -> CGFloat {
        let ns = a.string as NSString
        var widest: CGFloat = 0
        var start = 0
        while start <= ns.length {
            let nl = ns.range(of: "\n", options: [], range: NSRange(location: start, length: ns.length - start))
            let end = nl.location == NSNotFound ? ns.length : nl.location
            if end > start {
                let line = a.attributedSubstring(from: NSRange(location: start, length: end - start))
                let starts = TextRunsLayout.segmentStarts(line.string as NSString)
                for (i, s) in starts.enumerated() {
                    let e = i + 1 < starts.count ? starts[i + 1] : line.length
                    if e > s { widest = max(widest, TextShaper.lineWidth(line.attributedSubstring(from: NSRange(location: s, length: e - s)))) }
                }
            }
            if nl.location == NSNotFound { break }
            start = end + 1
        }
        return widest
    }

    /// A Text with runs or a drop cap at an inner wrap width (nil =
    /// max-content, 0 = min-content), `clamp` = its `lines` (a drop cap is
    /// never clamped): measured as the styled string it paints.
    static func measure(_ props: Props, _ look: TextRunsLook, wrap: CGFloat?, lines clampLines: Int?) -> CGSize {
        let p = parts(props, look)
        let lh = look.style.lineHeight
        if p.cap == nil, !p.text.string.contains(where: { !($0 == " " || $0 == "\t") }) { return .zero }
        let clamp = clampLines.flatMap { $0 > 0 ? $0 : nil }
        if let cap = p.cap {
            let capW = TextShaper.lineWidth(cap) + look.gap
            let maxW = capW + maxContent(p.text)
            let minW = max(capW, minContent(p.text))
            let w: CGFloat
            switch wrap {
            case .none: w = maxW
            case .some(let x) where x <= 0: w = minW
            case .some(let x): w = min(maxW, max(x, minW))
            }
            let layoutW = wrap.map { $0 > 0 ? max($0, w) : w } ?? w
            let layout = TextRunsLayout(text: p.text, cap: cap, capLines: p.capLines, gap: look.gap, lineHeight: lh, width: layoutW)
            return CGSize(width: w, height: layout.height)
        }
        func height(_ width: CGFloat?) -> CGFloat {
            let n = TextShaper.richLineCount(p.text, wrap: width)
            return lh * CGFloat(max(clamp.map { min(n, $0) } ?? n, 1))
        }
        let maxW = maxContent(p.text)
        if clamp == 1 {
            let w = wrap.map { $0 <= 0 ? maxW : min(maxW, $0) } ?? maxW
            return CGSize(width: w, height: lh)
        }
        switch wrap {
        case .none:
            return CGSize(width: maxW, height: height(nil))
        case .some(let x) where x <= 0:
            let m = minContent(p.text)
            return CGSize(width: m, height: height(m))
        case .some(let x):
            let used = min(maxW, max(x, minContent(p.text)))
            return CGSize(width: used, height: height(max(x, used)))
        }
    }
}
