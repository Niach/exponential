import SwiftUI

/// The avatar fallback rule every Exponential UI renderer shares: two
/// initials, tinted from a seed (the React renderer's `seedHue`, the gpui
/// painter's `seed_hue`: `h = h * 31 + utf16unit`, mod 360).
public enum AvatarFallback {
    public static func initials(_ name: String) -> String {
        let letters = name.split(whereSeparator: { $0.isWhitespace }).prefix(2).compactMap { $0.first }.map { String($0).uppercased() }
        return letters.joined()
    }

    public static func seedHue(_ seed: String) -> Double {
        var h: UInt32 = 0
        for unit in seed.utf16 {
            h = h &* 31 &+ UInt32(unit)
        }
        return Double(h % 360)
    }

    /// The fill and the letter colour for a seed in a mode.
    public static func tint(seed: String, dark: Bool) -> (fill: RGBA, ink: RGBA) {
        let hue = seedHue(seed)
        return (RGBA(hue: hue, saturation: 0.7, lightness: 0.55, alpha: 0.22), RGBA(hue: hue, saturation: 0.6, lightness: dark ? 0.72 : 0.38))
    }
}

/// A round avatar: the image when one loads, else tinted initials. The
/// Exponential app's `UserAvatar` / `TeamAvatar` wrap it with their own hue
/// rule; the painter feeds it the catalog `Avatar` props.
public struct AvatarView<Picture: View>: View {
    let name: String
    let seed: String
    let size: CGFloat
    let dark: Bool
    let fill: Color?
    let ink: Color?
    let fontSize: CGFloat?
    let initials: String?
    let cornerRadius: CGFloat?
    let font: Font?
    let minimumScaleFactor: CGFloat
    let picture: Picture

    /// - `fill`/`ink` override the seed tint (the `Avatar/fallback` recipe
    ///   when the seed is empty); `fontSize` the letters' size (default 40%).
    /// - `initials` draws those letters verbatim instead of deriving them
    ///   from `name`; `cornerRadius` clips a rounded square instead of a
    ///   circle; `font` wins over `fontSize` (a Dynamic Type style);
    ///   `minimumScaleFactor` is how far the letters shrink before they
    ///   truncate.
    public init(
        name: String,
        seed: String? = nil,
        size: CGFloat = 32,
        dark: Bool = true,
        fill: Color? = nil,
        ink: Color? = nil,
        fontSize: CGFloat? = nil,
        initials: String? = nil,
        cornerRadius: CGFloat? = nil,
        font: Font? = nil,
        minimumScaleFactor: CGFloat = 0.5,
        @ViewBuilder picture: () -> Picture
    ) {
        self.name = name
        self.seed = seed ?? name
        self.size = size
        self.dark = dark
        self.fill = fill
        self.ink = ink
        self.fontSize = fontSize
        self.initials = initials
        self.cornerRadius = cornerRadius
        self.font = font
        self.minimumScaleFactor = minimumScaleFactor
        self.picture = picture()
    }

    public var body: some View {
        ZStack {
            if Picture.self != EmptyView.self {
                picture
            } else {
                let tint = AvatarFallback.tint(seed: seed, dark: dark)
                let derived = AvatarFallback.initials(name)
                Rectangle().fill(fill ?? (seed.isEmpty ? Color.gray.opacity(0.2) : tint.fill.color))
                Text(initials ?? (derived.isEmpty ? "?" : derived))
                    .font(font ?? .system(size: fontSize ?? (size * 0.4).rounded(), weight: .medium))
                    .foregroundStyle(ink ?? (seed.isEmpty ? Color.primary : tint.ink.color))
                    .lineLimit(1)
                    .minimumScaleFactor(minimumScaleFactor)
            }
        }
        .frame(width: size, height: size)
        .clipShape(AvatarShape(cornerRadius: cornerRadius))
        .accessibilityLabel(name)
    }
}

extension AvatarView where Picture == EmptyView {
    public init(
        name: String,
        seed: String? = nil,
        size: CGFloat = 32,
        dark: Bool = true,
        fill: Color? = nil,
        ink: Color? = nil,
        fontSize: CGFloat? = nil,
        initials: String? = nil,
        cornerRadius: CGFloat? = nil,
        font: Font? = nil,
        minimumScaleFactor: CGFloat = 0.5
    ) {
        self.init(
            name: name, seed: seed, size: size, dark: dark, fill: fill, ink: ink, fontSize: fontSize,
            initials: initials, cornerRadius: cornerRadius, font: font, minimumScaleFactor: minimumScaleFactor,
            picture: { EmptyView() }
        )
    }
}

/// A circle, or a rounded square when a corner radius is given.
struct AvatarShape: Shape {
    let cornerRadius: CGFloat?

    func path(in rect: CGRect) -> Path {
        if let cornerRadius {
            return RoundedRectangle(cornerRadius: cornerRadius).path(in: rect)
        }
        return Circle().path(in: rect)
    }
}
