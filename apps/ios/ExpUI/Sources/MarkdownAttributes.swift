import Foundation
#if os(iOS)
import UIKit
#elseif os(macOS)
import AppKit
#endif

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
    // `nonisolated(unsafe)`: these are immutable font/color constants. On macOS
    // NSFont/NSColor are not `Sendable`, so a plain `static let` trips strict
    // concurrency; the values never mutate, so opting out is safe.
    public nonisolated(unsafe) static let bodyFont = PlatformFont.preferredFont(forTextStyle: .body)
    public nonisolated(unsafe) static let textColor = PlatformColor.white.withAlphaComponent(0.9)
    public nonisolated(unsafe) static let linkColor = PlatformColor(red: 0.42, green: 0.64, blue: 1.0, alpha: 1.0)
    public nonisolated(unsafe) static let codeBackground = PlatformColor.white.withAlphaComponent(0.08)
    public nonisolated(unsafe) static let codeBlockBackground = PlatformColor.white.withAlphaComponent(0.06)
    public nonisolated(unsafe) static let blockquoteTextColor = PlatformColor.white.withAlphaComponent(0.6)
    public nonisolated(unsafe) static let placeholderColor = PlatformColor.white.withAlphaComponent(0.3)

    public static func headingFont(level: Int) -> PlatformFont {
        let sizes: [CGFloat] = [0, 24, 20, 18, 16, 15, 14]
        let size = level >= 1 && level <= 6 ? sizes[level] : bodyFont.pointSize
        return PlatformFont.systemFont(ofSize: size, weight: .semibold)
    }

    public static var monospaceFont: PlatformFont {
        PlatformFont.monospacedSystemFont(ofSize: bodyFont.pointSize * 0.9, weight: .regular)
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

// MARK: - Cross-platform font-trait helpers

/// Returns `font` with the bold trait added (UIKit `.traitBold` / AppKit `.bold`).
public func expBoldFont(_ font: PlatformFont) -> PlatformFont {
    let descriptor = font.fontDescriptor
    #if os(macOS)
    let newDescriptor = descriptor.withSymbolicTraits(descriptor.symbolicTraits.union(.bold))
    return PlatformFont(descriptor: newDescriptor, size: font.pointSize) ?? font
    #else
    var traits = descriptor.symbolicTraits
    traits.insert(.traitBold)
    guard let newDescriptor = descriptor.withSymbolicTraits(traits) else { return font }
    return PlatformFont(descriptor: newDescriptor, size: font.pointSize)
    #endif
}

/// Returns `font` with the italic trait added.
public func expItalicFont(_ font: PlatformFont) -> PlatformFont {
    let descriptor = font.fontDescriptor
    #if os(macOS)
    let newDescriptor = descriptor.withSymbolicTraits(descriptor.symbolicTraits.union(.italic))
    return PlatformFont(descriptor: newDescriptor, size: font.pointSize) ?? font
    #else
    var traits = descriptor.symbolicTraits
    traits.insert(.traitItalic)
    guard let newDescriptor = descriptor.withSymbolicTraits(traits) else { return font }
    return PlatformFont(descriptor: newDescriptor, size: font.pointSize)
    #endif
}

public func expFontHasBold(_ font: PlatformFont?) -> Bool {
    guard let font else { return false }
    #if os(macOS)
    return font.fontDescriptor.symbolicTraits.contains(.bold)
    #else
    return font.fontDescriptor.symbolicTraits.contains(.traitBold)
    #endif
}

public func expFontHasItalic(_ font: PlatformFont?) -> Bool {
    guard let font else { return false }
    #if os(macOS)
    return font.fontDescriptor.symbolicTraits.contains(.italic)
    #else
    return font.fontDescriptor.symbolicTraits.contains(.traitItalic)
    #endif
}
