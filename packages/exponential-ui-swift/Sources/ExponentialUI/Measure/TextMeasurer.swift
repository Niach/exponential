import Foundation
import CoreText
import ExponentialUIPrimitives

#if canImport(UIKit)
import UIKit
#elseif canImport(AppKit)
import AppKit
#endif

// MARK: - Fonts the way CSS picks them

/// The face a text style lays out with, picked the way a browser does
/// (CSS Fonts 4 §5.2 "font matching"), so a theme's `fontWeight: 500` in a
/// family without a 500 face gets the 400 face Chromium uses (CoreText's
/// nearest-weight descriptor match would pick the 600 one, wider text).
///
/// Families resolve through, in order: the installed `TextFonts.Set` (a host
/// that ships its fonts; the conformance harness installs exactly
/// `conformance/fonts.json`), else the faces CoreText knows for the family
/// name, else the system font. Generic names follow CSS: `system-ui` /
/// `sans-serif` = the set's default (else the system font), `ui-monospace` /
/// `monospace` = the set's alias (else the system monospaced font).
public enum TextFonts {
    /// One face of a family.
    public struct Face: @unchecked Sendable {
        public let weight: Int
        public let italic: Bool
        public let descriptor: CTFontDescriptor

        public init(weight: Int, italic: Bool, descriptor: CTFontDescriptor) {
            self.weight = weight
            self.italic = italic
            self.descriptor = descriptor
        }
    }

    /// An explicit font set: family name → its faces, theme family names →
    /// the family that stands in for them (`Nunito` → `Inter`), and the
    /// default family (the system font and every unmapped family). With
    /// `exclusive`, nothing outside the set is ever used.
    public struct Set: @unchecked Sendable {
        public var faces: [String: [Face]]
        public var aliases: [String: String]
        public var defaultFamily: String?
        public var exclusive: Bool

        public init(faces: [String: [Face]] = [:], aliases: [String: String] = [:], defaultFamily: String? = nil, exclusive: Bool = false) {
            self.faces = faces
            self.aliases = aliases
            self.defaultFamily = defaultFamily
            self.exclusive = exclusive
        }

        /// The faces of the font files at `urls`, keyed by `family` (the
        /// files are also registered with the process so views that name the
        /// family find them). Weight = the face's OS/2 `usWeightClass`.
        public mutating func add(family: String, files urls: [URL]) {
            for url in urls {
                ExponentialUIFonts.register(url: url)
                guard let descs = CTFontManagerCreateFontDescriptorsFromURL(url as CFURL) as? [CTFontDescriptor] else { continue }
                for d in descs { faces[family, default: []].append(TextFonts.face(of: d)) }
            }
        }
    }

    nonisolated(unsafe) private static var installed: Set?
    nonisolated(unsafe) private static var familyCache: [String: [Face]] = [:]
    nonisolated(unsafe) private static var fontCache: [String: CTFont] = [:]
    nonisolated(unsafe) private static var epochValue: UInt64 = 0
    private static let lock = NSLock()

    /// Install (or with nil remove) an explicit font set. Every shaped width
    /// is re-derived; surfaces re-measure on their next pass (`epoch`).
    public static func install(_ set: Set?) {
        lock.lock()
        installed = set
        familyCache.removeAll()
        fontCache.removeAll()
        epochValue &+= 1
        lock.unlock()
        TextShaper.clearCache()
    }

    /// Bumped by every `install`: part of the measurer identity.
    public static var epoch: UInt64 {
        lock.lock(); defer { lock.unlock() }
        return epochValue
    }

    static func face(of d: CTFontDescriptor) -> Face {
        let font = CTFontCreateWithFontDescriptor(d, 12, nil)
        var weight = 400
        if let table = CTFontCopyTable(font, CTFontTableTag(kCTFontTableOS2), []) as Data?, table.count >= 6 {
            weight = Int(table[4]) << 8 | Int(table[5])
        } else if let traits = CTFontCopyTraits(font) as? [CFString: Any], let w = traits[kCTFontWeightTrait] as? CGFloat {
            weight = w >= 0.62 ? 900 : w >= 0.5 ? 800 : w >= 0.35 ? 700 : w >= 0.25 ? 600 : w >= 0.15 ? 500 : w > -0.15 ? 400 : w > -0.3 ? 300 : 200
        }
        let italic = CTFontGetSymbolicTraits(font).contains(.traitItalic)
        return Face(weight: weight, italic: italic, descriptor: d)
    }

