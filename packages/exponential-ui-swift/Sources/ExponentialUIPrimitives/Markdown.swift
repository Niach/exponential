import Foundation

// The read-only GFM subset every Exponential UI renderer paints the same way
// (the gpui painter's `paint::markdown` and the React renderer's block
// model): paragraphs, ATX headings, bullet / ordered / task lists (nested
// by marker indentation), quotes, fenced code, rules, pipe tables, images
// alone on a line; inline bold, italic, strike, code, links (destinations
// balance parentheses like CommonMark, no whitespace). #FEED-208: an image
// directly in a top-level paragraph's text splits the paragraph around a
// block image; anywhere else (a heading, list item, cell, quote, or inside
// emphasis or a link label) it paints its alt text.
// No dependency: the parser is ours so the measurer and the painter share
// ONE block model, and the layout rule below is what both use.

public struct MarkdownInline: Equatable, Sendable {
    public var text: String
    public var bold = false
    public var italic = false
    public var code = false
    public var strike = false
    public var link: String?
    /// #VAPP-258 (`catalog/host.json` markdown): the host's inline chip for
    /// this (linked) run; a chip run never merges with a neighbour.
    public var chip: MarkdownChip?
    /// Round 9: a fenced block's text: never the inline code's pill.
    public var fenced = false

    public init(text: String, bold: Bool = false, italic: Bool = false, code: Bool = false, strike: Bool = false, link: String? = nil, chip: MarkdownChip? = nil, fenced: Bool = false) {
        self.text = text
        self.bold = bold
        self.italic = italic
        self.code = code
        self.strike = strike
        self.link = link
        self.chip = chip
        self.fenced = fenced
    }
}

/// #VAPP-258: a host's inline chip (a `textLinks` segment's `chip`): the
/// catalog Chip's look inline in a text run, one unbreakable unit.
public struct MarkdownChip: Equatable, Sendable, Hashable {
    public var kind: String
    public var label: String
    /// An icon registry name.
    public var icon: String?
    /// #VAPP-258: an identifier (`ABC-1`) painted mono before the label
    /// (nil when the host gave none or an empty one).
    public var ident: String?

    /// `catalog/host.json` markdown `chipKinds`.
    public static let kinds: Set<String> = ["issue", "mention", "entity"]

    public init(kind: String, label: String, icon: String? = nil, ident: String? = nil) {
        self.kind = kind
        self.label = label
        self.icon = icon
        self.ident = ident?.isEmpty == true ? nil : ident
    }

    /// A known kind and a non-empty label (else the chip is dropped, the
    /// link stays).
    public var isValid: Bool { Self.kinds.contains(kind) && !label.isEmpty }
}

/// #VAPP-258: `attachmentKinds`.
public enum MarkdownAttachmentKind: String, Equatable, Sendable, CaseIterable {
    case video, audio, file
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
    /// `![alt](src)` alone on a line, or (#FEED-208) split out of a
    /// top-level paragraph's text: painted through the host's media
    /// request (a denied src becomes a paragraph of its alt text, see
    /// `resolveImages`; a failed load paints the alt text in the box).
    case image(src: String, alt: String)
    /// #VAPP-258: a top-level paragraph that is exactly one markdown link
    /// the host classified (`attachment(href)`): `href` = the policed href.
    case attachment(type: MarkdownAttachmentKind, name: String, href: String)
}

public struct MarkdownBlock: Equatable, Sendable {
    public var kind: MarkdownBlockKind
    public var inlines: [MarkdownInline]
    /// Consecutive list items share a group (no paragraph gap inside it).
    public var listGroup: Int?
    /// A list item's nesting level (0 = top level): indented one
    /// `listIndent` per level.
    public var depth: Int
    /// #VAPP-258: a code block's fence info string (` ```ts ` = `ts`).
    public var lang: String?
    /// #VAPP-258: an image block's natural size as the host's `imageSize`
    /// answered it (positive and finite; nil = the fixed box).
    public var imageSize: CGSize?

    public init(kind: MarkdownBlockKind, inlines: [MarkdownInline], listGroup: Int? = nil, depth: Int = 0, lang: String? = nil, imageSize: CGSize? = nil) {
        self.kind = kind
        self.inlines = inlines
        self.listGroup = listGroup
        self.depth = depth
        self.lang = lang
        self.imageSize = imageSize
    }
}

public enum Markdown {
    private struct Flags {
        var bold = false, italic = false, strike = false
    }

    private static func push(_ out: inout [MarkdownInline], _ raw: Substring, _ f: Flags, _ link: String?) {
        if raw.isEmpty { return }
        let text = raw.contains("&") ? Substring(decodeEntities(String(raw))) : raw
        if let last = out.indices.last, !out[last].code, out[last].bold == f.bold, out[last].italic == f.italic, out[last].strike == f.strike, out[last].link == link {
            out[last].text += text
            return
        }
        out.append(MarkdownInline(text: String(text), bold: f.bold, italic: f.italic, strike: f.strike, link: link))
    }

    private static let entities: [String: String] = ["amp": "&", "lt": "<", "gt": ">", "quot": "\"", "apos": "'", "nbsp": "\u{00A0}"]

