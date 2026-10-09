import CoreText
import SwiftUI
import ExponentialUICore

#if canImport(UIKit)
import UIKit
public typealias PlatformFont = UIFont
#elseif canImport(AppKit)
import AppKit
public typealias PlatformFont = NSFont
#endif

/// Fonts named by a theme (`tokens.type.family.sans` = `Inter`) are
/// REGISTERED by the host; the painter resolves family names to platform
/// fonts here and falls back to the system font when a family is missing.
public enum ExponentialUIFonts {
    /// Register a font file (`.ttf`/`.otf`) with the process. Returns false
    /// when CoreText refused it (already registered counts as success).
    @discardableResult
    public static func register(url: URL) -> Bool {
        var error: Unmanaged<CFError>?
        if CTFontManagerRegisterFontsForURL(url as CFURL, .process, &error) { return true }
        if let e = error?.takeRetainedValue(), CFErrorGetCode(e) == CTFontManagerError.alreadyRegistered.rawValue { return true }
        return false
    }

    nonisolated(unsafe) private static var families: Set<String> = []
    private static let lock = NSLock()

    /// Is `family` available to the process?
    public static func isAvailable(_ family: String) -> Bool {
        lock.lock(); defer { lock.unlock() }
        if families.contains(family) { return true }
        #if canImport(UIKit)
        let ok = UIFont.familyNames.contains(family)
        #else
        let ok = NSFontManager.shared.availableFontFamilies.contains(family)
        #endif
        if ok { families.insert(family) }
        return ok
    }

    static func platformWeight(_ w: Int) -> PlatformFont.Weight {
        switch w {
        case ..<200: .ultraLight
        case ..<300: .thin
        case ..<400: .light
        case ..<500: .regular
        case ..<600: .medium
        case ..<700: .semibold
        case ..<800: .bold
        case ..<900: .heavy
        default: .black
        }
    }

    static func swiftUIWeight(_ w: Int) -> Font.Weight {
        switch w {
        case ..<200: .ultraLight
        case ..<300: .thin
        case ..<400: .light
        case ..<500: .regular
        case ..<600: .medium
        case ..<700: .semibold
        case ..<800: .bold
        case ..<900: .heavy
        default: .black
        }
    }

    /// The platform font for a family NAME (nil/missing = system), a CSS
    /// weight and a size: the SAME face the measurer shapes with (CSS font
    /// matching, `TextFonts`), so painted text matches measured text.
    public static func font(family: String?, weight: Int, size: CGFloat, italic: Bool = false) -> PlatformFont {
        TextFonts.platformFont(family: family, weight: weight, size: size, italic: italic)
    }

    /// The system font at a CSS weight (`TextFonts`' fallback).
    static func systemFont(weight: Int, size: CGFloat, italic: Bool) -> PlatformFont {
        let font = PlatformFont.systemFont(ofSize: size, weight: platformWeight(weight))
        return italic ? italicized(font, size: size) : font
    }

    /// The system monospaced font at a CSS weight (`ui-monospace`), italic
    /// by the trait (a CodeBlock comment token is `fontStyle: italic`).
    static func monospacedFont(weight: Int, size: CGFloat, italic: Bool) -> PlatformFont {
        let font = PlatformFont.monospacedSystemFont(ofSize: size, weight: platformWeight(weight))
        return italic ? italicized(font, size: size) : font
    }

    /// `font` with the italic trait (unchanged when the family has none).
    static func italicized(_ font: PlatformFont, size: CGFloat) -> PlatformFont {
        #if canImport(UIKit)
        return font.fontDescriptor.withSymbolicTraits(font.fontDescriptor.symbolicTraits.union(.traitItalic)).map { UIFont(descriptor: $0, size: size) } ?? font
        #else
        return NSFontManager.shared.convert(font, toHaveTrait: .italicFontMask)
        #endif
    }
}

/// A resolved text style (the core's `TextStyle`): size, weight, line
/// height, an optional family NAME and (round 2) letter spacing (pt, the
/// font scale applied), `textTransform` and an italic `fontStyle`.
public struct TextStyle: Equatable, Hashable, Sendable {
    public var fontSize: CGFloat
    public var fontWeight: Int
    public var lineHeight: CGFloat
    public var fontFamily: String?
    /// Extra advance after every character (CSS `letter-spacing`); nil = none.
    public var letterSpacing: CGFloat?
    /// `uppercase | lowercase | capitalize`; nil = none.
    public var textTransform: String?
    /// `fontStyle: italic`.
    public var italic: Bool

    public init(fontSize: CGFloat, fontWeight: Int, lineHeight: CGFloat, fontFamily: String?, letterSpacing: CGFloat? = nil, textTransform: String? = nil, italic: Bool = false) {
        self.fontSize = fontSize
        self.fontWeight = fontWeight
        self.lineHeight = lineHeight
        self.fontFamily = fontFamily
        self.letterSpacing = letterSpacing
        self.textTransform = textTransform
        self.italic = italic
    }

    /// `defaultFamily` = the theme's sans family: a part whose recipe names
    /// no family inherits the theme's (CSS inheritance), never the system font.
    init(_ f: FfiTextStyle, defaultFamily: String? = nil) {
        fontSize = CGFloat(f.fontSize)
        fontWeight = Int(f.fontWeight)
        lineHeight = CGFloat(f.lineHeight)
        fontFamily = f.fontFamily ?? defaultFamily
        letterSpacing = f.letterSpacing.flatMap { $0 == 0 ? nil : CGFloat($0) }
        textTransform = f.textTransform.flatMap { $0 == "none" || $0.isEmpty ? nil : $0 }
        italic = f.fontStyle == "italic"
    }

    /// `text` as this style shows it (`textTransform`, CSS semantics).
    public func shown(_ text: String) -> String {
        switch textTransform {
        case "uppercase": text.uppercased()
        case "lowercase": text.lowercased()
        case "capitalize": text.split(separator: " ", omittingEmptySubsequences: false).map { w in w.prefix(1).uppercased() + w.dropFirst() }.joined(separator: " ")
        default: text
        }
    }

    public static let body = TextStyle(fontSize: 14, fontWeight: 400, lineHeight: 20, fontFamily: nil)
}
