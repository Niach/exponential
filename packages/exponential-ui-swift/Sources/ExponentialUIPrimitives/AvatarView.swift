import SwiftUI

/// The avatar fallback rule every Exponential UI renderer shares
/// (`packages/exponential-ui/fixtures/avatar.json`, #EXP-1629), the app's
/// own (design-tokens `avatar`, EXP-698 r4): a seed (a user id) picks
/// `palette[fnv1a32(utf8(seed)) % 8]`; the fill is that hue at `fillAlpha`,
/// the initials the hue at full alpha, no stroke. No seed = the
/// `Avatar/fallback` recipe colours (muted).
public enum AvatarFallback {
    /// design-tokens `avatar` in ITS order (append-only: the index is what
    /// the hash lands on).
    public static let palette: [RGBA] = ["#F87171", "#FB923C", "#FACC15", "#4ADE80", "#2DD4BF", "#60A5FA", "#A78BFA", "#F472B6"].map { RGBA(hex: $0)! }

    /// The hue's alpha in the fallback's fill.
    public static let fillAlpha: Double = 0.2

    /// FNV-1a 32-bit over the seed's UTF-8 bytes, modulo the palette size.
    public static func hueIndex(_ seed: String) -> Int {
        var hash: UInt32 = 0x811c9dc5
        for byte in seed.utf8 {
            hash ^= UInt32(byte)
            hash = hash &* 0x01000193
        }
        return Int(hash % UInt32(palette.count))
    }

    /// What the hue is keyed on (fixtures/avatar.json `keys`, #EXP-1754): the
    /// id, else the trimmed name, else the trimmed email; blank counts as
    /// absent. `nil` = no key: the muted fallback.
    public static func hueKey(id: String?, name: String? = nil, email: String? = nil) -> String? {
        if let id, !id.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { return id }
        for label in [name, email] {
            let trimmed = (label ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
            if !trimmed.isEmpty { return trimmed }
        }
        return nil
    }

    /// The cross-client initials: an email reduces to its local part, the
    /// rest splits on space . _ - +; two or more parts give their first
    /// letters, one part its first two characters; uppercased; nothing = `?`.
    public static func initials(_ name: String) -> String {
        let base = name.trimmingCharacters(in: .whitespacesAndNewlines)
        let local = base.firstIndex(of: "@").map { String(base[..<$0]) } ?? base
        let separators: Set<Character> = [" ", ".", "_", "-", "+"]
        let parts = local.split(whereSeparator: { separators.contains($0) }).filter { !$0.trimmingCharacters(in: .whitespaces).isEmpty }
        let picked = parts.count >= 2 ? String(parts[0].prefix(1)) + String(parts[1].prefix(1)) : String(local.prefix(2))
        return picked.isEmpty ? "?" : picked.uppercased()
    }

    /// The fill and the letter colour for a seed (the same in both modes).
    public static func tint(seed: String, dark: Bool = true) -> (fill: RGBA, ink: RGBA) {
        let hue = palette[hueIndex(seed)]
        var fill = hue
        fill.a = fillAlpha
        return (fill, hue)
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
    let fontWeight: Font.Weight
    let initials: String?
    let cornerRadius: CGFloat?
    let font: Font?
    let minimumScaleFactor: CGFloat
    let ring: Color?
    let ringWidth: CGFloat
    let picture: Picture

    /// - `seed` (a user id) picks the palette tint; none or empty = muted
    ///   (`fill`/`ink`, else a grey wash). `fill`/`ink` override the tint;
    ///   `fontSize` the letters' size (default half the diameter),
    ///   `fontWeight` their weight (default regular).
    /// - `initials` draws those letters verbatim instead of deriving them
    ///   from `name`; `cornerRadius` clips a rounded square instead of a
    ///   circle; `font` wins over `fontSize` (a Dynamic Type style);
    ///   `minimumScaleFactor` is how far the letters shrink before they
    ///   truncate.
    /// - `ring` strokes `ringWidth` inside the shape over a picture (the
    ///   `Avatar/image` part), never over initials.
    public init(
        name: String,
        seed: String? = nil,
        size: CGFloat = 32,
        dark: Bool = true,
        fill: Color? = nil,
        ink: Color? = nil,
        fontSize: CGFloat? = nil,
        fontWeight: Font.Weight = .regular,
        initials: String? = nil,
        cornerRadius: CGFloat? = nil,
        font: Font? = nil,
        minimumScaleFactor: CGFloat = 0.5,
        ring: Color? = nil,
        ringWidth: CGFloat = 1,
        @ViewBuilder picture: () -> Picture
    ) {
        self.name = name
        self.seed = seed ?? ""
        self.size = size
        self.dark = dark
        self.fill = fill
        self.ink = ink
        self.fontSize = fontSize
        self.fontWeight = fontWeight
        self.initials = initials
        self.cornerRadius = cornerRadius
        self.font = font
        self.minimumScaleFactor = minimumScaleFactor
        self.ring = ring
        self.ringWidth = ringWidth
        self.picture = picture()
    }

    public var body: some View {
        ZStack {
            if Picture.self != EmptyView.self {
                if let ring, ringWidth > 0 {
                    // The picture sits inside the ring (the IDE's look).
                    picture
                        .frame(width: max(size - 2 * ringWidth, 0), height: max(size - 2 * ringWidth, 0))
                        .clipShape(AvatarShape(cornerRadius: cornerRadius))
                    AvatarShape(cornerRadius: cornerRadius).strokeBorder(ring, lineWidth: ringWidth)
                } else {
                    picture
                }
            } else {
                let tint = AvatarFallback.tint(seed: seed, dark: dark)
                Rectangle().fill(fill ?? (seed.isEmpty ? Color.gray.opacity(0.2) : tint.fill.color))
                Text(initials ?? AvatarFallback.initials(name))
                    .font(font ?? .system(size: fontSize ?? (size * 0.5).rounded(), weight: fontWeight))
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
        fontWeight: Font.Weight = .regular,
        initials: String? = nil,
        cornerRadius: CGFloat? = nil,
        font: Font? = nil,
        minimumScaleFactor: CGFloat = 0.5
    ) {
        self.init(
            name: name, seed: seed, size: size, dark: dark, fill: fill, ink: ink, fontSize: fontSize, fontWeight: fontWeight,
            initials: initials, cornerRadius: cornerRadius, font: font, minimumScaleFactor: minimumScaleFactor,
            picture: { EmptyView() }
        )
    }
}

/// A circle, or a rounded square when a corner radius is given.
struct AvatarShape: InsettableShape {
    let cornerRadius: CGFloat?
    var insetAmount: CGFloat = 0

    func path(in rect: CGRect) -> Path {
        let r = rect.insetBy(dx: insetAmount, dy: insetAmount)
        if let cornerRadius {
            return RoundedRectangle(cornerRadius: max(0, cornerRadius - insetAmount)).path(in: r)
        }
        return Circle().path(in: r)
    }

    func inset(by amount: CGFloat) -> AvatarShape {
        var s = self
        s.insetAmount += amount
        return s
    }
}
