import SwiftUI
import UIKit

/// #EXP-1629: every mono run is JetBrains Mono (OFL-1.1), the face the IDE
/// bundles and the web self-hosts: the same look ×4, never the system mono.
/// The app bundles the four faces (`UIAppFonts`, Project.swift); genUI's
/// `exponential` theme names the same family, which CoreText then resolves.
public enum ExpMono {
    /// The family name genUI themes ask for.
    public static let family = "JetBrains Mono"

    /// The face for a weight: CSS matching over the two bundled weights
    /// (≤ 500 = Regular, ≥ 600 = Bold), as the web and the IDE pick.
    static func postScriptName(bold: Bool, italic: Bool) -> String {
        switch (bold, italic) {
        case (false, false): "JetBrainsMono-Regular"
        case (true, false): "JetBrainsMono-Bold"
        case (false, true): "JetBrainsMono-Italic"
        case (true, true): "JetBrainsMono-BoldItalic"
        }
    }

    static func isBold(_ weight: Font.Weight) -> Bool {
        [.semibold, .bold, .heavy, .black].contains(weight)
    }

    /// The text style's default (Large) point size, the size Dynamic Type
    /// scales from (`Font.custom(_:size:relativeTo:)`).
    static func pointSize(_ style: Font.TextStyle) -> CGFloat {
        switch style {
        case .largeTitle: 34
        case .title: 28
        case .title2: 22
        case .title3: 20
        case .headline, .body: 17
        case .callout: 16
        case .subheadline: 15
        case .footnote: 13
        case .caption: 12
        case .caption2: 11
        @unknown default: 17
        }
    }
}

extension Font {
    /// JetBrains Mono at a text style's size, scaling with Dynamic Type (the
    /// system `.caption.monospaced()` it replaces did).
    public static func expMono(_ style: Font.TextStyle, weight: Font.Weight = .regular, italic: Bool = false) -> Font {
        .custom(ExpMono.postScriptName(bold: ExpMono.isBold(weight) || style == .headline, italic: italic), size: ExpMono.pointSize(style), relativeTo: style)
    }

    /// JetBrains Mono at a fixed size.
    public static func expMono(size: CGFloat, weight: Font.Weight = .regular, italic: Bool = false) -> Font {
        .custom(ExpMono.postScriptName(bold: ExpMono.isBold(weight), italic: italic), fixedSize: size)
    }
}

extension UIFont {
    /// JetBrains Mono for TextKit runs (code blocks, diffs, markdown code).
    public static func expMono(size: CGFloat, weight: UIFont.Weight = .regular, italic: Bool = false) -> UIFont {
        UIFont(name: ExpMono.postScriptName(bold: weight.rawValue >= UIFont.Weight.semibold.rawValue, italic: italic), size: size)
            ?? .monospacedSystemFont(ofSize: size, weight: weight)
    }
}
