import CoreText
import SwiftUI

#if canImport(UIKit)
import UIKit
#elseif canImport(AppKit)
import AppKit
#endif

/// #EXP-2215: Inter, the `exponential` theme's `type.family.sans` and the
/// face web and desktop draw. The package bundles the four faces
/// (`Resources/Fonts`, the same files as apps/desktop/assets/fonts, SIL OFL
/// 1.1: `OFL-Inter.txt` beside them) ONCE: genUI resolves `Inter` to them
/// when the host registered none, and an app draws its own text through
/// `Font.inter` / `PlatformInter.font`, never a second copy.
public enum InterFont {
    /// The family name themes ask for.
    public static let family = "Inter"
    /// The bundled faces (400, 500, 600, 700).
    public static let files = ["Inter-Regular", "Inter-Medium", "Inter-SemiBold", "Inter-Bold"]

    /// The bundled files' URLs.
    public static var urls: [URL] {
        files.compactMap { Bundle.module.url(forResource: $0, withExtension: "ttf", subdirectory: "Fonts") }
    }

    private static let registered: Bool = {
        var ok = true
        for url in urls {
            var error: Unmanaged<CFError>?
            if CTFontManagerRegisterFontsForURL(url as CFURL, .process, &error) { continue }
            if let e = error?.takeRetainedValue(), CFErrorGetCode(e) == CTFontManagerError.alreadyRegistered.rawValue { continue }
            ok = false
        }
        return ok
    }()

    /// Register the bundled faces with the process (once; every `Font.inter`
    /// call does it). False when CoreText refused a file.
    @discardableResult
    public static func register() -> Bool { registered }

    /// The PostScript name of the bundled face for a CSS weight (CSS font
    /// matching over 400 / 500 / 600 / 700).
    public static func postScriptName(weight: Int) -> String {
        switch weight {
        case ..<500: "Inter-Regular"
        case ..<600: "Inter-Medium"
        case ..<700: "Inter-SemiBold"
        default: "Inter-Bold"
        }
    }
}

extension Font {
    /// Inter at `size` pt and `weight`. With `relativeTo`, the size scales
    /// with Dynamic Type like that text style. `.weight(_:)`, `.bold()` and
    /// `.fontWeight(_:)` pick the family's other faces.
    public static func inter(size: CGFloat, weight: Font.Weight = .regular, relativeTo style: Font.TextStyle? = nil) -> Font {
        InterFont.register()
        let base: Font = style.map { .custom(InterFont.family, size: size, relativeTo: $0) } ?? .custom(InterFont.family, fixedSize: size)
        return weight == .regular ? base : base.weight(weight)
    }
}

#if canImport(UIKit)
/// Inter as a platform font (TextKit runs, UIKit appearances).
public enum PlatformInter {
    /// Inter at `size` and a UIKit weight; with `relativeTo`, scaled by
    /// that text style's `UIFontMetrics` (Dynamic Type).
    public static func font(size: CGFloat, weight: UIFont.Weight = .regular, relativeTo style: UIFont.TextStyle? = nil) -> UIFont {
        InterFont.register()
        let css = weight.rawValue >= UIFont.Weight.bold.rawValue ? 700 : weight.rawValue >= UIFont.Weight.semibold.rawValue ? 600 : weight.rawValue >= UIFont.Weight.medium.rawValue ? 500 : 400
        let font = UIFont(name: InterFont.postScriptName(weight: css), size: size) ?? .systemFont(ofSize: size, weight: weight)
        return style.map { UIFontMetrics(forTextStyle: $0).scaledFont(for: font) } ?? font
    }
}
#elseif canImport(AppKit)
/// Inter as a platform font (TextKit runs).
public enum PlatformInter {
    public static func font(size: CGFloat, weight: NSFont.Weight = .regular) -> NSFont {
        InterFont.register()
        let css = weight.rawValue >= NSFont.Weight.bold.rawValue ? 700 : weight.rawValue >= NSFont.Weight.semibold.rawValue ? 600 : weight.rawValue >= NSFont.Weight.medium.rawValue ? 500 : 400
        return NSFont(name: InterFont.postScriptName(weight: css), size: size) ?? .systemFont(ofSize: size, weight: weight)
    }
}
#endif
