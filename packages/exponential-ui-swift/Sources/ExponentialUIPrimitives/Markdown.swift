import Foundation

// The read-only GFM subset every Exponential UI renderer paints the same way
// (the gpui painter's `paint::markdown` and the React renderer's block
// model): paragraphs, ATX headings, bullet / ordered / task lists (nested
// by marker indentation), quotes, fenced code, rules, pipe tables, images
// alone on a line; inline bold, italic, strike, code, links (destinations
// balance parentheses like CommonMark, no whitespace). An image inside a
// line paints its alt text.
// No dependency: the parser is ours so the measurer and the painter share
// ONE block model, and the layout rule below is what both use.

public struct MarkdownInline: Equatable, Sendable {
    public var text: String
    public var bold = false
    public var italic = false
    public var code = false
    public var strike = false
    public var link: String?

    public init(text: String, bold: Bool = false, italic: Bool = false, code: Bool = false, strike: Bool = false, link: String? = nil) {
        self.text = text
        self.bold = bold
        self.italic = italic
        self.code = code
        self.strike = strike
        self.link = link
    }
}

public enum MarkdownBlockKind: Equatable, Sendable {
    case paragraph
    case heading(Int)
    /// `marker` = "•" or "3."; `task` = a GFM task box state.
    case listItem(marker: String, task: Bool?)
    case quote
    case codeBlock
    case table(header: [[MarkdownInline]], rows: [[[MarkdownInline]]])
    case rule
    /// `![alt](src)` alone on a line: painted through the host's media
    /// request (a denied src becomes a paragraph of its alt text, see
    /// `resolveImages`; a failed load paints the alt text in the box).
    case image(src: String, alt: String)
}

public struct MarkdownBlock: Equatable, Sendable {
    public var kind: MarkdownBlockKind
    public var inlines: [MarkdownInline]
    /// Consecutive list items share a group (no paragraph gap inside it).
    public var listGroup: Int?
    /// A list item's nesting level (0 = top level): indented one
    /// `listIndent` per level.
    public var depth: Int

    public init(kind: MarkdownBlockKind, inlines: [MarkdownInline], listGroup: Int? = nil, depth: Int = 0) {
        self.kind = kind
        self.inlines = inlines
        self.listGroup = listGroup
        self.depth = depth
    }
}

public enum Markdown {
    private struct Flags {
        var bold = false, italic = false, strike = false
    }

    private static func push(_ out: inout [MarkdownInline], _ text: Substring, _ f: Flags, _ link: String?) {
        if text.isEmpty { return }
        if let last = out.indices.last, !out[last].code, out[last].bold == f.bold, out[last].italic == f.italic, out[last].strike == f.strike, out[last].link == link {
            out[last].text += text
            return
        }
        out.append(MarkdownInline(text: String(text), bold: f.bold, italic: f.italic, strike: f.strike, link: link))
    }

    /// Link labels (and emphasis) nest at most this deep; a deeper `[` is
    /// text (hostile input cannot recurse without bound).
    public static let maxNesting = 32

    /// `[label](href)` at the start of `s` → (label, href, consumed
    /// characters). The label balances brackets; the destination balances
    /// parentheses (CommonMark: `[a](https://x/A_(b))` → `https://x/A_(b)`),
    /// takes `\(` / `\)` escapes and holds no whitespace. Both closers come
    /// from `scan` (built once per string), so trying a link is O(1) until
    /// it matches: `"[a](".repeat(n)` stays linear.
    static func linkAt(_ s: Substring, emptyLabel: Bool = false, scan: InlineScan? = nil) -> (label: Substring, href: String, consumed: Int)? {
        guard s.hasPrefix("[") else { return nil }
        let scan = scan ?? InlineScan(s.base)
        guard let open = scan.ordinal[s.startIndex], let limit = scan.ordinal[s.endIndex],
              let close = scan.closeBracket(open, limit: limit) else { return nil }
        let label = s[scan.index[open + 1]..<scan.index[close]]
        let paren = close + 1
        guard emptyLabel || !label.isEmpty, paren < limit, scan.chars[paren] == "(",
              let end = scan.closeParen(paren, limit: limit) else { return nil }
        var href = ""
        var j = paren + 1
        while j < end {
            let c = scan.chars[j]
            if c == "\\", j + 1 < end, scan.chars[j + 1].isPunctuation || scan.chars[j + 1].isSymbol {
                j += 1
            }
            href.append(scan.chars[j])
            j += 1
        }
        return (label, href, end - open + 1)
    }

