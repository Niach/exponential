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
/// `monospace` = the set's alias (else the system monospaced font),
/// `ui-serif` / `serif` = the set's alias (else the system serif, New York).
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
    /// VAPP-162: `ui-serif` (every built-in theme's `type.family.serif`) and
    /// the CSS `serif` = the system serif (New York), like `ui-monospace`.
    private static let serifGenerics: Swift.Set<String> = ["ui-serif", "serif"]

    /// The family `name` resolves to under the installed set: a family with
    /// faces, or nil (= the system font; `mono` = the system monospaced one,
    /// `serif` = the system serif).
    static func resolve(_ name: String?) -> (family: String?, mono: Bool, serif: Bool) {
        let raw = name?.trimmingCharacters(in: .whitespaces) ?? ""
        let set = installed
        // A CSS stack (`"Inter, system-ui"`): the first family that resolves.
        if raw.contains(",") {
            for part in raw.split(separator: ",") {
                let n = part.trimmingCharacters(in: CharacterSet(charactersIn: " \"'"))
                let r = resolve(n)
                if r.family != nil || monoGenerics.contains(n) || serifGenerics.contains(n) || sansGenerics.contains(n) { return r }
            }
            return (set?.defaultFamily, false, false)
        }
        let isMono = monoGenerics.contains(raw)
        let isSerif = serifGenerics.contains(raw)
        if raw.isEmpty || sansGenerics.contains(raw) {
            if let a = set?.aliases[raw], !raw.isEmpty { return (a, false, false) }
            return (set?.defaultFamily, false, false)
        }
        let mapped = set?.aliases[raw] ?? raw
        if let set, set.faces[mapped] != nil { return (mapped, isMono, false) }
        if set?.exclusive == true { return (set?.defaultFamily, false, false) }
        if isMono { return (nil, true, false) }
        if isSerif { return (nil, false, true) }
        if !faces(of: mapped).isEmpty { return (mapped, false, false) }
        return (set?.defaultFamily, false, false)
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
    public static func font(family: String?, weight: Int, size: CGFloat, italic: Bool = false, tabular: Bool = false) -> CTFont {
        let key = "\(family ?? "")|\(weight)|\(size)|\(italic)|\(tabular)"
        lock.lock()
        if let hit = fontCache[key] { lock.unlock(); return hit }
        lock.unlock()
        let (resolved, mono, serif) = resolve(family)
        var font: CTFont
        if let resolved, let face = match(faces(of: resolved), weight: weight, italic: italic) {
            font = CTFontCreateWithFontDescriptor(face.descriptor, size, nil)
        } else if mono {
            font = ExponentialUIFonts.monospacedFont(weight: weight, size: size, italic: italic) as CTFont
        } else if serif {
            font = ExponentialUIFonts.serifFont(weight: weight, size: size, italic: italic) as CTFont
        } else {
            font = ExponentialUIFonts.systemFont(weight: weight, size: size, italic: italic) as CTFont
        }
        if tabular {
            // `tnum`: the monospaced-numbers selector of the number-spacing feature.
            let feature: [CFString: Any] = [kCTFontFeatureTypeIdentifierKey: kNumberSpacingType, kCTFontFeatureSelectorIdentifierKey: kMonospacedNumbersSelector]
            let d = CTFontDescriptorCreateWithAttributes([kCTFontFeatureSettingsAttribute: [feature]] as CFDictionary)
            font = CTFontCreateCopyWithAttributes(font, size, nil, d)
        }
        lock.lock()
        fontCache[key] = font
        lock.unlock()
        return font
    }

    /// The same font as a platform font (TextKit attributes).
    public static func platformFont(family: String?, weight: Int, size: CGFloat, italic: Bool = false, tabular: Bool = false) -> PlatformFont {
        font(family: family, weight: weight, size: size, italic: italic, tabular: tabular) as PlatformFont
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
        let font = TextFonts.platformFont(family: family ?? ts.fontFamily, weight: ts.fontWeight, size: ts.fontSize, italic: italic || ts.italic, tabular: ts.tabular)
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
        let key = "\(ts.fontFamily ?? "")|\(ts.fontWeight)|\(ts.fontSize)|\(italic)|\(ts.tabular)|\(spacing)|\(text)"
        lock.lock()
        if let hit = widths[key] { lock.unlock(); return hit }
        lock.unlock()
        let font = TextFonts.font(family: ts.fontFamily, weight: ts.fontWeight, size: ts.fontSize, italic: italic, tabular: ts.tabular)
        let line = CTLineCreateWithAttributedString(NSAttributedString(string: text, attributes: [NSAttributedString.Key(kCTFontAttributeName as String): font]))
        // Letter spacing follows every character (CSS), the last one too.
        let w = layoutUnit(CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil)) + spacing * CGFloat(text.count))
        lock.lock()
        // VAPP-142: room for a 10k template's texts (at 8192 the cache emptied
        // mid-pass and every text of a rebind was shaped twice).
        if widths.count > 65_536 { widths.removeAll() }
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
        // VAPP-142: without a line break the soft segments (one ICU line
        // tokenizer per call) only join back into the text itself.
        if !text.contains(where: isNewline) { return [text] }
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

    /// The attribute a markdown link's (policy-allowed) href rides on.
    static let linkKey = NSAttributedString.Key("ExponentialUI.link")

    static func hasLinks(_ a: NSAttributedString) -> Bool {
        var found = false
        a.enumerateAttribute(linkKey, in: NSRange(location: 0, length: a.length)) { v, _, stop in
            if v != nil {
                found = true
                stop.pointee = true
            }
        }
        return found
    }

    /// The link href under `point` of `a` laid out as the label lays it out
    /// (CoreText lines at `wrap`, each `lineHeight` tall).
    static func link(in a: NSAttributedString, lineHeight: CGFloat, wrap: CGFloat, at point: CGPoint) -> String? {
        guard a.length > 0, point.y >= 0 else { return nil }
        let target = Int(point.y / max(lineHeight, 1))
        let ns = a.string as NSString
        let typesetter = CTTypesetterCreateWithAttributedString(a)
        var line = 0
        var start = 0
        while start <= ns.length {
            let nl = ns.range(of: "\n", options: [], range: NSRange(location: start, length: ns.length - start))
            let end = nl.location == NSNotFound ? ns.length : nl.location
            var pos = start
            repeat {
                let n = end > pos ? max(CTTypesetterSuggestLineBreak(typesetter, pos, Double(max(wrap, 1) + 0.5)), 1) : 0
                if line == target {
                    guard n > 0 else { return nil }
                    let ct = CTTypesetterCreateLine(typesetter, CFRange(location: pos, length: n))
                    let idx = CTLineGetStringIndexForPosition(ct, CGPoint(x: point.x, y: 0))
                    let i = min(max(idx == kCFNotFound ? pos : idx, pos), pos + n - 1)
                    // The glyph left of the caret position when the tap sits on its right half.
                    let probe = i > pos && CTLineGetOffsetForStringIndex(ct, i, nil) > point.x ? i - 1 : i
                    guard point.x <= CTLineGetTypographicBounds(ct, nil, nil, nil) else { return nil }
                    return a.attribute(linkKey, at: probe, effectiveRange: nil) as? String
                }
                pos += n
                line += 1
            } while pos < end
            if nl.location == NSNotFound { break }
            start = end + 1
        }
        return nil
    }

    /// #VAPP-258: the attribute an inline chip's object replacement
    /// character carries (`MarkdownChipRun`).
    static let chipKey = NSAttributedString.Key("ExponentialUI.chip")

    /// The runs of some markdown spans in a base spec (bold/italic/code/link).
    /// #VAPP-258: with `chip` (the chip metrics), a span carrying a chip is
    /// ONE object replacement character as wide as the chip (capped at
    /// `wrap`): a CoreText run delegate for the measurer's typesetter and a
    /// clear text attachment for TextKit, the same width, so it breaks like
    /// one unbreakable word and never grows the line; the painter draws the
    /// chip over it (`chipRects`).
    public static func runs(_ inlines: [MarkdownInline], _ spec: MarkdownTextSpec, mono: String?, ink: PlatformColor, link: PlatformColor, codeBackground: PlatformColor?, align: NSTextAlignment = .natural, chip chipStyle: MarkdownChipStyle? = nil, wrap: CGFloat? = nil) -> NSAttributedString {
        let out = NSMutableAttributedString()
        let base = TextStyle(fontSize: spec.size, fontWeight: spec.weight, lineHeight: spec.lineHeight, fontFamily: spec.family)
        for span in inlines where !span.text.isEmpty {
            var ts = base
            if span.bold { ts.fontWeight = max(ts.fontWeight, 600) }
            let family = span.code ? (mono ?? "ui-monospace") : spec.family
            var attrs = attributes(ts, italic: span.italic, family: family, align: align)
            if let chip = span.chip, let chipStyle, let href = span.link {
                out.append(chipRun(chip, href: href, style: chipStyle, lineHeight: spec.lineHeight, wrap: wrap, attrs: attrs))
                continue
            }
            // Round 9: inline code = its type between its side padding (the
            // run's line keeps the block's half-leading and baseline).
            if span.code, !span.fenced, let chipStyle {
                let body = attrs
                var mono = ts
                mono.fontSize = chipStyle.code.size(spec)
                attrs = attributes(mono, italic: span.italic, family: family, align: align)
                attrs[halfLeadingKey] = body[halfLeadingKey]
                let font = attrs[.font] as? PlatformFont
                let run = MarkdownCodeRun(fill: codeBackground, radius: chipStyle.code.radius, ascent: (font?.ascender ?? spec.size) + chipStyle.code.paddingV, descent: -(font?.descender ?? 0) + chipStyle.code.paddingV)
                let pad = codePad(chipStyle.code.paddingH(spec), lineHeight: spec.lineHeight, run: run, attrs: body)
                attrs[.foregroundColor] = span.link != nil ? link : ink
                if span.strike { attrs[.strikethroughStyle] = NSUnderlineStyle.single.rawValue }
                if let href = span.link {
                    attrs[.underlineStyle] = NSUnderlineStyle.single.rawValue
                    attrs[Self.linkKey] = href
                }
                attrs[codeKey] = run
                // The word joiners keep the padding on the span's line.
                var glue = body
                glue[codeKey] = run
                out.append(NSAttributedString(string: "\u{FFFC}", attributes: pad))
                out.append(NSAttributedString(string: "\u{2060}", attributes: glue))
                out.append(NSAttributedString(string: span.text, attributes: attrs))
                out.append(NSAttributedString(string: "\u{2060}", attributes: glue))
                out.append(NSAttributedString(string: "\u{FFFC}", attributes: pad))
                continue
            }
            attrs[.foregroundColor] = span.link != nil ? link : ink
            if span.strike { attrs[.strikethroughStyle] = NSUnderlineStyle.single.rawValue }
            if let href = span.link {
                attrs[.underlineStyle] = NSUnderlineStyle.single.rawValue
                attrs[Self.linkKey] = href
            }
            if span.code, let codeBackground { attrs[.backgroundColor] = codeBackground }
            out.append(NSAttributedString(string: span.text, attributes: attrs))
        }
        return out
    }
}

