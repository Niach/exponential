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
        guard italic else { return font }
        #if canImport(UIKit)
        return font.fontDescriptor.withSymbolicTraits([.traitItalic]).map { UIFont(descriptor: $0, size: size) } ?? font
        #else
        return NSFontManager.shared.convert(font, toHaveTrait: .italicFontMask)
        #endif
    }
}

/// A resolved text style (the core's `TextStyle`): size, weight, line
/// height and an optional family NAME.
public struct TextStyle: Equatable, Hashable, Sendable {
    public var fontSize: CGFloat
    public var fontWeight: Int
    public var lineHeight: CGFloat
    public var fontFamily: String?

    public init(fontSize: CGFloat, fontWeight: Int, lineHeight: CGFloat, fontFamily: String?) {
        self.fontSize = fontSize
        self.fontWeight = fontWeight
        self.lineHeight = lineHeight
        self.fontFamily = fontFamily
    }

    init(_ f: FfiTextStyle) {
        fontSize = CGFloat(f.fontSize)
        fontWeight = Int(f.fontWeight)
        lineHeight = CGFloat(f.lineHeight)
        fontFamily = f.fontFamily
    }

    public static let body = TextStyle(fontSize: 14, fontWeight: 400, lineHeight: 20, fontFamily: nil)
}