    private static let sansGenerics: Swift.Set<String> = ["system-ui", "sans-serif", "ui-sans-serif", "-apple-system", "BlinkMacSystemFont"]
    private static let monoGenerics: Swift.Set<String> = ["ui-monospace", "monospace", "ui-mono"]

    /// The family `name` resolves to under the installed set: a family with
    /// faces, or nil (= the system font; `mono` = the system monospaced one).
    static func resolve(_ name: String?) -> (family: String?, mono: Bool) {
        let raw = name?.trimmingCharacters(in: .whitespaces) ?? ""
        let set = installed
        // A CSS stack (`"Inter, system-ui"`): the first family that resolves.
        if raw.contains(",") {
            for part in raw.split(separator: ",") {
                let n = part.trimmingCharacters(in: CharacterSet(charactersIn: " \"'"))
                let r = resolve(n)
                if r.family != nil || monoGenerics.contains(n) || sansGenerics.contains(n) { return r }
            }
            return (set?.defaultFamily, false)
        }
        let isMono = monoGenerics.contains(raw)
        if raw.isEmpty || sansGenerics.contains(raw) {
            if let a = set?.aliases[raw], !raw.isEmpty { return (a, false) }
            return (set?.defaultFamily, false)
        }
        let mapped = set?.aliases[raw] ?? raw
        if let set, set.faces[mapped] != nil { return (mapped, isMono) }
        if set?.exclusive == true { return (set?.defaultFamily, false) }
        if isMono { return (nil, true) }
        if !faces(of: mapped).isEmpty { return (mapped, false) }
        return (set?.defaultFamily, false)
    }

    /// The faces of a family (the installed set's, else CoreText's).
    static func faces(of family: String) -> [Face] {
        lock.lock()
        if let f = installed?.faces[family] { lock.unlock(); return f }
        if let hit = familyCache[family] { lock.unlock(); return hit }
        lock.unlock()
        let probe = CTFontDescriptorCreateWithAttributes([kCTFontFamilyNameAttribute: family] as CFDictionary)
        let found = (CTFontDescriptorCreateMatchingFontDescriptors(probe, Swift.Set([kCTFontFamilyNameAttribute as String]) as CFSet) as? [CTFontDescriptor]) ?? []
        let list = found.filter { (CTFontDescriptorCopyAttribute($0, kCTFontFamilyNameAttribute) as? String) == family }.map(face(of:))
        lock.lock()
        familyCache[family] = list
        lock.unlock()
        return list
    }

    /// CSS Fonts 4 §5.2 step 4: the style first (italic faces for italic
    /// text when any exist), then the weight: exact; for 400…500 the
    /// weights up to 500 ascending, then below descending, then above;
    /// below 400 descending first; above 500 ascending first.
    public static func match(_ faces: [Face], weight w: Int, italic: Bool) -> Face? {
        let styled = faces.filter { $0.italic == italic }
        let pool = styled.isEmpty ? faces : styled
        guard !pool.isEmpty else { return nil }
        if let exact = pool.first(where: { $0.weight == w }) { return exact }
        let below = pool.filter { $0.weight < w }.sorted { $0.weight > $1.weight }
        let above = pool.filter { $0.weight > w }.sorted { $0.weight < $1.weight }
        if w >= 400, w <= 500 {
            if let up = above.first(where: { $0.weight <= 500 }) { return up }
            if let down = below.first { return down }
            return above.first
        }
        if w < 400 { return below.first ?? above.first }
        return above.first ?? below.first
    }

    /// The CoreText font for a family NAME, CSS weight, size and style.
    public static func font(family: String?, weight: Int, size: CGFloat, italic: Bool = false) -> CTFont {
        let key = "\(family ?? "")|\(weight)|\(size)|\(italic)"
        lock.lock()
        if let hit = fontCache[key] { lock.unlock(); return hit }
        lock.unlock()
        let (resolved, mono) = resolve(family)
        let font: CTFont
        if let resolved, let face = match(faces(of: resolved), weight: weight, italic: italic) {
            font = CTFontCreateWithFontDescriptor(face.descriptor, size, nil)
        } else if mono {
            font = ExponentialUIFonts.monospacedFont(weight: weight, size: size, italic: italic) as CTFont
        } else {
            font = ExponentialUIFonts.systemFont(weight: weight, size: size, italic: italic) as CTFont
        }
        lock.lock()
        fontCache[key] = font
        lock.unlock()
        return font
    }