/// Round 9: one inline code span's fill as painted (`codeRects`): above and
/// below the baseline, `paddingV` included.
final class MarkdownCodeRun: NSObject {
    let fill: PlatformColor?
    let radius: CGFloat
    let ascent: CGFloat
    let descent: CGFloat
    /// The side padding's run delegate reads this.
    var padding: CGFloat = 0

    init(fill: PlatformColor?, radius: CGFloat, ascent: CGFloat, descent: CGFloat) {
        self.fill = fill
        self.radius = radius
        self.ascent = ascent
        self.descent = descent
    }
}

/// #VAPP-258: an inline chip as laid out: what it shows, where it opens,
/// its box size.
final class MarkdownChipRun: NSObject {
    let chip: MarkdownChip
    let href: String
    let width: CGFloat
    let height: CGFloat
    let ascent: CGFloat
    let descent: CGFloat

    init(chip: MarkdownChip, href: String, width: CGFloat, height: CGFloat, ascent: CGFloat, descent: CGFloat) {
        self.chip = chip
        self.href = href
        self.width = width
        self.height = height
        self.ascent = ascent
        self.descent = descent
    }

    override func isEqual(_ object: Any?) -> Bool {
        guard let o = object as? MarkdownChipRun else { return false }
        return o.chip == chip && o.href == href && o.width == width && o.height == height
    }