    /// One string's bracket and parenthesis closers, found in ONE pass each
    /// (`linkAt`'s rules: a label skips `\` + any character, a destination
    /// `\` + punctuation and ends at whitespace). A closer is the first `]`
    /// / `)` after the opener at the opener's depth: the depth before each
    /// character, and the closers listed per depth.
    final class InlineScan {
        let chars: [Character]
        /// Ordinal → string index (`count` = the end index).
        let index: [String.Index]
        let ordinal: [String.Index: Int]
        private var bracketDepth: [Int] = []
        private var bracketClosers: [Int: [Int]] = [:]
        private var parenDepth: [Int] = []
        private var parenClosers: [Int: [Int]] = [:]
        private var spaces: [Int] = []

        init(_ base: String) {
            var chars: [Character] = []
            var index: [String.Index] = []
            var i = base.startIndex
            while i < base.endIndex {
                chars.append(base[i])
                index.append(i)
                i = base.index(after: i)
            }
            index.append(base.endIndex)
            self.chars = chars
            self.index = index
            var ordinal: [String.Index: Int] = [:]
            ordinal.reserveCapacity(index.count)
            for (n, idx) in index.enumerated() { ordinal[idx] = n }
            self.ordinal = ordinal
            let n = chars.count
            bracketDepth = Array(repeating: 0, count: n + 1)
            parenDepth = Array(repeating: 0, count: n + 1)
            var d = 0
            var p = 0
            while p < n {
                bracketDepth[p] = d
                if chars[p] == "\\" {
                    if p + 1 < n { bracketDepth[p + 1] = d }
                    p += 2
                    continue
                }
                if chars[p] == "[" { d += 1 } else if chars[p] == "]" {
                    bracketClosers[bracketDepth[p], default: []].append(p)
                    d -= 1
                }
                p += 1
            }
            if n < bracketDepth.count { bracketDepth[n] = d }
            d = 0
            p = 0
            while p < n {
                parenDepth[p] = d
                let c = chars[p]
                if c == "\\", p + 1 < n, chars[p + 1].isPunctuation || chars[p + 1].isSymbol {
                    parenDepth[p + 1] = d
                    p += 2
                    continue
                }
                if c.isWhitespace { spaces.append(p) } else if c == "(" { d += 1 } else if c == ")" {
                    parenClosers[parenDepth[p], default: []].append(p)
                    d -= 1
                }
                p += 1
            }
            if n < parenDepth.count { parenDepth[n] = d }
        }

        /// The first element of sorted `list` greater than `after`.
        private static func first(_ list: [Int]?, after: Int) -> Int? {
            guard let list else { return nil }
            var lo = 0, hi = list.count
            while lo < hi {
                let mid = (lo + hi) / 2
                if list[mid] > after { hi = mid } else { lo = mid + 1 }
            }
            return lo < list.count ? list[lo] : nil
        }

        /// The `]` closing the label opened at `open`, before `limit`.
        func closeBracket(_ open: Int, limit: Int) -> Int? {
            guard open + 1 <= chars.count else { return nil }
            guard let p = Self.first(bracketClosers[bracketDepth[open + 1]], after: open), p < limit else { return nil }
            return p
        }

        /// The `)` closing the destination opened at `open`, before `limit`
        /// and before any whitespace.
        func closeParen(_ open: Int, limit: Int) -> Int? {
            guard open + 1 <= chars.count else { return nil }
            guard let p = Self.first(parenClosers[parenDepth[open + 1]], after: open), p < limit else { return nil }
            if let space = Self.first(spaces, after: open), space < p { return nil }
            return p
        }
    }

