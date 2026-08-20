import Foundation
import UIKit

/// Pure formatting operations behind the editor's formatting rail (EXP-568).
///
/// Everything here mutates an `NSMutableAttributedString` (or transforms a
/// typing-attributes dictionary) and nothing else — no text view, no UIKit
/// chrome — so the whole feature set is unit-testable by round-tripping
/// through `MarkdownConversion`.
///
/// Two invariants the callers must not have to think about:
/// - INLINE marks never enter code (`.markdownInlineCode` / `.markdownCodeBlock`)
///   runs: the serializer emits `` `code` `` exclusive of the emphasis
///   delimiters, so a half-marked code run would silently lose the mark on the
///   next save. Bold additionally skips HEADING runs — the serializer ignores
///   bold inside a heading (`InlineRun.isHeading`), and rewriting the heading
///   font here would flatten it.
/// - LINE formats (heading / list / quote / code block) are orthogonal
///   attribute keys, so switching one on strips the others; the list path bakes
///   its `• ` / `☐ ` / `1. ` prefix into the TEXT, which is deleted with it.
public enum MarkdownFormatOps {

    // MARK: - Attribute groups

    /// The paragraph-level keys that are mutually exclusive with one another.
    private static let lineFormatKeys: [NSAttributedString.Key] = [
        .markdownHeadingLevel,
        .markdownListType,
        .markdownListItemIndex,
        .markdownListDepth,
        .markdownBlockquote,
        .markdownCodeBlock,
        .markdownCodeBlockLang,
    ]

    /// The plain paragraph style the editor uses everywhere it has no block
    /// decoration to draw (matches `MarkdownStyle.baseAttributes`).
    private static var bodyParagraphStyle: NSParagraphStyle {
        let style = NSMutableParagraphStyle()
        style.lineSpacing = 4
        return style
    }

    // MARK: - Range helpers

    private static func clamped(_ range: NSRange, in text: NSAttributedString) -> NSRange {
        let location = max(0, min(range.location, text.length))
        let length = max(0, min(range.length, text.length - location))
        return NSRange(location: location, length: length)
    }

    /// Marks apply to the non-whitespace core of a selection: `**a **` is not
    /// bold in GFM (the closing delimiter is not left-flanking), so a selection
    /// that swept up a trailing space would serialize to something that no
    /// longer parses back.
    private static func trimmedForMarks(_ range: NSRange, in text: NSAttributedString) -> NSRange {
        var r = clamped(range, in: text)
        guard r.length > 0 else { return r }
        let ns = text.string as NSString
        let skip = CharacterSet.whitespacesAndNewlines
        while r.length > 0,
              let scalar = Unicode.Scalar(ns.character(at: r.location)),
              skip.contains(scalar) {
            r.location += 1
            r.length -= 1
        }
        while r.length > 0,
              let scalar = Unicode.Scalar(ns.character(at: NSMaxRange(r) - 1)),
              skip.contains(scalar) {
            r.length -= 1
        }
        return r
    }

    private static func isCodeRun(_ attrs: [NSAttributedString.Key: Any]) -> Bool {
        attrs[.markdownInlineCode] as? Bool == true || attrs[.markdownCodeBlock] as? Bool == true
    }

    private static func headingLevel(of attrs: [NSAttributedString.Key: Any]) -> Int {
        (attrs[.markdownHeadingLevel] as? Int) ?? 0
    }

    /// Attribute runs inside `range` that may carry inline marks.
    private static func enumerateMarkRuns(
        _ text: NSAttributedString,
        _ range: NSRange,
        skipHeadings: Bool,
        _ body: (NSRange, [NSAttributedString.Key: Any]) -> Void
    ) {
        let r = clamped(range, in: text)
        guard r.length > 0 else { return }
        text.enumerateAttributes(in: r, options: []) { attrs, runRange, _ in
            if isCodeRun(attrs) { return }
            if skipHeadings, headingLevel(of: attrs) > 0 { return }
            body(runRange, attrs)
        }
    }

    // MARK: - Queries

    /// True when EVERY eligible run in `range` already carries the mark — the
    /// tri-state tiptap uses to decide add-vs-remove.
    public static func isBold(_ text: NSAttributedString, range: NSRange) -> Bool {
        var sawRun = false
        var all = true
        enumerateMarkRuns(text, range, skipHeadings: true) { _, attrs in
            sawRun = true
            if !expFontHasBold(attrs[.font] as? PlatformFont) { all = false }
        }
        return sawRun && all
    }