    override var hash: Int { chip.hashValue ^ href.hashValue }
}

extension TextShaper {
    /// The chip label's text style (the Text caption).
    static func chipLabelStyle(_ style: MarkdownChipStyle) -> TextStyle {
        TextStyle(fontSize: style.label.size, fontWeight: style.label.weight, lineHeight: style.label.lineHeight, fontFamily: style.label.family)
    }

    /// #VAPP-258: the chip ident's text style (`Markdown/chipIdent`, else mono).
    static func chipIdentStyle(_ style: MarkdownChipStyle) -> TextStyle {
        let s = style.identSpec
        return TextStyle(fontSize: s.size, fontWeight: s.weight, lineHeight: s.lineHeight, fontFamily: s.family)
    }

    /// #VAPP-258: how far a line of `other` moves down so its baseline meets
    /// `base`'s when both line boxes are centred on one row (the glyph box
    /// centred in each line box, [`attributes`]' half-leading).
    static func baselineShift(_ base: TextStyle, _ other: TextStyle) -> CGFloat {
        func baseline(_ ts: TextStyle) -> CGFloat {
            let font = TextFonts.platformFont(family: ts.fontFamily, weight: ts.fontWeight, size: ts.fontSize, italic: ts.italic, tabular: ts.tabular)
            return -ts.lineHeight / 2 + max(0, (ts.lineHeight - (font.ascender - font.descender)) / 2) + font.ascender
        }
        return baseline(base) - baseline(other)
    }

