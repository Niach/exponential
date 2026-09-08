import Foundation
import SwiftUI
import UIKit

extension NSAttributedString.Key {
    public static let markdownHeadingLevel = NSAttributedString.Key("exp.markdownHeadingLevel")
    public static let markdownListType = NSAttributedString.Key("exp.markdownListType")
    public static let markdownListItemIndex = NSAttributedString.Key("exp.markdownListItemIndex")
    public static let markdownListDepth = NSAttributedString.Key("exp.markdownListDepth")
    public static let markdownCodeBlock = NSAttributedString.Key("exp.markdownCodeBlock")
    public static let markdownCodeBlockLang = NSAttributedString.Key("exp.markdownCodeBlockLang")
    public static let markdownBlockquote = NSAttributedString.Key("exp.markdownBlockquote")
    public static let markdownInlineCode = NSAttributedString.Key("exp.markdownInlineCode")
    public static let markdownImageURL = NSAttributedString.Key("exp.markdownImageURL")
    public static let markdownImageAlt = NSAttributedString.Key("exp.markdownImageAlt")
    public static let markdownStrikethrough = NSAttributedString.Key("exp.markdownStrikethrough")
}

public enum MarkdownStyle {
    /// EXP-698: per-RENDER palette deviations. The defaults (`nil`) are the
    /// interchange look every editable surface keeps; the chat feeds (the
    /// steering narration, bubbles and cards) tint their inline code with the
    /// shared `Semantic.code*` tokens so a `path/like/this` reads as code the
    /// way it does on web. Nothing here reaches serialization — the tint is
    /// display only.
    public struct Overrides: Equatable, Hashable, Sendable {
        public var inlineCodeForeground: Color?
        public var inlineCodeBackground: Color?
        /// EXP-787: the point size prose renders at. The agent transcript
        /// reads at its own measure (`DesignTokens.Transcript.bodySize`) —
        /// every editable surface keeps the interchange body font, so `nil`
        /// resolves back to `MarkdownStyle.bodyFont`.
        public var bodySize: CGFloat?
        /// EXP-787: the LINE BOX the render targets — `nil` keeps the
        /// editors' 4pt leading. Stored as the token (22), never as leading:
        /// both it and the font scale with Dynamic Type, so the leading
        /// between them can only be computed at resolve time
        /// (`resolvedLineSpacing`).
        public var lineHeight: CGFloat?

        public init(
            inlineCodeForeground: Color? = nil,
            inlineCodeBackground: Color? = nil,
            bodySize: CGFloat? = nil,
            lineHeight: CGFloat? = nil
        ) {
            self.inlineCodeForeground = inlineCodeForeground
            self.inlineCodeBackground = inlineCodeBackground
            self.bodySize = bodySize
            self.lineHeight = lineHeight
        }

        /// The leading this render needs RIGHT NOW, at the reader's current
        /// text size. `nil` = no deviation (the editors' 4pt).
        public var resolvedLineSpacing: CGFloat? {
            MarkdownStyle.resolvedLineSpacing(bodySize: bodySize, lineHeight: lineHeight)
        }

        /// No deviation — the contract look.
        public static let none = Overrides()

        /// Key material for the render caches that key on the parse inputs
        /// (`AgentMarkdownText`). Describes BOTH colours in full: a hash would
        /// collide eventually, and a collision there hands a cached render
        /// back under the wrong tint — a silently wrong colour is exactly the
        /// bug a cache key must not be able to produce.
        public var cacheKey: String {
            func describe(_ color: Color?) -> String {
                guard let color else { return "-" }
                return String(describing: color.resolve(in: EnvironmentValues()))
            }
            // The metrics go in RESOLVED (EXP-787): the tokens are constants,
            // but what they render as moves with Dynamic Type — keying on the
            // tokens would hand back a render baked at the previous text size.
            return "fg:\(describe(inlineCodeForeground))|bg:\(describe(inlineCodeBackground))"
                + "|size:\(bodySize.map { "\(resolvedBodyFont($0).pointSize)" } ?? "-")"
                + "|lead:\(resolvedLineSpacing.map { "\($0)" } ?? "-")"
        }
    }