    public static func isItalic(_ text: NSAttributedString, range: NSRange) -> Bool {
        var sawRun = false
        var all = true
        enumerateMarkRuns(text, range, skipHeadings: false) { _, attrs in
            sawRun = true
            if !expFontHasItalic(attrs[.font] as? PlatformFont) { all = false }
        }
        return sawRun && all
    }

    public static func isStrikethrough(_ text: NSAttributedString, range: NSRange) -> Bool {
        var sawRun = false
        var all = true
        enumerateMarkRuns(text, range, skipHeadings: false) { _, attrs in
            sawRun = true
            if attrs[.markdownStrikethrough] as? Bool != true { all = false }
        }
        return sawRun && all
    }

    /// Heading level of the paragraph containing `location` (0 = body).
    public static func headingLevel(_ text: NSAttributedString, at location: Int) -> Int {
        guard let attrs = text.attributesIfInBounds(at: location) else { return 0 }
        return headingLevel(of: attrs)
    }

    /// The href at (or immediately before) `range`, for prefilling the URL
    /// field.
    public static func linkURL(_ text: NSAttributedString, at range: NSRange) -> String? {
        let r = clamped(range, in: text)
        let probe = r.length > 0 ? r.location : max(0, r.location - 1)
        guard let attrs = text.attributesIfInBounds(at: probe) else { return nil }
        if let url = attrs[.link] as? URL { return url.absoluteString }
        return attrs[.link] as? String
    }

    /// The contiguous `.link` run covering `location`, so a collapsed caret
    /// inside a link retargets the WHOLE link instead of splitting it.
    public static func linkRange(_ text: NSAttributedString, at location: Int) -> NSRange? {
        let probe = min(max(0, location), max(0, text.length - 1))
        guard text.length > 0, text.attributesIfInBounds(at: probe)?[.link] != nil else { return nil }
        var effective = NSRange(location: 0, length: 0)
        _ = text.attribute(
            .link,
            at: probe,
            longestEffectiveRange: &effective,
            in: NSRange(location: 0, length: text.length)
        )
        return effective.length > 0 ? effective : nil
    }

    public static func hasLink(_ text: NSAttributedString, range: NSRange) -> Bool {
        let r = clamped(range, in: text)
        if r.length == 0 { return linkURL(text, at: r) != nil }
        var found = false
        text.enumerateAttribute(.link, in: r, options: []) { value, _, stop in
            if value != nil {
                found = true
                stop.pointee = true
            }
        }
        return found
    }

    // MARK: - Inline marks (selection)

    @discardableResult
    public static func toggleBold(in text: NSMutableAttributedString, range: NSRange) -> Bool {
        let add = !isBold(text, range: range)
        setBold(add, in: text, range: range)
        return add
    }

    public static func setBold(_ add: Bool, in text: NSMutableAttributedString, range: NSRange) {
        let target = add ? trimmedForMarks(range, in: text) : clamped(range, in: text)
        var edits: [(NSRange, PlatformFont)] = []
        enumerateMarkRuns(text, target, skipHeadings: true) { runRange, attrs in
            let font = (attrs[.font] as? PlatformFont) ?? MarkdownStyle.bodyFont
            edits.append((runRange, add ? expBoldFont(font) : fontRemovingBold(font)))
        }
        for (runRange, font) in edits { text.addAttribute(.font, value: font, range: runRange) }
    }

    @discardableResult
    public static func toggleItalic(in text: NSMutableAttributedString, range: NSRange) -> Bool {
        let add = !isItalic(text, range: range)
        setItalic(add, in: text, range: range)
        return add
    }

    public static func setItalic(_ add: Bool, in text: NSMutableAttributedString, range: NSRange) {
        let target = add ? trimmedForMarks(range, in: text) : clamped(range, in: text)
        var edits: [(NSRange, PlatformFont)] = []
        enumerateMarkRuns(text, target, skipHeadings: false) { runRange, attrs in
            let font = (attrs[.font] as? PlatformFont) ?? MarkdownStyle.bodyFont
            edits.append((runRange, add ? expItalicFont(font) : fontRemovingItalic(font)))
        }
        for (runRange, font) in edits { text.addAttribute(.font, value: font, range: runRange) }
    }