    /// The chip's width: 2 × padding + the label shaped in the caption
    /// style (+ icon + gap) (+ the ident + gap), capped at `wrap`; a theme's
    /// rules for the chip's kind first.
    static func chipWidth(_ chip: MarkdownChip, style base: MarkdownChipStyle, wrap: CGFloat?) -> CGFloat {
        let style = base.resolved(chip.kind)
        return style.width(chip, labelWidth: width(chip.label, chipLabelStyle(style)), identWidth: chip.ident.map { width($0, chipIdentStyle(style)) } ?? 0, wrap: wrap)
    }

    static func chipRun(_ chip: MarkdownChip, href: String, style: MarkdownChipStyle, lineHeight: CGFloat, wrap: CGFloat?, attrs base: [NSAttributedString.Key: Any]) -> NSAttributedString {
        var attrs = base
        let w = chipWidth(chip, style: style, wrap: wrap)
        let h = style.height(lineHeight: lineHeight)
        let font = attrs[.font] as? PlatformFont
        let ascent = font.map { $0.ascender } ?? lineHeight * 0.8
        let descent = font.map { -$0.descender } ?? lineHeight * 0.2
        let run = MarkdownChipRun(chip: chip, href: href, width: w, height: h, ascent: ascent, descent: descent)
        attrs[chipKey] = run
        attrs[linkKey] = href
        // CoreText (the measurer's typesetter, hit-testing): the run delegate.
        var callbacks = CTRunDelegateCallbacks(version: kCTRunDelegateVersion1, dealloc: { ref in
            Unmanaged<MarkdownChipRun>.fromOpaque(ref).release()
        }, getAscent: { ref in
            Unmanaged<MarkdownChipRun>.fromOpaque(ref).takeUnretainedValue().ascent
        }, getDescent: { ref in
            Unmanaged<MarkdownChipRun>.fromOpaque(ref).takeUnretainedValue().descent
        }, getWidth: { ref in
            Unmanaged<MarkdownChipRun>.fromOpaque(ref).takeUnretainedValue().width
        })
        if let delegate = CTRunDelegateCreate(&callbacks, Unmanaged.passRetained(run).toOpaque()) {
            attrs[NSAttributedString.Key(kCTRunDelegateAttributeName as String)] = delegate
        }
        // TextKit (the label): a clear attachment as wide, no taller than the glyphs.
        let attachment = NSTextAttachment()
        attachment.image = clearImage
        attachment.bounds = CGRect(x: 0, y: -descent, width: w, height: min(h, ascent + descent))
        attrs[.attachment] = attachment
        return NSAttributedString(string: "\u{FFFC}", attributes: attrs)
    }

