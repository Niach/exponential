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

    nonisolated(unsafe) private static var cache: [String: PlatformFont] = [:]
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
    /// weight and a size. Memoized.
    public static func font(family: String?, weight: Int, size: CGFloat, italic: Bool = false) -> PlatformFont {
        let key = "\(family ?? "")|\(weight)|\(size)|\(italic)"
        lock.lock()
        if let hit = cache[key] {
            lock.unlock()
            return hit
        }
        lock.unlock()
        var font: PlatformFont?
        if let family, !family.isEmpty, family != "system-ui", family != "ui-sans-serif", family != "ui-monospace", isAvailable(family) {
            #if canImport(UIKit)
            var traits: [UIFontDescriptor.TraitKey: Any] = [.weight: platformWeight(weight)]
            if italic { traits[.symbolic] = UIFontDescriptor.SymbolicTraits.traitItalic.rawValue }
            let d = UIFontDescriptor(fontAttributes: [.family: family, .traits: traits])
            font = UIFont(descriptor: d, size: size)
            #else
            var traits: [NSFontDescriptor.TraitKey: Any] = [.weight: platformWeight(weight)]
            if italic { traits[.symbolic] = NSFontDescriptor.SymbolicTraits.italic.rawValue }
            let d = NSFontDescriptor(fontAttributes: [.family: family, .traits: traits])
            font = NSFont(descriptor: d, size: size)
            #endif
        }
        if font == nil, family == "ui-monospace" || family == "monospace" {
            font = PlatformFont.monospacedSystemFont(ofSize: size, weight: platformWeight(weight))
        }
        var resolved = font ?? PlatformFont.systemFont(ofSize: size, weight: platformWeight(weight))
        if italic, font == nil {
            #if canImport(UIKit)
            if let d = resolved.fontDescriptor.withSymbolicTraits([.traitItalic]) { resolved = UIFont(descriptor: d, size: size) }
            #else
            resolved = NSFontManager.shared.convert(resolved, toHaveTrait: .italicFontMask)
            #endif
        }
        lock.lock()
        cache[key] = resolved
        lock.unlock()
        return resolved
    }

    static func clearCache() {
        lock.lock()
        cache.removeAll()
        families.removeAll()
        lock.unlock()
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