    // `nonisolated(unsafe)`: these are immutable font/color constants that
    // never mutate, so opting out of strict-concurrency checking is safe.
    public nonisolated(unsafe) static let bodyFont = PlatformFont.preferredFont(forTextStyle: .body)
    public nonisolated(unsafe) static let textColor = PlatformColor.white.withAlphaComponent(0.9)
    public nonisolated(unsafe) static let linkColor = PlatformColor(red: 0.42, green: 0.64, blue: 1.0, alpha: 1.0)
    public nonisolated(unsafe) static let codeBackground = PlatformColor.white.withAlphaComponent(0.08)
    public nonisolated(unsafe) static let codeBlockBackground = PlatformColor.white.withAlphaComponent(0.06)
    /// Chip capsule fill + hairline border (EXP-423, Linear parity): a bordered
    /// rounded rect, not the flat highlight the pill used to borrow from
    /// `codeBackground`. Painted by `MarkdownLayoutManager`, never as a
    /// `.backgroundColor` run.
    public nonisolated(unsafe) static let chipBackground = PlatformColor.white.withAlphaComponent(0.08)
    public nonisolated(unsafe) static let chipBorder = PlatformColor.white.withAlphaComponent(0.16)
    /// The muted `#IDENT` identifier inside an issue chip (Linear look —
    /// Android's `MdStyle.ChipToken`, web's `--muted-foreground`). Mention
    /// pills keep `linkColor` via `expChipAttributes`.
    public nonisolated(unsafe) static let chipTokenColor = PlatformColor.white.withAlphaComponent(0.55)
    /// Issue chips are rounded RECTS (web 6px / desktop 4 / Android 5dp);
    /// mention pills stay fully round on every platform.
    public static let chipCornerRadius: CGFloat = 5
    /// Extra advance kerned onto a status chip's hidden `#` so the painted
    /// glyph clears the identifier next to it (EXP-655).
    public static let chipStatusIconGap: CGFloat = 6
    /// EXP-726 — GFM table chrome. The hairline matches `chipBorder` and the
    /// header tint `codeBlockBackground`, so a table reads as the same family
    /// of surfaces as a fence or a chip (web `--border` / `--foreground 4%`,
    /// Android `MdStyle.TableBorder`/`TableHeaderBg`).
    public nonisolated(unsafe) static let tableBorder = PlatformColor.white.withAlphaComponent(0.16)
    public nonisolated(unsafe) static let tableHeaderBackground = PlatformColor.white.withAlphaComponent(0.06)
    /// Cell width bounds. A cell hugs its content between these — narrower
    /// would make one-character columns unreadable, wider would push the row
    /// off a phone before the horizontal scroller earns its keep.
    public static let tableCellMinWidth: CGFloat = 56
    public static let tableCellMaxWidth: CGFloat = 280
    public nonisolated(unsafe) static let blockquoteTextColor = PlatformColor.white.withAlphaComponent(0.6)
    public nonisolated(unsafe) static let blockquoteBarColor = PlatformColor.white.withAlphaComponent(0.25)
    public nonisolated(unsafe) static let placeholderColor = PlatformColor.white.withAlphaComponent(0.3)

    /// The body font one render uses (EXP-787): the interchange default, or
    /// the point size a display-only render asked for — SCALED by the reader's
    /// Dynamic Type setting, exactly like `bodyFont` (a `preferredFont`) is.
    /// The token is the default-size value, never a fixed point.
    public static func resolvedBodyFont(_ bodySize: CGFloat?) -> PlatformFont {
        guard let bodySize else { return bodyFont }
        #if canImport(UIKit)
        return UIFontMetrics(forTextStyle: .body)
            .scaledFont(for: PlatformFont.systemFont(ofSize: bodySize))
        #else
        return PlatformFont.systemFont(ofSize: bodySize)
        #endif
    }

    /// EXP-787: `lineSpacing` is LEADING, but the tokens describe a LINE BOX —
    /// and both the box and the font grow with Dynamic Type, at rates that are
    /// not identical. So the leading is the difference of the two SCALED
    /// values, resolved per render, and never precomputed. `nil` line height =
    /// no deviation from the editors' 4pt.
    public static func resolvedLineSpacing(bodySize: CGFloat?, lineHeight: CGFloat?) -> CGFloat? {
        guard let lineHeight else { return nil }
        #if canImport(UIKit)
        let box = UIFontMetrics(forTextStyle: .body).scaledValue(for: lineHeight)
        #else
        let box = lineHeight
        #endif
        return max(0, box - resolvedBodyFont(bodySize).lineHeight)
    }