    /// Round 9: the attribute over an inline code span, its side padding included.
    static let codeKey = NSAttributedString.Key("ExponentialUI.code")
    private static let halfLeadingKey = NSAttributedString.Key("ExponentialUI.halfLeading")

    /// Round 9: the attributes of inline code's side padding: reserved like a
    /// chip (a CoreText run delegate for the measurer, a clear attachment for
    /// TextKit), in the block's font so its line keeps the block's baseline.
    private static func codePad(_ width: CGFloat, lineHeight: CGFloat, run: MarkdownCodeRun, attrs base: [NSAttributedString.Key: Any]) -> [NSAttributedString.Key: Any] {
        var attrs = base
        attrs[codeKey] = run
        run.padding = width
        var callbacks = CTRunDelegateCallbacks(version: kCTRunDelegateVersion1, dealloc: { ref in
            Unmanaged<MarkdownCodeRun>.fromOpaque(ref).release()
        }, getAscent: { _ in 0 }, getDescent: { _ in 0 }, getWidth: { ref in
            Unmanaged<MarkdownCodeRun>.fromOpaque(ref).takeUnretainedValue().padding
        })
        if let delegate = CTRunDelegateCreate(&callbacks, Unmanaged.passRetained(run).toOpaque()) {
            attrs[NSAttributedString.Key(kCTRunDelegateAttributeName as String)] = delegate
        }
        let font = base[.font] as? PlatformFont
        let ascent = font.map { $0.ascender } ?? lineHeight * 0.8
        let descent = font.map { -$0.descender } ?? lineHeight * 0.2
        let attachment = NSTextAttachment()
        attachment.image = clearImage
        attachment.bounds = CGRect(x: 0, y: -descent, width: width, height: ascent + descent)
        attrs[.attachment] = attachment
        return attrs
    }

    /// Round 9: each inline code span's fill per line it crosses, laid out as
    /// the label lays it out (`baseline` = the block's in its line box).
    static func codeRects(in a: NSAttributedString, lineHeight: CGFloat, baseline: CGFloat, wrap: CGFloat, rtl: Bool = false) -> [(fill: PlatformColor, radius: CGFloat, rect: CGRect)] {
        codeRects(in: TextLines(a, wrap: wrap), lineHeight: lineHeight, baseline: baseline, rtl: rtl)
    }

    static func codeRects(in lines: TextLines, lineHeight: CGFloat, baseline: CGFloat, rtl: Bool = false) -> [(fill: PlatformColor, radius: CGFloat, rect: CGRect)] {
        let a = lines.text, wrap = lines.wrap
        var spans: [(range: NSRange, run: MarkdownCodeRun, fill: PlatformColor)] = []
        a.enumerateAttribute(codeKey, in: NSRange(location: 0, length: a.length)) { v, range, _ in
            if let run = v as? MarkdownCodeRun, let fill = run.fill { spans.append((range, run, fill)) }
        }
        if spans.isEmpty { return [] }
        var out: [(fill: PlatformColor, radius: CGFloat, rect: CGRect)] = []
        let typesetter = lines.typesetter
        for (line, r) in lines.ranges.enumerated() where r.length > 0 {
            let ct = CTTypesetterCreateLine(typesetter, CFRange(location: r.location, length: r.length))
            let pen = rtl ? CGFloat(CTLineGetPenOffsetForFlush(ct, 1, Double(wrap))) : 0
            for span in spans {
                let part = NSIntersectionRange(span.range, r)
                guard part.length > 0 else { continue }
                let x0 = CTLineGetOffsetForStringIndex(ct, part.location, nil) + pen
                let x1 = CTLineGetOffsetForStringIndex(ct, NSMaxRange(part), nil) + pen
                let y = CGFloat(line) * lineHeight + baseline - span.run.ascent
                out.append((span.fill, span.run.radius, CGRect(x: min(x0, x1), y: y, width: abs(x1 - x0), height: span.run.ascent + span.run.descent)))
            }
        }
        return out
    }