    /// The same font as a platform font (TextKit attributes).
    public static func platformFont(family: String?, weight: Int, size: CGFloat, italic: Bool = false) -> PlatformFont {
        font(family: family, weight: weight, size: size, italic: italic) as PlatformFont
    }
}

// MARK: - The shaper

/// Text shaping for the measurer AND the text leaves, laid out the way the
/// web lays text out (the gpui painter's `text.rs` rules, the reference the
/// conformance harness checks against Chromium):
///
/// - **Widths** are CoreText's typographic advances (kerning and ligatures
///   on, as HarfBuzz), unrounded but for Chromium's 1/64 px layout unit.
/// - **Break opportunities** are UAX #14 (CFStringTokenizer's line-break
///   unit), except after a `/` a word follows (URLs stay whole).
/// - **Min-content** = the widest unbreakable segment, **max-content** = the
///   widest hard line; trailing spaces HANG (never counted).
/// - **Wrapping** is greedy over those segments with a 0.5 px slack (gpui);
///   a segment wider than the line overflows on its own line.
/// - **Line height** follows CSS: `n` lines = `n × lineHeight`, the glyphs
///   centred in the line box (half-leading).
public enum TextShaper {
    /// The attributes a text style renders with.
    public static func attributes(_ ts: TextStyle, italic: Bool = false, family: String? = nil, align: NSTextAlignment = .natural, lineBreak: NSLineBreakMode = .byWordWrapping) -> [NSAttributedString.Key: Any] {
        let font = TextFonts.platformFont(family: family ?? ts.fontFamily, weight: ts.fontWeight, size: ts.fontSize, italic: italic || ts.italic)
        let p = NSMutableParagraphStyle()
        p.minimumLineHeight = ts.lineHeight
        p.maximumLineHeight = ts.lineHeight
        p.lineBreakMode = lineBreak
        p.alignment = align
        p.lineBreakStrategy = []
        // Half-leading above and below (CSS): the glyph's box is centred in
        // the line box. TextKit places the baseline at the line's bottom
        // minus the descender when min/max heights pin the line, so shift it.
        let natural = font.ascender - font.descender
        let offset = max(0, (ts.lineHeight - natural) / 2)
        var attrs: [NSAttributedString.Key: Any] = [.font: font, .paragraphStyle: p, .baselineOffset: 0, .init("ExponentialUI.halfLeading"): offset]
        // CSS `letter-spacing`: extra advance after every character.
        if let ls = ts.letterSpacing, ls != 0 { attrs[.kern] = ls }
        return attrs
    }

    /// `text` as an attributed string in `ts`.
    public static func attributed(_ text: String, _ ts: TextStyle, align: NSTextAlignment = .natural, lineBreak: NSLineBreakMode = .byWordWrapping) -> NSAttributedString {
        NSAttributedString(string: text, attributes: attributes(ts, align: align, lineBreak: lineBreak))
    }

    // MARK: widths

    nonisolated(unsafe) private static var widths: [String: CGFloat] = [:]
    private static let lock = NSLock()

    static func clearCache() {
        lock.lock()
        widths.removeAll()
        lock.unlock()
    }

    /// Chromium's layout unit: text widths ceil to 1/64 px.
    static func layoutUnit(_ v: CGFloat) -> CGFloat { (v * 64 - 0.0001).rounded(.up) / 64 }