    @discardableResult
    public static func toggleStrikethrough(in text: NSMutableAttributedString, range: NSRange) -> Bool {
        let add = !isStrikethrough(text, range: range)
        setStrikethrough(add, in: text, range: range)
        return add
    }

    public static func setStrikethrough(_ add: Bool, in text: NSMutableAttributedString, range: NSRange) {
        let target = add ? trimmedForMarks(range, in: text) : clamped(range, in: text)
        var runs: [NSRange] = []
        enumerateMarkRuns(text, target, skipHeadings: false) { runRange, _ in runs.append(runRange) }
        for runRange in runs {
            if add {
                text.addAttributes([
                    .strikethroughStyle: NSUnderlineStyle.single.rawValue,
                    .markdownStrikethrough: true,
                ], range: runRange)
            } else {
                text.removeAttribute(.strikethroughStyle, range: runRange)
                text.removeAttribute(.markdownStrikethrough, range: runRange)
            }
        }
    }

    // MARK: - Inline marks (collapsed caret / typing attributes)

    public static func togglingBold(_ attrs: [NSAttributedString.Key: Any]) -> [NSAttributedString.Key: Any] {
        guard !isCodeRun(attrs), headingLevel(of: attrs) == 0 else { return attrs }
        var out = attrs
        let font = (attrs[.font] as? PlatformFont) ?? MarkdownStyle.bodyFont
        out[.font] = expFontHasBold(font) ? fontRemovingBold(font) : expBoldFont(font)
        return out
    }

    public static func togglingItalic(_ attrs: [NSAttributedString.Key: Any]) -> [NSAttributedString.Key: Any] {
        guard !isCodeRun(attrs) else { return attrs }
        var out = attrs
        let font = (attrs[.font] as? PlatformFont) ?? MarkdownStyle.bodyFont
        out[.font] = expFontHasItalic(font) ? fontRemovingItalic(font) : expItalicFont(font)
        return out
    }

    public static func togglingStrikethrough(_ attrs: [NSAttributedString.Key: Any]) -> [NSAttributedString.Key: Any] {
        guard !isCodeRun(attrs) else { return attrs }
        var out = attrs
        if attrs[.markdownStrikethrough] as? Bool == true {
            out[.strikethroughStyle] = nil
            out[.markdownStrikethrough] = nil
        } else {
            out[.strikethroughStyle] = NSUnderlineStyle.single.rawValue
            out[.markdownStrikethrough] = true
        }
        return out
    }

    // MARK: - Headings / line format

    /// Applies `level` (1...3 here, 0 = body paragraph) to the paragraph(s) the
    /// range covers, stripping every other line format on the way — including
    /// the baked list prefix, which is TEXT and would otherwise be serialized
    /// as literal heading content.
    public static func setHeading(level: Int, in text: NSMutableAttributedString, paragraphRange: NSRange) {
        var range = removingBakedListPrefix(in: text, paragraphRange: clamped(paragraphRange, in: text))
        range = clamped(range, in: text)
        guard range.length > 0 else { return }

        for key in lineFormatKeys { text.removeAttribute(key, range: range) }
        text.removeAttribute(.backgroundColor, range: range)

        var attrs: [NSAttributedString.Key: Any] = [
            .paragraphStyle: bodyParagraphStyle,
        ]
        if level > 0 {
            attrs[.markdownHeadingLevel] = level
            // The heading font replaces whatever was there wholesale (the
            // inverse direction restores the body font the same way).
            attrs[.font] = MarkdownStyle.headingFont(level: level)
        } else {
            attrs[.font] = MarkdownStyle.bodyFont
        }
        text.addAttributes(attrs, range: range)
        resetForegroundColor(in: text, range: range)
    }

    /// `setHeading(level: 0, …)` under the name the callers actually mean.
    public static func clearLineFormatting(in text: NSMutableAttributedString, paragraphRange: NSRange) {
        setHeading(level: 0, in: text, paragraphRange: paragraphRange)
    }

    public static func headingTypingAttributes(
        level: Int,
        from attrs: [NSAttributedString.Key: Any]
    ) -> [NSAttributedString.Key: Any] {
        var out = attrs
        for key in lineFormatKeys { out[key] = nil }
        out[.backgroundColor] = nil
        out[.foregroundColor] = MarkdownStyle.textColor
        out[.paragraphStyle] = bodyParagraphStyle
        out[.font] = level > 0 ? MarkdownStyle.headingFont(level: level) : MarkdownStyle.bodyFont
        if level > 0 { out[.markdownHeadingLevel] = level }
        return out
    }