    /// `![alt](src)` filling all of `line` → (src, alt).
    static func wholeImage(_ line: String) -> (src: String, alt: String)? {
        let t = Substring(line)
        guard t.hasPrefix("!["), let (alt, src, n) = linkAt(t.dropFirst(), emptyLabel: true), n + 1 == t.count else { return nil }
        return (src, plain(parseInline(String(alt))))
    }

    /// A delimited run `ddTEXTdd` at the start of `s` → (TEXT, consumed).
    private static func delimited(_ s: Substring, _ delim: String) -> (Substring, Int)? {
        guard s.hasPrefix(delim) else { return nil }
        let rest = s.dropFirst(delim.count)
        guard let end = rest.range(of: delim) else { return nil }
        let inner = rest[rest.startIndex..<end.lowerBound]
        if inner.isEmpty || inner.first!.isWhitespace { return nil }
        if delim.hasPrefix("_"), let next = rest[end.upperBound...].first, next.isLetter || next.isNumber { return nil }
        return (inner, delim.count * 2 + inner.count)
    }

    private static func parseInline(_ s: Substring, _ f: Flags, _ link: String?, _ out: inout [MarkdownInline], scan: InlineScan, depth: Int = 0) {
        let nest = depth < maxNesting
        var plainStart = s.startIndex
        var i = s.startIndex
        while i < s.endIndex {
            let rest = s[i...]
            let c = s[i]
            var handled: Int? = nil
            if c == "`" {
                let body = rest.dropFirst()
                if let end = body.firstIndex(of: "`") {
                    push(&out, s[plainStart..<i], f, link)
                    out.append(MarkdownInline(text: String(body[body.startIndex..<end]), code: true, link: link))
                    handled = body.distance(from: body.startIndex, to: end) + 2
                }
            } else if c == "!", rest.dropFirst().hasPrefix("[") {
                if let (alt, _, n) = linkAt(rest.dropFirst(), emptyLabel: true, scan: scan) {
                    // An image inside a line paints its alt text.
                    push(&out, s[plainStart..<i], f, link)
                    push(&out, alt, f, link)
                    handled = n + 1
                }
            } else if c == "[", nest {
                if let (label, href, n) = linkAt(rest, scan: scan) {
                    push(&out, s[plainStart..<i], f, link)
                    parseInline(label, f, href, &out, scan: scan, depth: depth + 1)
                    handled = n
                }
            } else if nest, c == "*" || c == "_" || c == "~" {
                let prevAlnum = i > s.startIndex && { let p = s[s.index(before: i)]; return p.isLetter || p.isNumber }()
                if !(c == "_" && prevAlnum) {
                    let double = c == "*" ? "**" : (c == "_" ? "__" : "~~")
                    if let (inner, n) = delimited(rest, double) {
                        push(&out, s[plainStart..<i], f, link)
                        var nf = f
                        if c == "~" { nf.strike = true } else { nf.bold = true }
                        parseInline(inner, nf, link, &out, scan: scan, depth: depth + 1)
                        handled = n
                    } else if c != "~" {
                        if let (inner, n) = delimited(rest, String(c)) {
                            push(&out, s[plainStart..<i], f, link)
                            var nf = f
                            nf.italic = true
                            parseInline(inner, nf, link, &out, scan: scan, depth: depth + 1)
                            handled = n
                        }
                    }
                }
            }
            if let n = handled {
                i = s.index(i, offsetBy: n)
                plainStart = i
            } else {
                i = s.index(after: i)
            }
        }
        push(&out, s[plainStart...], f, link)
    }

    /// The inline spans of one line of markdown.
    public static func parseInline(_ s: String) -> [MarkdownInline] {
        var out: [MarkdownInline] = []
        parseInline(Substring(s), Flags(), nil, &out, scan: InlineScan(s))
        return out
    }