    /// The advance width of ONE line of `text` (no newlines) in `ts`.
    public static func width(_ text: String, _ ts: TextStyle, italic: Bool = false) -> CGFloat {
        if text.isEmpty { return 0 }
        let italic = italic || ts.italic
        let spacing = ts.letterSpacing ?? 0
        let key = "\(ts.fontFamily ?? "")|\(ts.fontWeight)|\(ts.fontSize)|\(italic)|\(spacing)|\(text)"
        lock.lock()
        if let hit = widths[key] { lock.unlock(); return hit }
        lock.unlock()
        let font = TextFonts.font(family: ts.fontFamily, weight: ts.fontWeight, size: ts.fontSize, italic: italic)
        let line = CTLineCreateWithAttributedString(NSAttributedString(string: text, attributes: [NSAttributedString.Key(kCTFontAttributeName as String): font]))
        // Letter spacing follows every character (CSS), the last one too.
        let w = layoutUnit(CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil)) + spacing * CGFloat(text.count))
        lock.lock()
        if widths.count > 8192 { widths.removeAll() }
        widths[key] = w
        lock.unlock()
        return w
    }

    /// The width of one attributed line (no wrap): rich runs (markdown).
    public static func lineWidth(_ attributed: NSAttributedString) -> CGFloat {
        if attributed.length == 0 { return 0 }
        let line = CTLineCreateWithAttributedString(attributed)
        let trailing = CTLineGetTrailingWhitespaceWidth(line)
        return layoutUnit(CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil) - trailing))
    }

    /// Where the first baseline sits in a line box (half-leading + ascent),
    /// for `alignItems: baseline` (`FfiIntrinsics.baseline`).
    public static func baseline(_ ts: TextStyle) -> CGFloat {
        let font = TextFonts.font(family: ts.fontFamily, weight: ts.fontWeight, size: ts.fontSize)
        let ascent = CTFontGetAscent(font), descent = CTFontGetDescent(font)
        return max(0, (ts.lineHeight - (ascent + descent)) / 2 + ascent)
    }

    // MARK: segments (UAX #14)

    /// One unbreakable run: its text (trailing spaces included, the line
    /// feed excluded) and whether a HARD break follows it.
    struct Segment: Equatable {
        var text: String
        var hard: Bool
    }

    static func isNewline(_ c: Character) -> Bool { c == "\n" || c == "\r\n" || c == "\r" || c == "\u{2028}" || c == "\u{2029}" }

    /// The text cut at every break opportunity.
    static func segments(_ text: String) -> [Segment] {
        var out: [Segment] = []
        // Hard lines first (the tokenizer's mandatory breaks, simplified).
        var hardLines: [String] = []
        var current = ""
        for c in text {
            if isNewline(c) {
                hardLines.append(current)
                current = ""
            } else {
                current.append(c)
            }
        }
        hardLines.append(current)
        let endsWithBreak = text.last.map(isNewline) ?? false
        for (li, line) in hardLines.enumerated() {
            let last = li == hardLines.count - 1
            if last, endsWithBreak, line.isEmpty, li > 0 { break }
            var segs = softSegments(line)
            if segs.isEmpty { segs = [Segment(text: "", hard: false)] }
            if !last { segs[segs.count - 1].hard = true }
            out += segs
        }
        if out.isEmpty { out = [Segment(text: "", hard: false)] }
        // Tailoring: no break after a slash a word follows (URLs stay whole).
        var merged: [Segment] = []
        for seg in out {
            if var prev = merged.last, !prev.hard, prev.text.hasSuffix("/"), let first = seg.text.first, !first.isWhitespace {
                prev.text += seg.text
                prev.hard = seg.hard
                merged[merged.count - 1] = prev
            } else {
                merged.append(seg)
            }
        }
        return merged
    }

    /// The soft break opportunities of one hard line.
    private static func softSegments(_ line: String) -> [Segment] {
        if line.isEmpty { return [] }
        let ns = line as NSString
        let tokenizer = CFStringTokenizerCreate(nil, ns, CFRange(location: 0, length: ns.length), kCFStringTokenizerUnitLineBreak, nil)
        var starts: [Int] = []
        while CFStringTokenizerAdvanceToNextToken(tokenizer).rawValue != 0 {
            starts.append(CFStringTokenizerGetCurrentTokenRange(tokenizer).location)
        }
        if starts.first != 0 { starts.insert(0, at: 0) }
        var segs: [Segment] = []
        for (i, s) in starts.enumerated() {
            let e = i + 1 < starts.count ? starts[i + 1] : ns.length
            if e > s { segs.append(Segment(text: ns.substring(with: NSRange(location: s, length: e - s)), hard: false)) }
        }
        return segs
    }

    /// `s` without the spaces that hang at a line end.
    static func hang(_ s: String) -> String {
        var t = Substring(s)
        while let l = t.last, l == " " || l == "\t" { t = t.dropLast() }
        return String(t)
    }

    /// The text split at its hard breaks only.
    static func hardLines(_ text: String) -> [String] {
        var lines = [""]
        for s in segments(text) {
            lines[lines.count - 1] += s.text
            if s.hard { lines.append("") }
        }
        if lines.count > 1, lines.last == "", !text.hasSuffix("\n\n") { lines.removeLast() }
        return lines
    }

    /// Max-content width of `text` (the widest hard line, spaces hanging).
    public static func maxContent(_ text: String, _ ts: TextStyle) -> CGFloat {
        hardLines(text).map { width(hang($0), ts) }.max() ?? 0
    }

    /// Min-content width: the widest unbreakable segment.
    public static func minContent(_ text: String, _ ts: TextStyle) -> CGFloat {
        segments(text).map { width(hang($0.text), ts) }.max() ?? 0
    }

    /// Greedy line breaking at `wrap` (nil = the hard breaks only); each
    /// line without its hanging spaces. The lines a painter draws verbatim.
    public static func lines(_ text: String, _ ts: TextStyle, wrap: CGFloat?) -> [String] {
        guard let limit = wrap else { return hardLines(text).map(hang) }
        var lines: [String] = []
        var line = ""
        var lineFull: CGFloat = 0
        var open = false
        for s in segments(text) {
            let full = width(s.text, ts)
            let trimmedText = hang(s.text)
            let trimmed = trimmedText.count == s.text.count ? full : width(trimmedText, ts)
            if open, !line.isEmpty, lineFull + trimmed > limit + 0.5 {
                lines.append(hang(line))
                line = ""
                lineFull = 0
            }
            line += s.text
            lineFull += full
            open = true
            if s.hard {
                lines.append(hang(line))
                line = ""
                lineFull = 0
                open = false
            }
        }
        if open || lines.isEmpty { lines.append(hang(line)) }
        return lines
    }

    /// The height of `attributed` (rich runs) wrapped at `wrap` (nil = no
    /// wrap), at most `clamp` lines, never less than one line.
    public static func wrappedHeight(_ attributed: NSAttributedString, lineHeight: CGFloat, wrap: CGFloat?, clamp: Int? = nil) -> CGFloat {
        if attributed.length == 0 { return lineHeight }
        var n = richLineCount(attributed, wrap: wrap)
        if let clamp, clamp > 0 { n = min(n, clamp) }
        return CGFloat(max(n, 1)) * lineHeight
    }

    /// Lines of rich text at `wrap` with CoreText's typesetter (UAX #14,
    /// hanging spaces), hard breaks honoured.
    static func richLineCount(_ attributed: NSAttributedString, wrap: CGFloat?) -> Int {
        let ns = attributed.string as NSString
        var count = 0
        var start = 0
        let typesetter = CTTypesetterCreateWithAttributedString(attributed)
        while start <= ns.length {
            let nl = ns.range(of: "\n", options: [], range: NSRange(location: start, length: ns.length - start))
            let end = nl.location == NSNotFound ? ns.length : nl.location
            if end == start {
                count += 1
            } else if let wrap {
                var pos = start
                while pos < end {
                    let n = CTTypesetterSuggestLineBreak(typesetter, pos, Double(max(wrap, 1) + 0.5))
                    pos += max(n, 1)
                    count += 1
                }
            } else {
                count += 1
            }
            if nl.location == NSNotFound { break }
            start = end + 1
            if start == ns.length { break }
        }
        return max(count, 1)
    }

    /// Text content `(w, h)` at an inner wrap width (nil = max-content,
    /// 0 = min-content): a one-line text never wraps (its min-content is the
    /// whole line); `lines` clamps the line count.
    public static func measure(_ text: String, _ ts: TextStyle, wrap: CGFloat?, lines clampLines: Int?) -> CGSize {
        let text = ts.shown(text)
        let single = clampLines == 1
        let clamp = clampLines.flatMap { $0 > 0 ? $0 : nil }
        let lh = ts.lineHeight
        // An empty (or all-collapsible-space) text lays out NO line box in
        // CSS: the block is 0 px tall (an unbound template Text, an empty
        // field caption), not one line.
        if !text.contains(where: { !($0 == " " || $0 == "\t") }) { return .zero }
        func height(_ n: Int) -> CGFloat { lh * CGFloat(max(clamp.map { min(n, $0) } ?? n, 1)) }
        switch wrap {
        case .none:
            let n = single ? 1 : lines(text, ts, wrap: nil).count
            return CGSize(width: maxContent(text, ts), height: height(n))
        case .some(let w) where w <= 0:
            // A one-line (nowrap) text cannot break: its min-content is its
            // whole line, as CSS (round 2; Compose and the web).
            if single { return CGSize(width: maxContent(text, ts), height: lh) }
            let m = minContent(text, ts)
            return CGSize(width: m, height: height(lines(text, ts, wrap: m).count))
        case .some(let w):
            let maxW = maxContent(text, ts)
            if single { return CGSize(width: min(maxW, w), height: lh) }
            let used = min(maxW, max(w, minContent(text, ts)))
            return CGSize(width: used, height: height(lines(text, ts, wrap: max(w, used)).count))
        }
    }

    /// The runs of some markdown spans in a base spec (bold/italic/code/link).
    public static func runs(_ inlines: [MarkdownInline], _ spec: MarkdownTextSpec, mono: String?, ink: PlatformColor, link: PlatformColor, codeBackground: PlatformColor?, align: NSTextAlignment = .natural) -> NSAttributedString {
        let out = NSMutableAttributedString()
        let base = TextStyle(fontSize: spec.size, fontWeight: spec.weight, lineHeight: spec.lineHeight, fontFamily: spec.family)
        for span in inlines where !span.text.isEmpty {
            var ts = base
            if span.bold { ts.fontWeight = max(ts.fontWeight, 600) }
            let family = span.code ? (mono ?? "ui-monospace") : spec.family
            var attrs = attributes(ts, italic: span.italic, family: family, align: align)
            attrs[.foregroundColor] = span.link != nil ? link : ink
            if span.strike { attrs[.strikethroughStyle] = NSUnderlineStyle.single.rawValue }
            if span.link != nil { attrs[.underlineStyle] = NSUnderlineStyle.single.rawValue }
            if span.code, let codeBackground { attrs[.backgroundColor] = codeBackground }
            out.append(NSAttributedString(string: span.text, attributes: attrs))
        }
        return out
    }
}

