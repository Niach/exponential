import Foundation
import ExponentialUIPrimitives

#if canImport(UIKit)
import UIKit
#elseif canImport(AppKit)
import AppKit
#endif

/// Text shaping for the measurer AND the text leaves: one TextKit engine
/// answers taffy's questions and paints the result, so a measured height is
/// the painted height. Line height follows CSS (`n` lines = `n ×
/// lineHeight`): the paragraph style pins minimum and maximum line height.
public enum TextShaper {
    /// The attributes a text style renders with.
    public static func attributes(_ ts: TextStyle, italic: Bool = false, family: String? = nil, align: NSTextAlignment = .natural, lineBreak: NSLineBreakMode = .byWordWrapping) -> [NSAttributedString.Key: Any] {
        let font = ExponentialUIFonts.font(family: family ?? ts.fontFamily, weight: ts.fontWeight, size: ts.fontSize, italic: italic)
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
        return [.font: font, .paragraphStyle: p, .baselineOffset: 0, .init("ExponentialUI.halfLeading"): offset]
    }

    /// `text` as an attributed string in `ts`.
    public static func attributed(_ text: String, _ ts: TextStyle, align: NSTextAlignment = .natural, lineBreak: NSLineBreakMode = .byWordWrapping) -> NSAttributedString {
        NSAttributedString(string: text, attributes: attributes(ts, align: align, lineBreak: lineBreak))
    }

    /// The width of one line (no wrap), ceiled.
    public static func lineWidth(_ attributed: NSAttributedString) -> CGFloat {
        if attributed.length == 0 { return 0 }
        let rect = attributed.boundingRect(with: CGSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude), options: [.usesLineFragmentOrigin], context: nil)
        return ceil(rect.width)
    }

    /// Max-content width of `text` (the widest line of a multi-line text).
    public static func maxContent(_ text: String, _ ts: TextStyle) -> CGFloat {
        text.components(separatedBy: "\n").map { lineWidth(attributed($0, ts)) }.max() ?? 0
    }

    /// Min-content width: the widest word.
    public static func minContent(_ text: String, _ ts: TextStyle) -> CGFloat {
        text.split(whereSeparator: { $0.isWhitespace || $0 == "\n" }).map { lineWidth(attributed(String($0), ts)) }.max() ?? 0
    }

    /// The height of `attributed` wrapped at `wrap` (nil = no wrap), at most
    /// `clamp` lines, never less than one line.
    public static func wrappedHeight(_ attributed: NSAttributedString, lineHeight: CGFloat, wrap: CGFloat?, clamp: Int? = nil) -> CGFloat {
        if attributed.length == 0 { return lineHeight }
        let width: CGFloat = wrap.map { max($0, 1) } ?? CGFloat.greatestFiniteMagnitude
        let rect = attributed.boundingRect(with: CGSize(width: width, height: CGFloat.greatestFiniteMagnitude), options: [.usesLineFragmentOrigin], context: nil)
        var h = ceil(rect.height / lineHeight - 0.01) * lineHeight
        if let clamp, clamp > 0 { h = min(h, CGFloat(clamp) * lineHeight) }
        return max(h, lineHeight)
    }

    /// Text content `(w, h)` at an inner wrap width (nil = max-content,
    /// 0 = min-content), the gpui rule: a one-line text shrinks to 0 at
    /// min-content and never wraps.
    public static func measure(_ text: String, _ ts: TextStyle, wrap: CGFloat?, lines: Int?) -> CGSize {
        let single = lines == 1
        switch wrap {
        case .none:
            return CGSize(width: maxContent(text, ts), height: wrappedHeight(attributed(text, ts), lineHeight: ts.lineHeight, wrap: nil, clamp: lines))
        case .some(let w) where w <= 0:
            return CGSize(width: single ? 0 : minContent(text, ts), height: ts.lineHeight)
        case .some(let w):
            let maxW = maxContent(text, ts)
            let used = single ? min(maxW, w) : min(maxW, max(w, minContent(text, ts)))
            let h = single ? ts.lineHeight : wrappedHeight(attributed(text, ts), lineHeight: ts.lineHeight, wrap: max(used, 1), clamp: lines)
            return CGSize(width: used, height: h)
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
        let a = TextShaper.runs(inlines, spec, mono: mono, ink: .black, link: .black, codeBackground: nil)
        return a.string.components(separatedBy: "\n").enumerated().map { _, line in
            TextShaper.lineWidth(TextShaper.runs([MarkdownInline(text: line, code: inlines.first?.code ?? false)], spec, mono: mono, ink: .black, link: .black, codeBackground: nil))
        }.max() ?? 0
    }

    func widestWord(_ inlines: [MarkdownInline], _ spec: MarkdownTextSpec) -> CGFloat {
        let ts = TextStyle(fontSize: spec.size, fontWeight: spec.weight, lineHeight: spec.lineHeight, fontFamily: spec.family)
        return TextShaper.minContent(Markdown.plain(inlines), ts)
    }
}