    private static func isTableRow(_ line: String) -> Bool {
        let t = line.trimmingCharacters(in: .whitespaces)
        return t.count >= 2 && t.hasPrefix("|") && t.hasSuffix("|")
    }

    private static func cells(_ line: String) -> [String] {
        var t = Substring(line.trimmingCharacters(in: .whitespaces))
        if t.hasPrefix("|") { t = t.dropFirst() }
        if t.hasSuffix("|") { t = t.dropLast() }
        return t.replacingOccurrences(of: "\\|", with: "\u{1}").split(separator: "|", omittingEmptySubsequences: false).map { $0.replacingOccurrences(of: "\u{1}", with: "|").trimmingCharacters(in: .whitespaces) }
    }

    private static func isTableRule(_ line: String) -> Bool {
        isTableRow(line) && cells(line).allSatisfy { c in
            let core = c.trimmingCharacters(in: CharacterSet(charactersIn: ":"))
            return !c.isEmpty && core.contains("-") && core.allSatisfy { $0 == "-" }
        }
    }

    private static func isRule(_ t: String) -> Bool {
        t.count >= 3 && (t.allSatisfy { $0 == "-" } || t.allSatisfy { $0 == "*" } || t.allSatisfy { $0 == "_" })
    }

    /// `- item`, `* item`, `3. item` → (marker, rest, the marker's column;
    /// a tab advances to the next multiple of 4).
    private static func listMarker(_ line: String) -> (String, String, Int)? {
        var column = 0
        for c in line {
            if c == " " { column += 1 } else if c == "\t" { column += 4 - column % 4 } else { break }
        }
        let t = line.drop(while: { $0 == " " || $0 == "\t" })
        if t.hasPrefix("- ") || t.hasPrefix("* ") || t.hasPrefix("+ ") {
            return ("•", String(t.dropFirst(2)), column)
        }
        let digits = t.prefix(while: { $0.isASCII && $0.isNumber })
        if !digits.isEmpty, digits.count <= 9 {
            let after = t.dropFirst(digits.count)
            if after.hasPrefix(". ") || after.hasPrefix(") ") {
                return ("\(digits).", String(after.dropFirst(2)), column)
            }
        }
        return nil
    }