    // MARK: - Clear formatting

    /// Strips inline marks only — the run's line format (heading font, quote
    /// color) survives.
    public static func clearInlineFormatting(in text: NSMutableAttributedString, range: NSRange) {
        let target = clamped(range, in: text)
        guard target.length > 0 else { return }
        var fontEdits: [(NSRange, PlatformFont)] = []
        var colorEdits: [(NSRange, PlatformColor)] = []
        text.enumerateAttributes(in: target, options: []) { attrs, runRange, _ in
            let level = headingLevel(of: attrs)
            let font = (attrs[.font] as? PlatformFont) ?? MarkdownStyle.bodyFont
            if level > 0 {
                fontEdits.append((runRange, MarkdownStyle.headingFont(level: level)))
            } else if attrs[.markdownInlineCode] as? Bool == true {
                fontEdits.append((runRange, MarkdownStyle.bodyFont))
            } else if attrs[.markdownCodeBlock] as? Bool == true {
                // A fenced block keeps its monospace font; only the marks go.
                fontEdits.append((runRange, font))
            } else {
                fontEdits.append((runRange, fontRemovingItalic(fontRemovingBold(font))))
            }
            let isQuote = attrs[.markdownBlockquote] as? Bool == true
            colorEdits.append((runRange, isQuote ? MarkdownStyle.blockquoteTextColor : MarkdownStyle.textColor))
        }
        text.removeAttribute(.strikethroughStyle, range: target)
        text.removeAttribute(.markdownStrikethrough, range: target)
        text.removeAttribute(.markdownInlineCode, range: target)
        text.removeAttribute(.link, range: target)
        text.removeAttribute(.backgroundColor, range: target)
        for (runRange, font) in fontEdits { text.addAttribute(.font, value: font, range: runRange) }
        for (runRange, color) in colorEdits { text.addAttribute(.foregroundColor, value: color, range: runRange) }
    }

    /// Web's `unsetAllMarks() + clearNodes()`: every inline mark in the
    /// selection AND the line format of every paragraph it touches.
    public static func clearFormatting(in text: NSMutableAttributedString, range: NSRange) {
        let target = clamped(range, in: text)
        guard target.length > 0 else { return }
        clearInlineFormatting(in: text, range: target)
        // Back to front: clearing a list deletes its baked prefix, which would
        // shift every range after it.
        for paragraph in paragraphRanges(in: text, covering: target).reversed() {
            clearLineFormatting(in: text, paragraphRange: paragraph)
        }
    }

    /// The typing attributes a caret gets after "clear formatting" — a plain
    /// body paragraph, exactly like the list-exit path in the editor.
    public static func clearedTypingAttributes() -> [NSAttributedString.Key: Any] {
        MarkdownStyle.baseAttributes
    }

    // MARK: - Links

    /// Applies `url` over `range`; a collapsed caret INSERTS the url as its own
    /// link text. Returns the range the link ended up covering.
    @discardableResult
    public static func applyLink(_ url: String, in text: NSMutableAttributedString, range: NSRange) -> NSRange {
        let value: Any = URL(string: url) ?? url
        let target = clamped(range, in: text)
        guard target.length == 0 else {
            text.addAttributes([.link: value, .foregroundColor: MarkdownStyle.linkColor], range: target)
            return target
        }
        var attrs = text.attributesIfInBounds(at: max(0, target.location - 1)) ?? MarkdownStyle.baseAttributes
        // The caret may sit right after a code span or a chip; neither may ride
        // along into the inserted link text.
        attrs[.markdownInlineCode] = nil
        attrs[.markdownChip] = nil
        attrs[.attachment] = nil
        attrs[.backgroundColor] = nil
        attrs[.font] = MarkdownStyle.bodyFont
        attrs[.link] = value
        attrs[.foregroundColor] = MarkdownStyle.linkColor
        text.insert(NSAttributedString(string: url, attributes: attrs), at: target.location)
        return NSRange(location: target.location, length: (url as NSString).length)
    }

    public static func removeLink(in text: NSMutableAttributedString, range: NSRange) {
        let target = clamped(range, in: text)
        guard target.length > 0 else { return }
        text.removeAttribute(.link, range: target)
        resetForegroundColor(in: text, range: target)
    }

    // MARK: - Internals