    /// #VAPP-258 (markdown-render.json): the common named entities and
    /// numeric references decoded (text, never code).
    static func decodeEntities(_ s: String) -> String {
        guard let regex = try? NSRegularExpression(pattern: "&(?:#([0-9]{1,7})|#[xX]([0-9a-fA-F]{1,6})|([A-Za-z]+));") else { return s }
        let ns = s as NSString
        var out = ""
        var at = 0
        for m in regex.matches(in: s, range: NSRange(location: 0, length: ns.length)) {
            out += ns.substring(with: NSRange(location: at, length: m.range.location - at))
            let whole = ns.substring(with: m.range)
            var decoded: String?
            if m.range(at: 3).location != NSNotFound {
                decoded = entities[ns.substring(with: m.range(at: 3))]
            } else {
                let dec = m.range(at: 1).location != NSNotFound
                let digits = ns.substring(with: m.range(at: dec ? 1 : 2))
                if let code = UInt32(digits, radix: dec ? 10 : 16), code > 0, let scalar = Unicode.Scalar(code) { decoded = String(Character(scalar)) }
            }
            out += decoded ?? whole
            at = m.range.location + m.range.length
        }
        out += ns.substring(from: at)
        return out
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
        guard let open = scan.ordinal(s.startIndex), let limit = scan.ordinal(s.endIndex),
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

        /// The ordinal of a character boundary of the string (binary search).
        func ordinal(_ i: String.Index) -> Int? {
            var lo = 0, hi = index.count
            while lo < hi {
                let mid = (lo + hi) / 2
                if index[mid] < i { lo = mid + 1 } else { hi = mid }
            }
            return lo < index.count && index[lo] == i ? lo : nil
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

    /// #FEED-208: the images directly in a top-level paragraph's text, each
    /// at the index of the empty code run that stands in for it in the
    /// paragraph's runs (it keeps the text before and after from merging).
    private final class ImageMarks {
        var marks: [(at: Int, src: String, alt: String)] = []
    }

    /// `images` (only the top-level call of a top-level paragraph passes
    /// it, so emphasis and link labels never do) collects the images
    /// directly in the text instead of painting their alt text.
    private static func parseInline(_ s: Substring, _ f: Flags, _ link: String?, _ out: inout [MarkdownInline], scan: InlineScan, depth: Int = 0, images: ImageMarks? = nil) {
        let nest = depth < maxNesting
        var plainStart = s.startIndex
        var i = s.startIndex
        while i < s.endIndex {
            let rest = s[i...]
            let c = s[i]
            var handled: Int? = nil
            if c == "\\", let next = s.index(i, offsetBy: 1, limitedBy: s.endIndex), next < s.endIndex, s[next].isASCII, s[next].isPunctuation || s[next].isSymbol {
                // #VAPP-258: a backslash-escaped punctuation character is literal.
                push(&out, s[plainStart..<i], f, link)
                push(&out, s[next...next], f, link)
                handled = 2
            } else if c == "`" {
                let body = rest.dropFirst()
                if let end = body.firstIndex(of: "`") {
                    push(&out, s[plainStart..<i], f, link)
                    out.append(MarkdownInline(text: String(body[body.startIndex..<end]), code: true, link: link))
                    handled = body.distance(from: body.startIndex, to: end) + 2
                }
            } else if c == "!", rest.dropFirst().hasPrefix("[") {
                if let (alt, src, n) = linkAt(rest.dropFirst(), emptyLabel: true, scan: scan) {
                    // #FEED-208: an image directly in a top-level paragraph's
                    // text splits it (`images`); anywhere else it paints its
                    // alt text.
                    push(&out, s[plainStart..<i], f, link)
                    if let images {
                        images.marks.append((at: out.count, src: src, alt: plain(parseInline(String(alt)))))
                        out.append(MarkdownInline(text: "", code: true))
                    } else {
                        push(&out, alt, f, link)
                    }
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
                    if c != "~", let (inner, n) = delimited(rest, String(repeating: c, count: 3)) {
                        // `***x***` is both.
                        push(&out, s[plainStart..<i], f, link)
                        var nf = f
                        nf.bold = true
                        nf.italic = true
                        parseInline(inner, nf, link, &out, scan: scan, depth: depth + 1)
                        handled = n
                    } else if let (inner, n) = delimited(rest, double) {
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

    /// #FEED-208 (markdown-render.json `image_split_*`): a top-level
    /// paragraph with images directly in its text = the text before each
    /// image (a paragraph), the image (a block image), the text after it.
    /// ASCII spaces, tabs and line breaks at a split edge trim (plain runs
    /// only); an empty side drops.
    private static func paragraphBlocks(_ text: String) -> [MarkdownBlock] {
        var runs: [MarkdownInline] = []
        let images = ImageMarks()
        parseInline(Substring(text), Flags(), nil, &runs, scan: InlineScan(text), images: images)
        guard !images.marks.isEmpty else { return [MarkdownBlock(kind: .paragraph, inlines: runs)] }
        func plainRun(_ r: MarkdownInline) -> Bool { !r.bold && !r.italic && !r.code && !r.strike && r.link == nil }
        func edge(_ c: Character) -> Bool { c == " " || c == "\t" || c == "\n" }
        var blocks: [MarkdownBlock] = []
        func side(_ slice: ArraySlice<MarkdownInline>) {
            var part = Array(slice)
            while let first = part.first, plainRun(first) {
                let t = String(first.text.drop(while: edge))
                if !t.isEmpty { part[0].text = t; break }
                part.removeFirst()
            }
            while let last = part.last, plainRun(last) {
                var t = Substring(last.text)
                while let c = t.last, edge(c) { t = t.dropLast() }
                if !t.isEmpty { part[part.count - 1].text = String(t); break }
                part.removeLast()
            }
            if !part.isEmpty { blocks.append(MarkdownBlock(kind: .paragraph, inlines: part)) }
        }
        var from = 0
        for mark in images.marks {
            side(runs[from..<mark.at])
            blocks.append(MarkdownBlock(kind: .image(src: mark.src, alt: mark.alt), inlines: mark.alt.isEmpty ? [] : [MarkdownInline(text: mark.alt)]))
            from = mark.at + 1
        }
        side(runs[from...])
        return blocks
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

    /// A line that opens a block of its own (never a continuation).
    private static func startsBlock(_ line: String) -> Bool {
        let t = line.trimmingCharacters(in: .whitespaces)
        let hashes = t.prefix(while: { $0 == "#" }).count
        return t.hasPrefix("```") || t.hasPrefix("~~~") || t.hasPrefix(">") || isTableRow(line) || isRule(t)
            || ((1...6).contains(hashes) && t.dropFirst(hashes).hasPrefix(" "))
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

    /// #VAPP-258 step 5 (`catalog/host.json` markdown.options): `hardBreaks`
    /// = every newline inside a paragraph, list item or quote paragraph is a
    /// line break; `streaming` = the text is a growing prefix
    /// (`parseStreaming`).
    public static func parse(_ text: String, hardBreaks: Bool, streaming: Bool = false) -> [MarkdownBlock] {
        streaming ? parseStreaming(text, hardBreaks: hardBreaks) : parseBlocks(text, hard: hardBreaks)
    }

    public static func parse(_ text: String) -> [MarkdownBlock] {
        parseBlocks(text, hard: false)
    }

    /// The 3-char fence a line opens (` ``` ` / `~~~`), else nil.
    private static func fenceOpen(_ line: Substring) -> String? {
        let t = line.trimmingCharacters(in: .whitespaces)
        if t.hasPrefix("```") { return "```" }
        if t.hasPrefix("~~~") { return "~~~" }
        return nil
    }

    /// `lines` from `from` (a line outside a fence) on: the line index after
    /// the last COMPLETE blank line outside a fence (else `from`; the last
    /// line is never complete), whether a fence is open at the end, and
    /// whether the last line is that fence's opener.
    private static func scanFences(_ lines: [Substring], from: Int) -> (boundary: Int, open: Bool, openerLast: Bool) {
        var fence: String?
        var boundary = from
        var openerLast = false
        var i = from
        while i < lines.count {
            let line = lines[i]
            let complete = i < lines.count - 1
            openerLast = false
            if let f = fence {
                if line.trimmingCharacters(in: .whitespaces).hasPrefix(f) { fence = nil }
            } else if let f = fenceOpen(line) {
                fence = f
                openerLast = true
            } else if complete, line.trimmingCharacters(in: .whitespaces).isEmpty {
                boundary = i + 1
            }
            i += 1
        }
        return (boundary, fence != nil, openerLast)
    }

    private final class StreamMemos: @unchecked Sendable {
        struct Entry {
            var text: String
            var hard: Bool
            var boundary: Int
            var stable: [MarkdownBlock]
        }
        let lock = NSLock()
        var entries: [Entry] = []
    }
    private static let streamMemos = StreamMemos()
    private static let streamMemoCap = 16

    /// Forget every streamed parse (tests: a cold parse of a prefix).
    public static func resetStreamMemo() {
        streamMemos.lock.lock()
        streamMemos.entries.removeAll()
        streamMemos.lock.unlock()
    }

    private static func maxGroup(_ blocks: [MarkdownBlock]) -> Int {
        blocks.compactMap(\.listGroup).max() ?? 0
    }

    private static func offsetGroups(_ blocks: inout [MarkdownBlock], by: Int) {
        guard by > 0 else { return }
        for i in blocks.indices where blocks[i].listGroup != nil { blocks[i].listGroup! += by }
    }

    /// #VAPP-258 step 5 (`fixtures/markdown-render.json` streamingCases): a
    /// growing prefix = the stable blocks before its last complete blank
    /// line outside a fence (memoized per text: only what extends the old
    /// text reparses) + the tail, whose unfinished end is held back.
    public static func parseStreaming(_ raw: String, hardBreaks hard: Bool) -> [MarkdownBlock] {
        let text = raw.replacingOccurrences(of: "\r\n", with: "\n").replacingOccurrences(of: "\r", with: "\n")
        let lines = text.split(separator: "\n", omittingEmptySubsequences: false)
        streamMemos.lock.lock()
        let memo = streamMemos.entries.filter { $0.hard == hard && text.hasPrefix($0.text) }.max { $0.text.utf8.count < $1.text.utf8.count }
        streamMemos.lock.unlock()
        let from = memo?.boundary ?? 0
        var stable = memo?.stable ?? []
        let boundary = scanFences(lines, from: from).boundary
        if boundary > from {
            var more = parseBlocks(lines[from..<boundary].joined(separator: "\n"), hard: hard)
            offsetGroups(&more, by: maxGroup(stable))
            stable += more
        }
        streamMemos.lock.lock()
        streamMemos.entries.removeAll { $0.hard == hard && text.hasPrefix($0.text) }
        streamMemos.entries.insert(StreamMemos.Entry(text: text, hard: hard, boundary: boundary, stable: stable), at: 0)
        if streamMemos.entries.count > streamMemoCap { streamMemos.entries.removeLast(streamMemos.entries.count - streamMemoCap) }
        streamMemos.lock.unlock()
        var tailLines = Array(lines[boundary...])
        if tailLines.count == 1, tailLines[0].isEmpty { return stable }
        let fences = scanFences(tailLines, from: 0)
        var tail: String
        if fences.open {
            // The open fence's last line waits while it is empty or only
            // fence characters (a closing fence on its way).
            if !fences.openerLast, let last = tailLines.last, last.trimmingCharacters(in: .whitespaces).allSatisfy({ $0 == "`" || $0 == "~" }) {
                tailLines.removeLast()
            }
            tail = tailLines.joined(separator: "\n")
        } else {
            tail = tailLines.joined(separator: "\n")
            while let held = holdTrailing(tail) { tail = String(tail.utf8.prefix(held))! }
        }
        var blocks = parseBlocks(tail, hard: hard)
        offsetGroups(&blocks, by: maxGroup(stable))
        if var last = blocks.popLast() {
            switch last.kind {
            case .paragraph, .heading, .quote, .listItem: last.inlines = holdBack(last.inlines)
            default: break
            }
            if !(last.kind == .paragraph && last.inlines.allSatisfy { $0.text.isEmpty }) { blocks.append(last) }
        }
        return stable + blocks
    }

    /// Where the tail's unfinished end starts (a UTF-8 offset), outside a
    /// fence (nil = it ends finished): a last line that is only a list
    /// marker or heading hashes so far, a trailing run of `*` / `_`, or of
    /// backticks / tildes unless they are the line's only text.
    static func holdTrailing(_ tail: String) -> Int? {
        let b = Array(tail.utf8)
        if b.isEmpty { return nil }
        let lineStart = (b.lastIndex(of: 10).map { $0 + 1 }) ?? 0
        var ls = lineStart
        while ls < b.count, b[ls] == 32 || b[ls] == 9 { ls += 1 }
        let line = Array(b[ls...])
        var hashesEnd = line.count
        while hashesEnd > 0, line[hashesEnd - 1] == 32 || line[hashesEnd - 1] == 9 { hashesEnd -= 1 }
        let hashes = line[..<hashesEnd]
        let digits = line.prefix { $0 >= 48 && $0 <= 57 }.count
        let marker = (line.count == 1 && (line[0] == 45 || line[0] == 43 || line[0] == 42))
            || ((1...6).contains(hashes.count) && hashes.allSatisfy { $0 == 35 })
            || ((1...9).contains(digits) && (line.count == digits || (line.count == digits + 1 && (line[digits] == 46 || line[digits] == 41))))
        if marker { return max(lineStart - 1, 0) }
        func run(_ set: Set<UInt8>) -> Int {
            var n = 0
            while n < b.count, set.contains(b[b.count - 1 - n]) { n += 1 }
            return n
        }
        let stars = run([42, 95])
        if stars > 0 { return b.count - stars }
        let ticks = run([96, 126])
        if ticks > 0 {
            let start = b.count - ticks
            if !b[min(lineStart, start)..<start].allSatisfy({ $0 == 32 || $0 == 9 }) { return start }
        }
        return nil
    }

    /// The streaming tail's last block: its plain runs (no flags, link or
    /// chip) hold back what would flicker: an unfinished link at the end
    /// paints as its label, and every opening delimiter run is dropped (a
    /// maximal run of `*` / `_` / backticks, or of 2+ `~`, not after an ASCII
    /// letter, digit or delimiter, followed by a non-space or ending the run).
    static func holdBack(_ inlines: [MarkdownInline]) -> [MarkdownInline] {
        func plain(_ i: MarkdownInline) -> Bool { !i.bold && !i.italic && !i.code && !i.strike && i.link == nil && i.chip == nil }
        var out = inlines
        if let last = out.last, plain(last), let open = last.text.lastIndex(of: "[") {
            let rest = last.text[last.text.index(after: open)...]
            let labelEnd = rest.firstIndex(of: "]") ?? rest.endIndex
            let after = rest[labelEnd...]
            let unfinished = after.isEmpty || after == "]" || (after.hasPrefix("](") && after.dropFirst(2).allSatisfy { $0 != "(" && $0 != ")" && !$0.isWhitespace })
            if unfinished {
                var start = open
                if open > last.text.startIndex, last.text[last.text.index(before: open)] == "!" { start = last.text.index(before: open) }
                out[out.count - 1].text = String(last.text[..<start]) + String(rest[..<labelEnd])
            }
        }
        for i in out.indices where plain(out[i]) { out[i].text = dropDelimiters(out[i].text) }
        return out.filter { !$0.text.isEmpty || $0.chip != nil }
    }

    static func dropDelimiters(_ s: String) -> String {
        let b = Array(s.utf8)
        var out: [UInt8] = []
        out.reserveCapacity(b.count)
        func delim(_ c: UInt8) -> Bool { c == 42 || c == 95 || c == 96 }
        func alnum(_ c: UInt8) -> Bool { (c >= 48 && c <= 57) || (c >= 65 && c <= 90) || (c >= 97 && c <= 122) }
        func space(_ c: UInt8) -> Bool { c == 32 || c == 9 || c == 10 || c == 13 || c == 11 || c == 12 }
        var i = 0
        while i < b.count {
            let c = b[i]
            let tilde = c == 126
            if delim(c) || tilde {
                var j = i
                while j < b.count, tilde ? b[j] == 126 : delim(b[j]) { j += 1 }
                let beforeOk = i == 0 || !(alnum(b[i - 1]) || delim(b[i - 1]) || b[i - 1] == 126)
                let afterOk = j == b.count || !space(b[j])
                if !(beforeOk && afterOk && (!tilde || j - i >= 2)) { out += b[i..<j] }
                i = j
                continue
            }
            out.append(c)
            i += 1
        }
        return String(decoding: out, as: UTF8.self)
    }

    private static func parseBlocks(_ text: String, hard: Bool) -> [MarkdownBlock] {
        let normalized = text.replacingOccurrences(of: "\r\n", with: "\n").replacingOccurrences(of: "\r", with: "\n")
        let lines = normalized.components(separatedBy: "\n")
        var blocks: [MarkdownBlock] = []
        var para: [String] = []
        var group = 0
        func flush() {
            if !para.isEmpty {
                // A line ending in a backslash or two spaces breaks hard (a
                // newline in the run); any other line break is a space.
                var text = ""
                for (k, line) in para.enumerated() {
                    if k > 0 {
                        if text.hasSuffix("\\") { text.removeLast(); text += "\n" }
                        else if text.hasSuffix("  ") { while text.hasSuffix(" ") { text.removeLast() }; text += "\n" }
                        else { text += hard ? "\n" : " " }
                    }
                    text += line
                }
                while text.hasSuffix(" ") { text.removeLast() }
                // #FEED-208: images directly in its text split it.
                blocks.append(contentsOf: paragraphBlocks(text))
                para.removeAll()
            }
        }
        var i = 0
        while i < lines.count {
            let line = lines[i]
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if trimmed.hasPrefix("```") || trimmed.hasPrefix("~~~") {
                flush()
                let fence = String(trimmed.prefix(3))
                let info = trimmed.drop(while: { $0 == "`" || $0 == "~" }).split(whereSeparator: \.isWhitespace).first.map(String.init)
                var body: [String] = []
                i += 1
                while i < lines.count, !lines[i].trimmingCharacters(in: .whitespaces).hasPrefix(fence) {
                    body.append(lines[i])
                    i += 1
                }
                i += 1
                blocks.append(MarkdownBlock(kind: .codeBlock, inlines: [MarkdownInline(text: body.joined(separator: "\n"), code: true, fenced: true)], lang: info))
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
                // Paragraphs join with a space; under `hard` a paragraph's
                // lines with a newline.
                var joined = ""
                var gap = false
                for l in body {
                    if l.isEmpty { gap = true; continue }
                    if !joined.isEmpty { joined += hard && !gap ? "\n" : " " }
                    joined += l
                    gap = false
                }
                blocks.append(MarkdownBlock(kind: .quote, inlines: parseInline(joined)))
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
                var lastBody = ""
                while i < lines.count {
                    guard let (marker, rest, column) = listMarker(lines[i]) else {
                        // A non-blank line under an item that starts no
                        // block continues it.
                        let next = lines[i].trimmingCharacters(in: .whitespaces)
                        guard !next.isEmpty, !startsBlock(lines[i]), let last = blocks.indices.last else { break }
                        lastBody += (hard ? "\n" : " ") + next
                        blocks[last].inlines = parseInline(lastBody)
                        i += 1
                        continue
                    }
                    while let last = columns.last, column < last + 2 { columns.removeLast() }
                    columns.append(column)
                    var task: Bool? = nil
                    var body = rest
                    if rest.hasPrefix("[ ] ") || rest == "[ ]" { task = false; body = String(rest.dropFirst(3)) }
                    else if rest.hasPrefix("[x] ") || rest.hasPrefix("[X] ") || rest == "[x]" || rest == "[X]" { task = true; body = String(rest.dropFirst(3)) }
                    lastBody = body.trimmingCharacters(in: .whitespaces)
                    blocks.append(MarkdownBlock(kind: .listItem(marker: marker, task: task), inlines: parseInline(lastBody), listGroup: group, depth: columns.count - 1))
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
            if trimmed.isEmpty { flush() } else { para.append(String(line.drop(while: { $0 == " " || $0 == "\t" }))) }
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

    /// #VAPP-258: every top-level paragraph that is exactly ONE markdown
    /// link (all its runs carry that link) whose href passes `href` (the URL
    /// policy) and that `classify` (the host's `attachment`, asked with the
    /// raw destination) answers with a known kind and a non-empty name
    /// becomes an attachment block.
    public static func resolveAttachments(_ blocks: [MarkdownBlock], href: (String) -> String?, classify: (String) -> (kind: String, name: String)?) -> [MarkdownBlock] {
        blocks.map { b in
            guard b.kind == .paragraph else { return b }
            let runs = b.inlines.filter { !$0.text.isEmpty }
            guard let raw = runs.first?.link, runs.allSatisfy({ $0.link == raw }),
                  let policed = href(raw), let answer = classify(raw),
                  let kind = MarkdownAttachmentKind(rawValue: answer.kind), !answer.name.isEmpty else { return b }
            return MarkdownBlock(kind: .attachment(type: kind, name: answer.name, href: policed), inlines: [])
        }
    }

    /// #VAPP-258: each block image's natural size from `size` (the host's
    /// `imageSize`, asked with the raw src); a non-positive or non-finite
    /// answer is none.
    public static func sizeImages(_ blocks: [MarkdownBlock], size: (String) -> CGSize?) -> [MarkdownBlock] {
        blocks.map { b in
            guard case let .image(src, _) = b.kind else { return b }
            var out = b
            out.imageSize = size(src).flatMap(validImageSize)
            return out
        }
    }

    static func validImageSize(_ s: CGSize) -> CGSize? {
        s.width.isFinite && s.height.isFinite && s.width > 0 && s.height > 0 ? s : nil
    }

    /// #VAPP-258 (`imageBoxes` `loaded`): the size a block image's box uses:
    /// the host's, else the size it DECODED at (the media cache: an image the
    /// host cannot size takes its real aspect once loaded, one relayout from
    /// the placeholder box), else nil.
    public static func imageSize(host: CGSize?, loaded: CGSize?) -> CGSize? {
        host.flatMap(validImageSize) ?? loaded.flatMap(validImageSize)
    }

    /// #VAPP-258 (`imageBoxes`): a block image's box `available` wide. With
    /// a `maxHeight` cap: the natural size scaled by min(1, available /
    /// width, maxHeight / height), start-aligned; no (valid) size =
    /// available × maxHeight. With none (nil): the available width at the
    /// size's aspect (4:3 unknown), scaled down to `viewport` when taller.
    public static func imageBox(size: CGSize?, available: CGFloat, maxHeight: CGFloat?, viewport: CGFloat? = nil) -> CGSize {
        let avail = max(available, 0)
        let valid = size.flatMap(validImageSize)
        guard let cap = maxHeight else {
            let ratio = valid.map { $0.height / $0.width } ?? unsizedAspect
            let h = avail * ratio
            if let v = viewport, v.isFinite, v > 0, h > v { return CGSize(width: v / ratio, height: v) }
            return CGSize(width: avail, height: h)
        }
        guard let s = valid else { return CGSize(width: avail, height: cap) }
        let scale = min(1, avail / s.width, cap / s.height)
        return CGSize(width: s.width * scale, height: s.height * scale)
    }

    /// An unsized image's aspect (height / width) with no cap: 4:3.
    public static let unsizedAspect: CGFloat = 0.75

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

    /// `fixtures/markdown-text-links.json`: every text run through a host's
    /// text links: `segments` = one piece's answer, policed, nil = text. Code,
    /// link labels, block images and bare urls never ask; only the marks
    /// change, so the layout is the same.
    public static func linkText(_ blocks: [MarkdownBlock], _ segments: (String) -> [(text: String, href: String?)]?) -> [MarkdownBlock] {
        linkText(blocks, chips: { piece in segments(piece)?.map { (text: $0.text, href: $0.href, chip: nil) } })
    }

    /// #VAPP-258: `linkText` whose segments may carry a chip: kept only on a
    /// segment with an (allowed) href and when valid (`MarkdownChip.isValid`).
    public static func linkText(_ blocks: [MarkdownBlock], chips segments: (String) -> [(text: String, href: String?, chip: MarkdownChip?)]?) -> [MarkdownBlock] {
        func map(_ spans: [MarkdownInline]) -> [MarkdownInline] {
            spans.flatMap { span -> [MarkdownInline] in
                guard !span.code, span.link == nil else { return [span] }
                return bareUrlPieces(span.text).flatMap { piece, isUrl -> [MarkdownInline] in
                    var plain = span
                    plain.text = piece
                    guard !isUrl, let linked = segments(piece) else { return [plain] }
                    return linked.map { segment in
                        var out = span
                        out.text = segment.text
                        out.link = segment.href
                        out.chip = segment.href != nil && segment.chip?.isValid == true ? segment.chip : nil
                        return out
                    }
                }
            }
        }
        return blocks.map { b in
            var out = b
            switch b.kind {
            case .paragraph, .heading, .listItem, .quote:
                out.inlines = map(b.inlines)
            case let .table(header, rows):
                out.kind = .table(header: header.map(map), rows: rows.map { $0.map(map) })
            case .codeBlock, .rule, .image, .attachment:
                break
            }
            return out
        }
    }

    /// A run cut around its bare urls (`http://` or `https://`, any case, not
    /// glued to an ASCII word character, up to the next ASCII space, tab or
    /// newline); no empty piece.
    static func bareUrlPieces(_ text: String) -> [(String, Bool)] {
        let units = Array(text.utf8)
        func isWord(_ c: UInt8) -> Bool { (c >= 48 && c <= 57) || (c >= 65 && c <= 90) || (c >= 97 && c <= 122) || c == 95 }
        func scheme(at i: Int) -> Bool {
            for prefix in ["http://", "https://"] {
                let p = Array(prefix.utf8)
                if i + p.count <= units.count, (0..<p.count).allSatisfy({ (units[i + $0] >= 65 && units[i + $0] <= 90 ? units[i + $0] + 32 : units[i + $0]) == p[$0] }) { return true }
            }
            return false
        }
        var out: [(String, Bool)] = []
        func piece(_ from: Int, _ to: Int, _ url: Bool) {
            if to > from { out.append((String(decoding: units[from..<to], as: UTF8.self), url)) }
        }
        var at = 0
        var i = 0
        while i < units.count {
            guard i == 0 || !isWord(units[i - 1]), scheme(at: i) else {
                i += 1
                continue
            }
            var end = i
            while end < units.count, units[end] != 32, units[end] != 9, units[end] != 10 { end += 1 }
            piece(at, i, false)
            piece(i, end, true)
            at = end
            i = end
        }
        piece(at, units.count, false)
        return out
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
    /// Round 9 §9.1: a paragraph's type under the inline profile (the Text
    /// recipe of its `variant`); nil = `body`.
    public var paragraph: MarkdownTextSpec?
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
    /// #VAPP-258: a heading's type and margins per `level` (1...6; the
    /// `Markdown/heading` rules with the painter-supplied `level`); a level
    /// absent = `heading` / `headingTop` / `headingBottom`.
    public var headingLevels: [Int: MarkdownTextSpec] = [:]
    public var headingTops: [Int: CGFloat] = [:]
    public var headingBottoms: [Int: CGFloat] = [:]
    /// #VAPP-258: a list item's `Markdown/listItem` margins (they collapse
    /// with each other and, at a list's end, with the block gap).
    public var itemTop: CGFloat = 0
    public var itemBottom: CGFloat = 0
    /// #VAPP-258: a fenced block's border (`Markdown/codeBlock` borderWidth),
    /// inside its box: the box grows by it on each side.
    public var codeBorder: CGFloat = 0
    /// An image block's box height (the picture fits inside it); #VAPP-258:
    /// `imageMaxHeight`, the cap of a host-sized image; nil = no cap (the
    /// host's `markdown.options.imageMaxHeight: null`).
    public var imageHeight: CGFloat?
    /// #VAPP-258: a block image's corner radius (the `Markdown/image`
    /// recipe's `borderRadius`; 0 without a theme).
    public var imageRadius: CGFloat = 0
    /// #VAPP-258: the viewport height an uncapped image stops at (nil = unknown, no limit).
    public var viewportHeight: CGFloat?
    /// #VAPP-258: the inline chip's metrics.
    public var chip: MarkdownChipStyle
    /// #VAPP-258: a video attachment's box: min(available, `mediaWidth`)
    /// wide, `mediaAspectRatio` (layout.json).
    public var mediaWidth: CGFloat = 320
    public var mediaAspectRatio: CGFloat = 1.7777778
    /// An audio attachment: the title line (body), `audioTitleGap`, the
    /// controls row (`$control.row`).
    public var audioTitleGap: CGFloat = 4
    public var audioControls: CGFloat = 32
    /// A file attachment's row: the file icon (`$control.iconSm`), `fileGap`
    /// ($spacing.sm), the name (body, one line).
    public var fileIcon: CGFloat = 16
    public var fileGap: CGFloat = 8

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
        chip = MarkdownChipStyle()
    }

    /// An attachment block's box `available` wide.
    public func attachmentBox(_ type: MarkdownAttachmentKind, available: CGFloat) -> CGSize {
        switch type {
        case .video:
            let w = max(min(available, mediaWidth), 0)
            return CGSize(width: w, height: w / mediaAspectRatio)
        case .audio:
            return CGSize(width: available, height: body.lineHeight + audioTitleGap + audioControls)
        case .file:
            return CGSize(width: available, height: max(fileIcon, body.lineHeight))
        }
    }

    /// A list item's indent before its body: one `listIndent` per level
    /// plus the marker column.
    public func listInset(_ block: MarkdownBlock) -> CGFloat {
        listIndent * CGFloat(block.depth + 1)
    }

    /// A heading `level` clamped to 1...6.
    static func levelSlot(_ level: Int) -> Int { min(max(level, 1), 6) }

    public func spec(of kind: MarkdownBlockKind) -> MarkdownTextSpec {
        switch kind {
        case let .heading(level): headingLevels[Self.levelSlot(level)] ?? heading
        case .codeBlock: code
        case .paragraph: paragraph ?? body
        default: body
        }
    }

    func margins(_ kind: MarkdownBlockKind) -> (CGFloat, CGFloat) {
        switch kind {
        case let .heading(level): return (headingTops[Self.levelSlot(level)] ?? headingTop, headingBottoms[Self.levelSlot(level)] ?? headingBottom)
        case .listItem: return (itemTop, max(itemBottom, blockGap))
        default: return (0, blockGap)
        }
    }

    /// The gap above block `i` (margins collapse; first/last margins drop;
    /// inside a list, the items' own margins).
    public func gapBefore(_ blocks: [MarkdownBlock], _ i: Int) -> CGFloat {
        if i == 0 { return 0 }
        let prev = blocks[i - 1], cur = blocks[i]
        if prev.listGroup != nil, prev.listGroup == cur.listGroup { return max(itemBottom, itemTop) }
        return max(margins(prev.kind).1, margins(cur.kind).0)
    }
}

/// Round 9 (`fixtures/markdown-inline.json` `documentPaint`): a bullet's
/// width and its centre in the indent, in em.
public enum MarkdownBullet {
    public static let width: CGFloat = 0.36
    public static let centre: CGFloat = 0.46
}

/// Round 9 (`fixtures/markdown-inline.json` `documentPaint`): inline code,
/// the `Markdown/code` recipe: its type (`size`; nil = 0.925 em of the run),
/// a `radius`-cornered fill, `paddingH` (nil = 0.3 em) reserved in the line on
/// each side and `paddingV` over the font's own ascent and descent, as the
/// web's inline box. On `MarkdownChipStyle`: every measurer gets that one.
public struct MarkdownCodeStyle: Equatable, Sendable {
    public var size: CGFloat?
    public var paddingH: CGFloat?
    public var paddingV: CGFloat
    public var radius: CGFloat

    public init(size: CGFloat? = nil, paddingH: CGFloat? = nil, paddingV: CGFloat = 0, radius: CGFloat = 4) {
        self.size = size
        self.paddingH = paddingH
        self.paddingV = paddingV
        self.radius = radius
    }

    public func size(_ base: MarkdownTextSpec) -> CGFloat { size ?? base.size * 0.925 }
    public func paddingH(_ base: MarkdownTextSpec) -> CGFloat { paddingH ?? base.size * 0.3 }
}

/// #VAPP-258: the inline chip (`catalog/host.json` markdown): a box of
/// height min(`height` = `$control.badge`, the line height), `paddingH`
/// ($spacing.sm) each side, the optional icon (`icon` = `$control.iconSm`)
/// then `gap` ($spacing.xs), the label in `label` (the Text caption).
public struct MarkdownChipStyle: Equatable, Sendable {
    public var label: MarkdownTextSpec
    public var paddingH: CGFloat
    public var gap: CGFloat
    public var icon: CGFloat
    public var height: CGFloat
    /// #VAPP-258: the `ident`'s type (`Markdown/chipIdent`; nil = the mono
    /// family at the label's size).
    public var ident: MarkdownTextSpec?
    /// #VAPP-258: the box's corner radius (nil = a capsule).
    public var radius: CGFloat?
    /// #VAPP-258: a theme's `Markdown/chip*` rules per chip `kind` (the
    /// metrics; the painter resolves the colours itself).
    public var kinds: [String: MarkdownChipKindStyle] = [:]
    /// Round 9: inline code.
    public var code = MarkdownCodeStyle()

    public init(label: MarkdownTextSpec = MarkdownTextSpec(size: 12, lineHeight: 16, weight: 400, family: nil), paddingH: CGFloat = 8, gap: CGFloat = 4, icon: CGFloat = 16, height: CGFloat = 20) {
        self.label = label
        self.paddingH = paddingH
        self.gap = gap
        self.icon = icon
        self.height = height
    }

    /// The style a chip of `kind` lays out with: its kind's overrides on top.
    public func resolved(_ kind: String) -> MarkdownChipStyle {
        guard let k = kinds[kind] else { return self }
        var s = self
        if let v = k.label { s.label = v }
        if let v = k.ident { s.ident = v }
        if let v = k.paddingH { s.paddingH = v }
        if let v = k.radius { s.radius = v }
        s.kinds = [:]
        return s
    }

    /// The ident's type: its own, else the mono family at the label's size.
    public var identSpec: MarkdownTextSpec {
        ident ?? MarkdownTextSpec(size: label.size, lineHeight: label.lineHeight, weight: label.weight, family: "ui-monospace")
    }

    /// The chip's width around a label `labelWidth` wide (and an ident
    /// `identWidth` wide), capped at `wrap`.
    public func width(_ chip: MarkdownChip, labelWidth: CGFloat, identWidth: CGFloat = 0, wrap: CGFloat?) -> CGFloat {
        let w = 2 * paddingH + labelWidth + (chip.icon == nil ? 0 : icon + gap) + (chip.ident == nil ? 0 : identWidth + gap)
        return wrap.map { min(w, max($0, 1)) } ?? w
    }

    /// Its height in a line `lineHeight` tall.
    public func height(lineHeight: CGFloat) -> CGFloat { min(height, lineHeight) }
}

/// #VAPP-258: one chip kind's `Markdown/chip*` metrics (nil = the base).
public struct MarkdownChipKindStyle: Equatable, Sendable {
    public var label: MarkdownTextSpec?
    public var ident: MarkdownTextSpec?
    public var paddingH: CGFloat?
    public var radius: CGFloat?

    public init(label: MarkdownTextSpec? = nil, ident: MarkdownTextSpec? = nil, paddingH: CGFloat? = nil, radius: CGFloat? = nil) {
        self.label = label
        self.ident = ident
        self.paddingH = paddingH
        self.radius = radius
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
    /// #VAPP-258: an image / video box's width (nil = the full width).
    public var width: CGFloat? = nil
    /// #FEED-224: tables: each column's CONTENT width (`Markdown.tableColumns`).
    public var cols: [CGFloat] = []
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
            var boxWidth: CGFloat? = nil
            var cols: [CGFloat] = []
            let height: CGFloat
            switch b.kind {
            case .paragraph, .heading:
                height = text.height(b.inlines, spec, width: max(width, 1))
            case .listItem:
                height = text.height(b.inlines, spec, width: max(width - s.listInset(b), 1))
            case .quote:
                height = text.height(b.inlines, spec, width: max(width - s.quoteBorder - s.quotePad, 1))
            case .codeBlock:
                height = text.height(b.inlines, spec, width: nil) + 2 * (s.codePad + s.codeBorder)
            case .rule:
                height = 1
            case .image:
                let box = imageBox(size: b.imageSize, available: width, maxHeight: s.imageHeight, viewport: s.viewportHeight)
                if b.imageSize != nil || s.imageHeight == nil { boxWidth = box.width }
                height = box.height
            case let .attachment(type, _, _):
                let box = s.attachmentBox(type, available: width)
                if type == .video { boxWidth = box.width }
                height = box.height
            case let .table(header, trs):
                // #FEED-224: each column its fitted width, not an equal share.
                cols = tableColumns(header: header, rows: trs, s, width: width, text: text)
                // #EXP-1728: the header row wraps in the semibold it paints in.
                var head = s.body
                head.weight = max(head.weight, 600)
                for (r, row) in ([header] + trs).enumerated() {
                    let h = row.enumerated().map { c, cell in text.height(cell, r == 0 ? head : s.body, width: max(c < cols.count ? cols[c] : 1, 1)) }.reduce(s.body.lineHeight, max)
                    rows.append(h + 2 * s.cellPadV + 1)
                }
                height = rows.reduce(0, +) + 1
            }
            out.append(MarkdownBlockBox(y: y, height: height, rows: rows, width: boxWidth, cols: cols))
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
            case .image:
                if let size = b.imageSize { return imageBox(size: size, available: size.width, maxHeight: s.imageHeight ?? size.height, viewport: s.viewportHeight).width }
                return ((s.imageHeight ?? 160) * 1.5).rounded()
            case let .attachment(type, name, _):
                switch type {
                case .video: return s.mediaWidth
                case .audio: return 300
                case .file: return s.fileIcon + s.fileGap + text.width([MarkdownInline(text: name)], s.body)
                }
            case .codeBlock:
                let first = b.inlines.first ?? MarkdownInline(text: "", code: true, fenced: true)
                return (first.text.components(separatedBy: "\n").map { text.width([MarkdownInline(text: $0, code: true, fenced: true)], spec) }.max() ?? 0) + 2 * (s.codePad + s.codeBorder)
            case .rule: return 0
            case let .table(header, rows):
                // #FEED-224: every column on one line.
                return tableWidth(tableColumns(header: header, rows: rows, s, width: .infinity, text: text), s)
            }
        }.max().map { ceil($0) } ?? 0
    }

    /// A table's column count.
    static func tableColumnCount(header: [[MarkdownInline]], rows: [[[MarkdownInline]]]) -> Int {
        max(header.count, rows.map(\.count).max() ?? 0, 1)
    }

    /// #FEED-224 (`fixtures/table-fit.json`): each column's CONTENT width in
    /// a box `width` wide (whole px): each column's min (its widest
    /// unbreakable piece, a chip whole) and max (its widest cell on one
    /// line), over the header in its semibold and the body, through
    /// `MarkdownTableFit`; the cells' horizontal paddings come off the box
    /// (no vertical borders).
    static func tableColumns(header: [[MarkdownInline]], rows: [[[MarkdownInline]]], _ s: MarkdownStyles, width: CGFloat, text: MarkdownTextMeasure) -> [CGFloat] {
        let n = tableColumnCount(header: header, rows: rows)
        var head = s.body
        head.weight = max(head.weight, 600)
        var fit = Array(repeating: MarkdownTableFit.Column(min: 0, max: 0), count: n)
        for (r, row) in ([header] + rows).enumerated() {
            for (c, cell) in row.enumerated() where c < n && !cell.allSatisfy({ $0.text.isEmpty }) {
                let spec = r == 0 ? head : s.body
                fit[c].min = max(fit[c].min, Double(text.widestWord(cell, spec)))
                fit[c].max = max(fit[c].max, Double(text.width(cell, spec)))
            }
        }
        let available = Double(width) - Double(n) * 2 * Double(s.cellPadH)
        return MarkdownTableFit.fit(fit, available: available).map { CGFloat($0) }
    }

    /// #FEED-224: a table's painted width from its column content widths.
    static func tableWidth(_ cols: [CGFloat], _ s: MarkdownStyles) -> CGFloat {
        cols.reduce(0) { $0 + $1 + 2 * s.cellPadH }
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
            case let .attachment(type, _, _): return type == .file ? s.fileIcon : (type == .audio ? 160 : 1)
            case .quote: return text.widestWord(b.inlines, spec) + s.quoteBorder + s.quotePad
            default: return text.widestWord(b.inlines, spec)
            }
        }.max().map { ceil($0) } ?? 0
    }
}