    public static func parse(_ text: String) -> [MarkdownBlock] {
        let normalized = text.replacingOccurrences(of: "\r\n", with: "\n").replacingOccurrences(of: "\r", with: "\n")
        let lines = normalized.components(separatedBy: "\n")
        var blocks: [MarkdownBlock] = []
        var para: [String] = []
        var group = 0
        func flush() {
            if !para.isEmpty {
                blocks.append(MarkdownBlock(kind: .paragraph, inlines: parseInline(para.joined(separator: " "))))
                para.removeAll()
            }
        }
        var i = 0
        while i < lines.count {
            let line = lines[i]
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if trimmed.hasPrefix("```") {
                flush()
                var body: [String] = []
                i += 1
                while i < lines.count, !lines[i].trimmingCharacters(in: .whitespaces).hasPrefix("```") {
                    body.append(lines[i])
                    i += 1
                }
                i += 1
                blocks.append(MarkdownBlock(kind: .codeBlock, inlines: [MarkdownInline(text: body.joined(separator: "\n"), code: true)]))
                continue
            }
            let hashes = trimmed.prefix(while: { $0 == "#" }).count
            if (1...6).contains(hashes), trimmed.dropFirst(hashes).hasPrefix(" ") {
                flush()
                blocks.append(MarkdownBlock(kind: .heading(hashes), inlines: parseInline(trimmed.dropFirst(hashes).trimmingCharacters(in: .whitespaces))))
                i += 1
                continue
            }
            if trimmed.hasPrefix(">") {
                flush()
                var body: [String] = []
                while i < lines.count, lines[i].drop(while: { $0 == " " }).hasPrefix(">") {
                    var l = Substring(lines[i].drop(while: { $0 == " " }))
                    while l.hasPrefix(">") { l = l.dropFirst() }
                    if l.hasPrefix(" ") { l = l.dropFirst() }
                    body.append(l.trimmingCharacters(in: .whitespaces))
                    i += 1
                }
                blocks.append(MarkdownBlock(kind: .quote, inlines: parseInline(body.filter { !$0.isEmpty }.joined(separator: " "))))
                continue
            }
            if isRule(trimmed), para.isEmpty {
                blocks.append(MarkdownBlock(kind: .rule, inlines: []))
                i += 1
                continue
            }
            if listMarker(line) != nil {
                flush()
                group += 1
                // The marker columns of the open lists, outermost first: an
                // item indented ≥ 2 columns past the previous item's marker
                // opens a child list under it; a shallower one closes lists
                // until it fits.
                var columns: [Int] = []
                while i < lines.count, let (marker, rest, column) = listMarker(lines[i]) {
                    while let last = columns.last, column < last + 2 { columns.removeLast() }
                    columns.append(column)
                    var task: Bool? = nil
                    var body = rest
                    if rest.hasPrefix("[ ] ") || rest == "[ ]" { task = false; body = String(rest.dropFirst(3)) }
                    else if rest.hasPrefix("[x] ") || rest.hasPrefix("[X] ") || rest == "[x]" || rest == "[X]" { task = true; body = String(rest.dropFirst(3)) }
                    blocks.append(MarkdownBlock(kind: .listItem(marker: marker, task: task), inlines: parseInline(body.trimmingCharacters(in: .whitespaces)), listGroup: group, depth: columns.count - 1))
                    i += 1
                }
                continue
            }
            if isTableRow(line), i + 1 < lines.count, isTableRule(lines[i + 1]) {
                flush()
                let header = cells(line).map { parseInline($0) }
                i += 2
                var rows: [[[MarkdownInline]]] = []
                while i < lines.count, isTableRow(lines[i]) {
                    rows.append(cells(lines[i]).map { parseInline($0) })
                    i += 1
                }
                blocks.append(MarkdownBlock(kind: .table(header: header, rows: rows), inlines: []))
                continue
            }
            if let (src, alt) = wholeImage(trimmed) {
                flush()
                blocks.append(MarkdownBlock(kind: .image(src: src, alt: alt), inlines: alt.isEmpty ? [] : [MarkdownInline(text: alt)]))
                i += 1
                continue
            }
            if trimmed.isEmpty { flush() } else { para.append(trimmed) }
            i += 1
        }
        flush()
        return blocks
    }

    /// Image blocks whose src `allowed` refuses become a paragraph of their
    /// alt text (dropped without one): what the measurer and the painter
    /// both lay out, so a denied image never reserves a box.
    public static func resolveImages(_ blocks: [MarkdownBlock], allowed: (String) -> Bool) -> [MarkdownBlock] {
        blocks.compactMap { b in
            guard case let .image(src, alt) = b.kind, !allowed(src) else { return b }
            return alt.isEmpty ? nil : MarkdownBlock(kind: .paragraph, inlines: [MarkdownInline(text: alt)])
        }
    }

    /// Every link through `href` (nil = the policy denied it: the span
    /// stays, as plain text).
    public static func mapLinks(_ blocks: [MarkdownBlock], _ href: (String) -> String?) -> [MarkdownBlock] {
        func map(_ spans: [MarkdownInline]) -> [MarkdownInline] {
            spans.map { span in
                var s = span
                if let l = span.link { s.link = href(l) }
                return s
            }
        }
        return blocks.map { b in
            var out = b
            out.inlines = map(b.inlines)
            if case let .table(header, rows) = b.kind {
                out.kind = .table(header: header.map(map), rows: rows.map { $0.map(map) })
            }
            return out
        }
    }

    /// The plain text of some spans.
    public static func plain(_ inlines: [MarkdownInline]) -> String {
        inlines.map(\.text).joined()
    }