    /// Repaints the base text color over runs that no longer have a reason to
    /// deviate (links keep theirs, quotes keep their muted color).
    private static func resetForegroundColor(in text: NSMutableAttributedString, range: NSRange) {
        let target = clamped(range, in: text)
        guard target.length > 0 else { return }
        var edits: [(NSRange, PlatformColor)] = []
        text.enumerateAttributes(in: target, options: []) { attrs, runRange, _ in
            if attrs[.link] != nil { return }
            if attrs[.markdownChip] != nil { return }
            let isQuote = attrs[.markdownBlockquote] as? Bool == true
            edits.append((runRange, isQuote ? MarkdownStyle.blockquoteTextColor : MarkdownStyle.textColor))
        }
        for (runRange, color) in edits { text.addAttribute(.foregroundColor, value: color, range: runRange) }
    }

    /// Deletes the `• ` / `☐ ` / `1. ` prefix the list path baked into the
    /// paragraph's TEXT, returning the paragraph range that survives.
    private static func removingBakedListPrefix(
        in text: NSMutableAttributedString,
        paragraphRange: NSRange
    ) -> NSRange {
        guard paragraphRange.length > 0,
              let attrs = text.attributesIfInBounds(at: paragraphRange.location),
              attrs[.markdownListType] != nil else { return paragraphRange }
        let paragraph = (text.string as NSString).substring(with: paragraphRange)
        let prefixLength = bakedListPrefixLength(of: paragraph)
        guard prefixLength > 0 else { return paragraphRange }
        text.replaceCharacters(
            in: NSRange(location: paragraphRange.location, length: prefixLength),
            with: ""
        )
        return NSRange(location: paragraphRange.location, length: paragraphRange.length - prefixLength)
    }

    /// UTF-16 length of a baked list prefix at the start of `string`: `• `,
    /// `☐ `/`☑ `, or `<digits>. `, each with one optional trailing space.
    /// Mirrors the serializer's `bakedListPrefixUTF16Length`.
    private static func bakedListPrefixLength(of string: String) -> Int {
        let scalars = string.unicodeScalars
        var index = scalars.startIndex
        guard index < scalars.endIndex else { return 0 }
        var length: Int
        let first = scalars[index].value
        if first == 0x2022 || first == 0x2610 || first == 0x2611 { // • ☐ ☑
            length = 1
            index = scalars.index(after: index)
        } else {
            var digits = 0
            while index < scalars.endIndex, (0x30...0x39).contains(scalars[index].value) {
                digits += 1
                index = scalars.index(after: index)
            }
            guard digits > 0, index < scalars.endIndex, scalars[index] == "." else { return 0 }
            index = scalars.index(after: index)
            length = digits + 1
        }
        if index < scalars.endIndex, scalars[index] == " " { length += 1 }
        return length
    }

    /// Every paragraph range the (possibly multi-paragraph) `range` touches.
    private static func paragraphRanges(in text: NSAttributedString, covering range: NSRange) -> [NSRange] {
        let ns = text.string as NSString
        var out: [NSRange] = []
        var location = range.location
        while true {
            let paragraph = ns.safeParagraphRange(at: location)
            if paragraph.length == 0 { break }
            out.append(paragraph)
            let next = NSMaxRange(paragraph)
            if next <= location || next >= NSMaxRange(range) { break }
            location = next
        }
        return out
    }

    private static func fontRemovingBold(_ font: PlatformFont) -> PlatformFont {
        fontRemoving(.traitBold, from: font)
    }

    private static func fontRemovingItalic(_ font: PlatformFont) -> PlatformFont {
        fontRemoving(.traitItalic, from: font)
    }

    private static func fontRemoving(
        _ trait: UIFontDescriptor.SymbolicTraits,
        from font: PlatformFont
    ) -> PlatformFont {
        let descriptor = font.fontDescriptor
        var traits = descriptor.symbolicTraits
        guard traits.contains(trait) else { return font }
        traits.remove(trait)
        // `withSymbolicTraits` returns nil for descriptors it cannot resolve;
        // rebuilding a plain system font of the same size is the safe fallback
        // (the mark is gone either way, which is what the user asked for).
        guard let newDescriptor = descriptor.withSymbolicTraits(traits) else {
            return PlatformFont.systemFont(ofSize: font.pointSize)
        }
        return PlatformFont(descriptor: newDescriptor, size: font.pointSize)
    }
}