    nonisolated(unsafe) static let clearImage: PlatformImage = {
        #if canImport(UIKit)
        return UIGraphicsImageRenderer(size: CGSize(width: 1, height: 1)).image { _ in }
        #else
        return NSImage(size: NSSize(width: 1, height: 1))
        #endif
    }()

    /// #VAPP-258: the lines of `a` at `wrap` as CoreText's typesetter breaks
    /// them (the measurer's lines, hard breaks honoured), each without its
    /// newline: a text with inline chips paints one label per line, so
    /// TextKit can never break it elsewhere than the measurer and
    /// `chipRects` did.
    static func lineRanges(_ a: NSAttributedString, wrap: CGFloat) -> [NSRange] {
        TextLines(a, wrap: wrap).ranges
    }

    /// `a` with a LINE SEPARATOR (U+2028) at each soft break of
    /// `lineRanges` (hard breaks kept): one selectable text that TextKit
    /// lays out on exactly the measurer's lines.
    static func withLineSeparators(_ a: NSAttributedString, wrap: CGFloat) -> NSAttributedString {
        withLineSeparators(TextLines(a, wrap: wrap))
    }

    static func withLineSeparators(_ lines: TextLines) -> NSAttributedString {
        let a = lines.text
        let ranges = lines.ranges
        guard ranges.count > 1 else { return a }
        let out = NSMutableAttributedString()
        for (i, r) in ranges.enumerated() {
            out.append(a.attributedSubstring(from: r))
            guard i + 1 < ranges.count else { continue }
            let next = ranges[i + 1].location
            if next > NSMaxRange(r) {
                out.append(a.attributedSubstring(from: NSRange(location: NSMaxRange(r), length: next - NSMaxRange(r))))
            } else {
                let at = max(NSMaxRange(r) - 1, 0)
                var attrs = a.attributes(at: at, effectiveRange: nil)
                attrs[chipKey] = nil
                attrs[codeKey] = nil
                attrs[.attachment] = nil
                attrs[NSAttributedString.Key(kCTRunDelegateAttributeName as String)] = nil
                out.append(NSAttributedString(string: "\u{2028}", attributes: attrs))
            }
        }
        return out
    }

    /// Inline chips or code: painted on the typesetter's lines.
    static func hasChips(_ a: NSAttributedString) -> Bool {
        var found = false
        for key in [chipKey, codeKey] where !found {
            a.enumerateAttribute(key, in: NSRange(location: 0, length: a.length)) { v, _, stop in
                if v != nil { found = true; stop.pointee = true }
            }
        }
        return found
    }

    /// #VAPP-258: each inline chip of `a` laid out as the label lays it out
    /// (CoreText lines at `wrap`, each `lineHeight` tall, flushed right under
    /// rtl): its box, vertically centred in its line.
    static func chipRects(in a: NSAttributedString, lineHeight: CGFloat, wrap: CGFloat, rtl: Bool = false) -> [(run: MarkdownChipRun, rect: CGRect)] {
        chipRects(in: TextLines(a, wrap: wrap), lineHeight: lineHeight, rtl: rtl)
    }

    static func chipRects(in lines: TextLines, lineHeight: CGFloat, rtl: Bool = false) -> [(run: MarkdownChipRun, rect: CGRect)] {
        let a = lines.text, wrap = lines.wrap
        guard a.length > 0 else { return [] }
        var chips: [Int: MarkdownChipRun] = [:]
        a.enumerateAttribute(chipKey, in: NSRange(location: 0, length: a.length)) { v, range, _ in
            if let run = v as? MarkdownChipRun { for i in range.location..<(range.location + range.length) { chips[i] = run } }
        }
        if chips.isEmpty { return [] }
        var out: [(run: MarkdownChipRun, rect: CGRect)] = []
        let typesetter = lines.typesetter
        for (line, r) in lines.ranges.enumerated() {
            // The typesetter's own break (a line's may take its newline).
            let pos = r.location, n = lines.breaks[line]
            guard n > 0, (pos..<(pos + n)).contains(where: { chips[$0] != nil }) else { continue }
            let ct = CTTypesetterCreateLine(typesetter, CFRange(location: pos, length: n))
            let pen = rtl ? CGFloat(CTLineGetPenOffsetForFlush(ct, 1, Double(wrap))) : 0
            for i in pos..<(pos + n) {
                guard let run = chips[i] else { continue }
                let x = CTLineGetOffsetForStringIndex(ct, i, nil) + pen
                let y = CGFloat(line) * lineHeight + (lineHeight - run.height) / 2
                out.append((run, CGRect(x: x, y: y, width: run.width, height: run.height)))
            }
        }
        return out
    }
}

