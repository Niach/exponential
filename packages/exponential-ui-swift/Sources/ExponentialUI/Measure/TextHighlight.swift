import Foundation
import ExponentialUIPrimitives

/// #EDE-95 (`fixtures/text-highlight.json`): genUI's search marks (Text,
/// Markdown, MessageBubble, Row `highlight`), the rule
/// packages/exponential-ui/src/highlight.ts implements. A match = the query
/// found LITERALLY in the text after folding both per Unicode scalar:
/// canonical decomposition (NFD), lower case (locale-independent), the
/// combining marks dropped (U+0300–036F, 1AB0–1AFF, 1DC0–1DFF, 20D0–20FF,
/// FE20–FE2F); `ß` stays `ß`. Matches run left to right and never overlap;
/// one ends after the scalars its last folded character came from plus the
/// combining marks right after them. A query folding to nothing matches
/// nothing; U+FFFC (a chip) never matches. Ranges are UTF-16 offsets (the
/// fixture's, `NSString`'s).
public struct TextHighlightRange: Equatable, Hashable, Sendable {
    /// UTF-16 offset of the first unit.
    public var start: Int
    /// UTF-16 offset after the last unit (exclusive).
    public var end: Int

    public init(start: Int, end: Int) {
        self.start = start
        self.end = end
    }

    public var nsRange: NSRange { NSRange(location: start, length: end - start) }

    /// The range as `String` indices into `text` (nil = outside it or
    /// splitting a surrogate pair).
    public func range(in text: String) -> Range<String.Index>? {
        Range(nsRange, in: text)
    }
}

/// #EDE-95: the `highlight` prop (catalog defs `highlight`): `query`, the
/// 0-based `current` match among the surface's, `before` = text whose
/// matches number BEFORE this node's (a Row subtitle gets its title).
public struct TextHighlightSpec: Equatable, Hashable, Sendable {
    public var query: String
    public var current: Int?
    public var before: String?

    public init(query: String, current: Int? = nil, before: String? = nil) {
        self.query = query
        self.current = current
        self.before = before
    }

    /// The prop as a spec: an object with a string `query` that folds to
    /// something (else nil); `current` a finite number (floored), `before`
    /// a non-empty string.
    public init?(_ value: JSONValue?) {
        guard case let .object(o)? = value, case let .string(query)? = o["query"], !TextHighlight.foldQuery(query).isEmpty else { return nil }
        self.query = query
        if case let .number(n)? = o["current"], n.isFinite {
            self.current = Int(max(min(n.rounded(.down), 1e15), -1e15))
        } else {
            self.current = nil
        }
        if case let .string(before)? = o["before"], !before.isEmpty {
            self.before = before
        } else {
            self.before = nil
        }
    }

    /// The index among THIS node's matches painted current (none = -1):
    /// `current` minus the matches of `before` (`highlightLocalCurrent`).
    public var localCurrent: Int {
        guard let current, current >= 0 else { return -1 }
        let local = current - (before.map { TextHighlight.count($0, query: query) } ?? 0)
        return local >= 0 ? local : -1
    }
}

public enum TextHighlight {
    /// A chip (or any piece that never matches) inside a searched text.
    public static let barrier: Unicode.Scalar = "\u{FFFC}"

    static func isMark(_ v: UInt32) -> Bool {
        (0x300...0x36F).contains(v) || (0x1AB0...0x1AFF).contains(v) || (0x1DC0...0x1DFF).contains(v) || (0x20D0...0x20FF).contains(v) || (0xFE20...0xFE2F).contains(v)
    }

    /// One scalar folded: NFD, lower case, marks dropped.
    static func foldScalar(_ s: Unicode.Scalar) -> [UInt32] {
        let v = s.value
        if v < 0x80 { return [(0x41...0x5A).contains(v) ? v + 0x20 : v] }
        if isMark(v) { return [] }
        let folded = String(Character(s)).decomposedStringWithCanonicalMapping.lowercased()
        return folded.unicodeScalars.map(\.value).filter { !isMark($0) }
    }