    /// The plain text of a whole document (accessibility).
    public static func plainText(_ text: String) -> String {
        parse(text).map { block -> String in
            if case let .table(header, rows) = block.kind {
                return ([header] + rows).map { $0.map(plain).joined(separator: " ") }.joined(separator: "\n")
            }
            return plain(block.inlines)
        }.joined(separator: "\n")
    }
}

/// Typography of one block kind.
public struct MarkdownTextSpec: Equatable, Sendable {
    public var size: CGFloat
    public var lineHeight: CGFloat
    public var weight: Int
    public var family: String?

    public init(size: CGFloat, lineHeight: CGFloat, weight: Int, family: String?) {
        self.size = size
        self.lineHeight = lineHeight
        self.weight = weight
        self.family = family
    }
}

/// Resolved metrics of a markdown document (the `Markdown/*` recipe parts
/// in the painter; plain defaults otherwise). The numbers are the gpui
/// painter's, so the three native painters agree.
public struct MarkdownStyles: Equatable, Sendable {
    public var body: MarkdownTextSpec
    public var heading: MarkdownTextSpec
    public var code: MarkdownTextSpec
    public var codePad: CGFloat
    public var quoteBorder: CGFloat
    public var quotePad: CGFloat
    public var blockGap: CGFloat
    public var headingTop: CGFloat
    public var headingBottom: CGFloat
    public var listIndent: CGFloat
    public var cellPadH: CGFloat
    public var cellPadV: CGFloat
    /// An image block's box height (the picture fits inside it).
    public var imageHeight: CGFloat

    public init(body: MarkdownTextSpec) {
        self.body = body
        heading = MarkdownTextSpec(size: body.size + 4, lineHeight: body.lineHeight + 8, weight: 600, family: body.family)
        code = MarkdownTextSpec(size: max(body.size - 2, 10), lineHeight: body.lineHeight, weight: 400, family: nil)
        listIndent = (body.size * 1.4).rounded()
        codePad = 12
        quoteBorder = 2
        quotePad = 12
        blockGap = 8
        headingTop = 12
        headingBottom = 4
        cellPadH = 8
        cellPadV = 4
        imageHeight = 160
    }

    /// A list item's indent before its body: one `listIndent` per level
    /// plus the marker column.
    public func listInset(_ block: MarkdownBlock) -> CGFloat {
        listIndent * CGFloat(block.depth + 1)
    }

    public func spec(of kind: MarkdownBlockKind) -> MarkdownTextSpec {
        switch kind {
        case .heading: heading
        case .codeBlock: code
        default: body
        }
    }

    func margins(_ kind: MarkdownBlockKind) -> (CGFloat, CGFloat) {
        if case .heading = kind { return (headingTop, headingBottom) }
        return (0, blockGap)
    }

    /// The gap above block `i` (margins collapse; first/last margins drop).
    public func gapBefore(_ blocks: [MarkdownBlock], _ i: Int) -> CGFloat {
        if i == 0 { return 0 }
        let prev = blocks[i - 1], cur = blocks[i]
        if prev.listGroup != nil, prev.listGroup == cur.listGroup { return 0 }
        return max(margins(prev.kind).1, margins(cur.kind).0)
    }
}

/// What the layout asks the text system: heights of spans wrapped at a
/// width (nil = one line per paragraph), widths, widest words.
public protocol MarkdownTextMeasure {
    func height(_ inlines: [MarkdownInline], _ spec: MarkdownTextSpec, width: CGFloat?) -> CGFloat
    func width(_ inlines: [MarkdownInline], _ spec: MarkdownTextSpec) -> CGFloat
    func widestWord(_ inlines: [MarkdownInline], _ spec: MarkdownTextSpec) -> CGFloat
}

public struct MarkdownBlockBox: Equatable, Sendable {
    public var y: CGFloat
    public var height: CGFloat
    /// Tables: each row's height (header first).
    public var rows: [CGFloat]
}