#if canImport(UIKit)
public typealias PlatformColor = UIColor
#elseif canImport(AppKit)
public typealias PlatformColor = NSColor
#endif

/// Markdown measurement through the shaper (the painter's `MdText`).
struct MarkdownShaper: MarkdownTextMeasure {
    let mono: String?

    func height(_ inlines: [MarkdownInline], _ spec: MarkdownTextSpec, width: CGFloat?) -> CGFloat {
        let a = TextShaper.runs(inlines, spec, mono: mono, ink: .black, link: .black, codeBackground: nil)
        if a.length == 0 { return spec.lineHeight }
        return TextShaper.wrappedHeight(a, lineHeight: spec.lineHeight, wrap: width)
    }

    func width(_ inlines: [MarkdownInline], _ spec: MarkdownTextSpec) -> CGFloat {
        // Each hard line of the RICH runs (bold / code / italic keep their
        // faces): a plain re-shaping measured a bold paragraph narrower than
        // it lays out, so it wrapped at its own max-content width.
        let a = TextShaper.runs(inlines, spec, mono: mono, ink: .black, link: .black, codeBackground: nil)
        let ns = a.string as NSString
        var widest: CGFloat = 0
        var start = 0
        while start <= ns.length {
            let nl = ns.range(of: "\n", options: [], range: NSRange(location: start, length: ns.length - start))
            let end = nl.location == NSNotFound ? ns.length : nl.location
            widest = max(widest, TextShaper.lineWidth(a.attributedSubstring(from: NSRange(location: start, length: end - start))))
            if nl.location == NSNotFound { break }
            start = end + 1
        }
        return widest
    }

    func widestWord(_ inlines: [MarkdownInline], _ spec: MarkdownTextSpec) -> CGFloat {
        let ts = TextStyle(fontSize: spec.size, fontWeight: spec.weight, lineHeight: spec.lineHeight, fontFamily: spec.family)
        return TextShaper.minContent(Markdown.plain(inlines), ts)
    }
}