    /// The text's scalars, the UTF-16 offset of each (+ the end), and the
    /// folded characters with the scalar index each came from.
    private struct Folded {
        var scalars: [UInt32] = []
        var utf16: [Int] = [0]
        var chars: [UInt32] = []
        var from: [Int] = []
    }

    private static func fold(_ text: String) -> Folded {
        var f = Folded()
        for (i, s) in text.unicodeScalars.enumerated() {
            f.scalars.append(s.value)
            f.utf16.append(f.utf16[i] + (s.value > 0xFFFF ? 2 : 1))
            for c in foldScalar(s) {
                f.chars.append(c)
                f.from.append(i)
            }
        }
        return f
    }

    /// The query as folded characters (a barrier never matches).
    static func foldQuery(_ query: String) -> [UInt32] {
        var out: [UInt32] = []
        for s in query.unicodeScalars {
            out += foldScalar(s).filter { $0 != barrier.value }
        }
        return out
    }

    /// Every match of `query` in `text`, left to right, never overlapping
    /// (UTF-16 offsets, `highlightRanges`).
    public static func ranges(_ text: String, query: String) -> [TextHighlightRange] {
        let q = foldQuery(query)
        if q.isEmpty || text.isEmpty { return [] }
        let f = fold(text)
        var out: [TextHighlightRange] = []
        var i = 0
        while i + q.count <= f.chars.count {
            var hit = true
            for k in 0..<q.count where f.chars[i + k] != q[k] {
                hit = false
                break
            }
            if !hit {
                i += 1
                continue
            }
            // The combining marks right after the match (folded to nothing) join it.
            var end = f.from[i + q.count - 1] + 1
            while end < f.scalars.count, isMark(f.scalars[end]) { end += 1 }
            out.append(TextHighlightRange(start: f.utf16[f.from[i]], end: f.utf16[end]))
            i += q.count
        }
        return out
    }

    /// How many matches `query` has in `text` (`highlightCount`).
    public static func count(_ text: String, query: String) -> Int {
        ranges(text, query: query).count
    }

    /// The text a run list searches as: its runs joined, a chip as ONE
    /// `barrier` (chips stay whole).
    public static func runsText(_ inlines: [MarkdownInline]) -> String {
        inlines.filter { !$0.text.isEmpty }.map { $0.chip != nil ? String(barrier) : $0.text }.joined()
    }

    /// The texts a Markdown searches, one per block of the markdown-render
    /// normal form in document order (`markdownHighlightTexts`): a
    /// paragraph, heading, list item or quote = its runs joined; each table
    /// cell, the header first, row by row; never a code block, image,
    /// attachment or rule. A link = its label (the inline's text).
    public static func markdownTexts(_ blocks: [MarkdownBlock]) -> [String] {
        var out: [String] = []
        for block in blocks {
            switch block.kind {
            case .paragraph, .heading, .listItem, .quote:
                out.append(runsText(block.inlines))
            case let .table(header, rows):
                for cell in header { out.append(runsText(cell)) }
                for row in rows { for cell in row { out.append(runsText(cell)) } }
            case .codeBlock, .rule, .image, .attachment:
                break
            }
        }
        return out
    }

    /// A Markdown's matches over parsed `blocks` (what a host numbers).
    public static func markdownCount(_ blocks: [MarkdownBlock], query: String) -> Int {
        markdownTexts(blocks).reduce(0) { $0 + count($1, query: query) }
    }

    /// A Markdown text's matches (`markdownHighlightCount`, the parser's blocks).
    public static func markdownCount(_ text: String, query: String) -> Int {
        markdownCount(Markdown.parse(text), query: query)
    }

    /// A node's matches for its own `highlight` (0 without one): Text = its
    /// text, Markdown and MessageBubble = the markdown, Row = title +
    /// subtitle (`nodeHighlightCount`).
    public static func nodeCount(_ component: String, props: Props) -> Int {
        guard let spec = TextHighlightSpec(props["highlight"]) else { return 0 }
        switch component {
        case "Text": return count(props.str("text"), query: spec.query)
        case "Markdown", "MessageBubble": return markdownCount(props.str("text"), query: spec.query)
        case "Row": return count(props.str("title"), query: spec.query) + count(props.str("subtitle"), query: spec.query)
        default: return 0
        }
    }
}