    /// The body style at the DEFAULT content size (17pt). The denominator for
    /// display-only scales: `bodyFont.pointSize` moves with Dynamic Type, so a
    /// ratio taken against it would silently change with the reader's setting.
    private static var defaultBodyPointSize: CGFloat {
        #if canImport(UIKit)
        return PlatformFont.preferredFont(
            forTextStyle: .body,
            compatibleWith: UITraitCollection(preferredContentSizeCategory: .large)
        ).pointSize
        #else
        return bodyFont.pointSize
        #endif
    }

    /// EXP-787: `bodySize` scales the whole ladder with the body, so a
    /// transcript heading stays proportional instead of towering over 14pt
    /// prose — and the result then goes through `UIFontMetrics` like the body
    /// does, so headings keep Dynamic Type too.
    public static func headingFont(level: Int, bodySize: CGFloat? = nil) -> PlatformFont {
        let sizes: [CGFloat] = [0, 24, 20, 18, 16, 15, 14]
        guard let bodySize else {
            let size = level >= 1 && level <= 6 ? sizes[level] : bodyFont.pointSize
            return PlatformFont.systemFont(ofSize: size, weight: .semibold)
        }
        let scale = bodySize / defaultBodyPointSize
        let size = level >= 1 && level <= 6 ? sizes[level] * scale : bodySize
        let font = PlatformFont.systemFont(ofSize: size, weight: .semibold)
        #if canImport(UIKit)
        return UIFontMetrics(forTextStyle: .body).scaledFont(for: font)
        #else
        return font
        #endif
    }

    public static func monospaceFont(bodySize: CGFloat? = nil) -> PlatformFont {
        PlatformFont.monospacedSystemFont(
            ofSize: resolvedBodyFont(bodySize).pointSize * 0.9, weight: .regular
        )
    }

    /// The paragraph style a plain run gets. `lineSpacing` nil is the editors'
    /// 4pt leading; the transcript passes what its line-height token needs.
    public static func paragraphStyle(lineSpacing: CGFloat? = nil) -> NSParagraphStyle {
        let style = NSMutableParagraphStyle()
        style.lineSpacing = lineSpacing ?? 4
        return style
    }

    /// Paragraph style for blockquote paragraphs (EXP-246, Linear-style): the
    /// head indents clear the gutter where MarkdownLayoutManager draws the
    /// vertical quote bar. Serialization is untouched — the serializer keys
    /// off `.markdownBlockquote` only.
    public static func blockquoteParagraphStyle(lineSpacing: CGFloat? = nil) -> NSParagraphStyle {
        let style = NSMutableParagraphStyle()
        style.lineSpacing = lineSpacing ?? 4
        style.headIndent = 14
        style.firstLineHeadIndent = 14
        return style
    }

    public static var baseAttributes: [NSAttributedString.Key: Any] {
        let paragraphStyle = NSMutableParagraphStyle()
        paragraphStyle.lineSpacing = 4
        return [
            .font: bodyFont,
            .foregroundColor: textColor,
            .paragraphStyle: paragraphStyle,
        ]
    }
}

// MARK: - Font-trait helpers

/// Returns `font` with the bold trait added.
public func expBoldFont(_ font: PlatformFont) -> PlatformFont {
    let descriptor = font.fontDescriptor
    var traits = descriptor.symbolicTraits
    traits.insert(.traitBold)
    guard let newDescriptor = descriptor.withSymbolicTraits(traits) else { return font }
    return PlatformFont(descriptor: newDescriptor, size: font.pointSize)
}

/// Returns `font` with the italic trait added.
public func expItalicFont(_ font: PlatformFont) -> PlatformFont {
    let descriptor = font.fontDescriptor
    var traits = descriptor.symbolicTraits
    traits.insert(.traitItalic)
    guard let newDescriptor = descriptor.withSymbolicTraits(traits) else { return font }
    return PlatformFont(descriptor: newDescriptor, size: font.pointSize)
}

public func expFontHasBold(_ font: PlatformFont?) -> Bool {
    guard let font else { return false }
    return font.fontDescriptor.symbolicTraits.contains(.traitBold)
}

public func expFontHasItalic(_ font: PlatformFont?) -> Bool {
    guard let font else { return false }
    return font.fontDescriptor.symbolicTraits.contains(.traitItalic)
}