public struct MarkdownLayout: Equatable, Sendable {
    public var blocks: [MarkdownBlockBox]
    public var height: CGFloat
}

public extension Markdown {
    /// Lay the blocks out `width` wide (the painter places blocks at these
    /// boxes; the measurer reports `height`).
    static func layout(_ blocks: [MarkdownBlock], _ s: MarkdownStyles, width: CGFloat, text: MarkdownTextMeasure) -> MarkdownLayout {
        var y: CGFloat = 0
        var out: [MarkdownBlockBox] = []
        for (i, b) in blocks.enumerated() {
            y += s.gapBefore(blocks, i)
            let spec = s.spec(of: b.kind)
            var rows: [CGFloat] = []
            let height: CGFloat
            switch b.kind {
            case .paragraph, .heading:
                height = text.height(b.inlines, spec, width: max(width, 1))
            case .listItem:
                height = text.height(b.inlines, spec, width: max(width - s.listInset(b), 1))
            case .quote:
                height = text.height(b.inlines, spec, width: max(width - s.quoteBorder - s.quotePad, 1))
            case .codeBlock:
                height = text.height(b.inlines, spec, width: nil) + 2 * s.codePad
            case .rule:
                height = 1
            case .image:
                height = s.imageHeight
            case let .table(header, trs):
                let cols = max(header.count, trs.map(\.count).max() ?? 0, 1)
                let cellW = max(width / CGFloat(cols) - 2 * s.cellPadH, 1)
                for row in [header] + trs {
                    let h = row.map { text.height($0, s.body, width: cellW) }.reduce(s.body.lineHeight, max)
                    rows.append(h + 2 * s.cellPadV + 1)
                }
                height = rows.reduce(0, +) + 1
            }
            out.append(MarkdownBlockBox(y: y, height: height, rows: rows))
            y += height
        }
        return MarkdownLayout(blocks: out, height: y)
    }

    /// Max-content width: the widest block on one line.
    static func maxContentWidth(_ blocks: [MarkdownBlock], _ s: MarkdownStyles, text: MarkdownTextMeasure) -> CGFloat {
        blocks.map { b -> CGFloat in
            let spec = s.spec(of: b.kind)
            switch b.kind {
            case .paragraph, .heading: return text.width(b.inlines, spec)
            case .listItem: return text.width(b.inlines, spec) + s.listInset(b)
            case .quote: return text.width(b.inlines, spec) + s.quoteBorder + s.quotePad
            case .image: return (s.imageHeight * 1.5).rounded()
            case .codeBlock:
                let first = b.inlines.first ?? MarkdownInline(text: "", code: true)
                return (first.text.components(separatedBy: "\n").map { text.width([MarkdownInline(text: $0, code: true)], spec) }.max() ?? 0) + 2 * s.codePad
            case .rule: return 0
            case let .table(header, rows):
                let cols = CGFloat(max(header.count, 1))
                let widest = ([header] + rows).flatMap { $0 }.map { text.width($0, s.body) }.max() ?? 0
                return cols * (widest + 2 * s.cellPadH)
            }
        }.max().map { ceil($0) } ?? 0
    }

    /// Min-content width: the widest word (code blocks never wrap).
    static func minContentWidth(_ blocks: [MarkdownBlock], _ s: MarkdownStyles, text: MarkdownTextMeasure) -> CGFloat {
        blocks.map { b -> CGFloat in
            let spec = s.spec(of: b.kind)
            switch b.kind {
            case .codeBlock: return maxContentWidth([b], s, text: text)
            case let .table(header, _): return CGFloat(max(header.count, 1)) * (2 * s.cellPadH + 16)
            case .listItem: return text.widestWord(b.inlines, spec) + s.listInset(b)
            case .image: return 1
            case .quote: return text.widestWord(b.inlines, spec) + s.quoteBorder + s.quotePad
            default: return text.widestWord(b.inlines, spec)
            }
        }.max().map { ceil($0) } ?? 0
    }
}