/// One text block's runs at a wrap width, typeset ONCE: its CoreText lines
/// (the measurer's, hard breaks honoured) shared by the label, the inline
/// code fills, the chips and the selectable text, built on first use.
final class TextLines {
    let text: NSAttributedString
    let wrap: CGFloat

    init(_ text: NSAttributedString, wrap: CGFloat) {
        self.text = text
        self.wrap = wrap
    }

    private(set) lazy var typesetter: CTTypesetter = CTTypesetterCreateWithAttributedString(text)
    private var broken: (ranges: [NSRange], breaks: [Int])?

    /// Each line's range without its newline (`TextShaper.lineRanges`).
    var ranges: [NSRange] { lineBreaks().ranges }
    /// Each line's break as the typesetter suggests it (may run past a
    /// hard break by its newline).
    var breaks: [Int] { lineBreaks().breaks }

    private(set) lazy var hasLinks: Bool = TextShaper.hasLinks(text)
    private(set) lazy var hasChips: Bool = TextShaper.hasChips(text)

    private func lineBreaks() -> (ranges: [NSRange], breaks: [Int]) {
        if let broken { return broken }
        let ns = text.string as NSString
        var ranges: [NSRange] = []
        var breaks: [Int] = []
        if ns.length > 0 {
            var start = 0
            while start <= ns.length {
                let nl = ns.range(of: "\n", options: [], range: NSRange(location: start, length: ns.length - start))
                let end = nl.location == NSNotFound ? ns.length : nl.location
                var pos = start
                repeat {
                    let raw = end > pos ? max(CTTypesetterSuggestLineBreak(typesetter, pos, Double(max(wrap, 1) + 0.5)), 1) : 0
                    let n = min(raw, end - pos)
                    ranges.append(NSRange(location: pos, length: n))
                    breaks.append(raw)
                    pos += n
                } while pos < end
                if nl.location == NSNotFound { break }
                start = end + 1
            }
        }
        broken = (ranges, breaks)
        return (ranges, breaks)
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
    /// #VAPP-258: the inline chip's metrics (a chip run = its box's width).
    var chip = MarkdownChipStyle()

    func height(_ inlines: [MarkdownInline], _ spec: MarkdownTextSpec, width: CGFloat?) -> CGFloat {
        let a = TextShaper.runs(inlines, spec, mono: mono, ink: .black, link: .black, codeBackground: nil, chip: chip, wrap: width)
        if a.length == 0 { return spec.lineHeight }
        return TextShaper.wrappedHeight(a, lineHeight: spec.lineHeight, wrap: width)
    }

    func width(_ inlines: [MarkdownInline], _ spec: MarkdownTextSpec) -> CGFloat {
        // Each hard line of the RICH runs (bold / code / italic keep their
        // faces): a plain re-shaping measured a bold paragraph narrower than
        // it lays out, so it wrapped at its own max-content width.
        let a = TextShaper.runs(inlines, spec, mono: mono, ink: .black, link: .black, codeBackground: nil, chip: chip)
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
        // #VAPP-258: a chip is one unbreakable word as wide as its box.
        let chips = inlines.filter { $0.chip != nil && $0.link != nil }
        if chips.isEmpty { return TextShaper.minContent(Markdown.plain(inlines), ts) }
        let text = inlines.map { $0.chip != nil && $0.link != nil ? " " : $0.text }.joined()
        let widestChip = chips.compactMap { $0.chip.map { TextShaper.chipWidth($0, style: chip, wrap: nil) } }.max() ?? 0
        return max(TextShaper.minContent(text, ts), widestChip)
    }
}
