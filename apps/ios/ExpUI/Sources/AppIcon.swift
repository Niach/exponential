import SwiftUI
import UIKit

/// EXP-273 — the one way to draw a registry icon on iOS.
///
/// The shared registry (`packages/icons/icons.json`) ships every glyph as a
/// Lucide SVG imageset in the app's asset catalog, generated alongside the
/// web/Android/desktop projections so all four clients draw identical art.
/// This replaces `Image(systemName:)` for every unified surface.
///
/// Why a wrapper instead of bare `Image("lucide-…")`: SF Symbols size
/// themselves from the ambient `.font(…)` and scale with Dynamic Type, while a
/// template imageset has an intrinsic pixel size and ignores both. `@ScaledMetric`
/// restores the Dynamic Type behaviour, and the `size` parameter takes the place
/// of the `.font()` modifier the SF Symbol call sites used to carry — so an icon
/// still grows with the user's text-size setting instead of staying pinned at
/// whatever looked right on the design canvas.
public struct AppIcon: View {
    private let assetName: String?
    private let baseSize: CGFloat
    private let weight: Font.Weight?
    @ScaledMetric private var scale: CGFloat = 1

    /// Draw a registry icon by its Lucide name (`"bug"`, `"git-pull-request"`).
    /// An unknown name renders nothing rather than Xcode's pink placeholder —
    /// a stored `boards.icon` from a newer client must never break the row.
    public init(_ name: String, size: CGFloat = 17, weight: Font.Weight? = nil) {
        self.assetName = AppIcons.assetName(name)
        self.baseSize = size
        self.weight = weight
    }

    public var body: some View {
        if let assetName {
            Image(assetName)
                .renderingMode(.template)
                .resizable()
                .scaledToFit()
                .frame(width: baseSize * scale, height: baseSize * scale)
                // Lucide art is a uniform 2px stroke on a 24pt grid, so it has
                // no weight axis. Callers that used `.font(.body.weight(.semibold))`
                // on an SF Symbol get the nearest equivalent: nothing changes
                // geometrically, but the accessibility/vibrancy treatment of the
                // surrounding label still matches.
                .fontWeight(weight)
        }
    }
}

public extension AppIcons {
    /// UIKit rendition of a registry icon — the stand-in for
    /// `UIImage(systemName:withConfiguration:)` in hand-built UIKit chrome
    /// (the markdown keyboard toolbar).
    ///
    /// A `SymbolConfiguration` has no effect on a vector imageset: it keeps its
    /// natural 24pt size, so the requested point size has to be baked in by
    /// redrawing. The result is returned as a template image so the caller's
    /// `tintColor` still drives the color, exactly like a symbol.
    static func uiImage(_ name: String, pointSize: CGFloat) -> UIImage? {
        guard let asset = assetName(name), let image = UIImage(named: asset) else { return nil }
        let size = CGSize(width: pointSize, height: pointSize)
        let scaled = UIGraphicsImageRenderer(size: size).image { _ in
            image.draw(in: CGRect(origin: .zero, size: size))
        }
        return scaled.withRenderingMode(.alwaysTemplate)
    }
}

public extension Label where Title == Text, Icon == Image {
    /// Registry icon inside a `Label` — the form context menus, `Menu` rows and
    /// swipe actions need.
    ///
    /// Those surfaces bridge to UIKit (`UIMenu`/`UIContextualAction`), which only
    /// renders an SF Symbol or an asset-catalog image and silently drops an
    /// arbitrary icon View — so they address the imageset by name instead of
    /// going through `AppIcon`. The catalog marks every Lucide entry as a
    /// template image, so UIKit still tints it like a symbol.
    init(_ title: String, appIcon name: String) {
        self.init(title, image: AppIcons.assetName(name) ?? name)
    }
}

public extension AppIcon {
    /// Common sizes, named so call sites read like the `.font(…)` they replaced.
    /// These match the point sizes SF Symbols resolved to at the default text
    /// size, so swapping a symbol for a registry icon is visually neutral.
    enum Size {
        /// Inline with `.caption` / secondary metadata rows.
        public static let small: CGFloat = 13
        /// Inline with `.body` — the default.
        public static let medium: CGFloat = 17
        /// Toolbar and tab-bar glyphs.
        public static let large: CGFloat = 20
        /// Empty-state and onboarding art.
        public static let xlarge: CGFloat = 44
    }
}
