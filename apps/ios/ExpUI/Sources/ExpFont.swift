import SwiftUI
import UIKit
internal import ExponentialUIPrimitives

/// #EXP-2215: every text is Inter, the face web and desktop draw, from the
/// SDK's one bundled copy (`ExponentialUIPrimitives` `InterFont`; the app
/// bundles none). The iOS text styles map onto the shared token scale
/// (`DesignTokens.Typography.Size`, web's text-xs…2xl, Android's pairs) at
/// tracking 0 and keep `relativeTo:`, so Dynamic Type still scales them:
/// body / callout = base, headline = base semibold, subheadline = sm,
/// footnote / caption / caption2 = xs, title1…3 = xl, largeTitle = 2xl.
public enum ExpType {
    /// The token size a text style maps onto.
    public static func size(_ style: Font.TextStyle) -> CGFloat {
        typealias S = DesignTokens.Typography.Size
        switch style {
        case .largeTitle: return S.xl2
        case .title, .title2, .title3: return S.xl
        case .headline, .body, .callout: return S.base
        case .subheadline: return S.sm
        case .footnote, .caption, .caption2: return S.xs
        @unknown default: return S.base
        }
    }

    static func uiStyle(_ style: Font.TextStyle) -> UIFont.TextStyle {
        switch style {
        case .largeTitle: .largeTitle
        case .title: .title1
        case .title2: .title2
        case .title3: .title3
        case .headline: .headline
        case .body: .body
        case .callout: .callout
        case .subheadline: .subheadline
        case .footnote: .footnote
        case .caption: .caption1
        case .caption2: .caption2
        @unknown default: .body
        }
    }
}

extension Font {
    /// Inter at a text style's token size, scaling with Dynamic Type.
    public static func exp(_ style: Font.TextStyle, weight: Font.Weight? = nil) -> Font {
        .inter(size: ExpType.size(style), weight: weight ?? (style == .headline ? .semibold : .regular), relativeTo: style)
    }

    /// Inter at a fixed size (the `.system(size:weight:)` it replaces).
    public static func exp(size: CGFloat, weight: Font.Weight = .regular) -> Font {
        .inter(size: size, weight: weight)
    }

    public static var expLargeTitle: Font { exp(.largeTitle) }
    public static var expTitle: Font { exp(.title) }
    public static var expTitle2: Font { exp(.title2) }
    public static var expTitle3: Font { exp(.title3) }
    public static var expHeadline: Font { exp(.headline) }
    public static var expBody: Font { exp(.body) }
    public static var expCallout: Font { exp(.callout) }
    public static var expSubheadline: Font { exp(.subheadline) }
    public static var expFootnote: Font { exp(.footnote) }
    public static var expCaption: Font { exp(.caption) }
    public static var expCaption2: Font { exp(.caption2) }
}

extension UIFont {
    /// Inter at a text style's token size, scaled by its `UIFontMetrics`
    /// (UIKit appearances, TextKit runs).
    public static func exp(_ style: Font.TextStyle, weight: UIFont.Weight = .regular) -> UIFont {
        PlatformInter.font(size: ExpType.size(style), weight: weight, relativeTo: ExpType.uiStyle(style))
    }

    /// Inter at a fixed size (the `.system(size:)` it replaces did not scale).
    public static func inter(size: CGFloat, weight: UIFont.Weight = .regular) -> UIFont {
        PlatformInter.font(size: size, weight: weight)
    }
}

/// #EXP-2215: Inter where SwiftUI's font environment does not reach (the
/// UIKit bars and fields SwiftUI hosts). Once, before the first scene.
@MainActor
public enum ExpAppearance {
    public static func applyInter() {
        let nav = UINavigationBar.appearance()
        nav.titleTextAttributes = [.font: UIFont.exp(.headline, weight: .semibold)]
        nav.largeTitleTextAttributes = [.font: UIFont.exp(.largeTitle, weight: .bold)]
        let item = UIBarButtonItem.appearance()
        for state: UIControl.State in [.normal, .highlighted, .disabled, .focused] {
            item.setTitleTextAttributes([.font: UIFont.exp(.body)], for: state)
        }
        UITabBarItem.appearance().setTitleTextAttributes([.font: UIFont.exp(.caption2, weight: .medium)], for: .normal)
        UISearchTextField.appearance().font = .exp(.body)
        UITextField.appearance(whenContainedInInstancesOf: [UISearchBar.self]).font = .exp(.body)
        UISegmentedControl.appearance().setTitleTextAttributes([.font: UIFont.exp(.subheadline, weight: .medium)], for: .normal)
    }
}
