import SwiftUI

public extension Color {
    /// `#rgb`, `#rrggbb`, `#rrggbbaa` (what the core's resolved visuals carry).
    /// Anything else returns nil.
    init?(hex: String) {
        var s = hex.trimmingCharacters(in: .whitespaces)
        guard s.hasPrefix("#") else { return nil }
        s.removeFirst()
        if s.count == 3 || s.count == 4 {
            s = s.map { "\($0)\($0)" }.joined()
        }
        guard s.count == 6 || s.count == 8, let v = UInt64(s, radix: 16) else { return nil }
        let a = s.count == 8 ? Double(v & 0xFF) / 255 : 1
        let shift: UInt64 = s.count == 8 ? 8 : 0
        let r = Double((v >> (16 + shift)) & 0xFF) / 255
        let g = Double((v >> (8 + shift)) & 0xFF) / 255
        let b = Double((v >> shift) & 0xFF) / 255
        self.init(.sRGB, red: r, green: g, blue: b, opacity: a)
    }
}

/// The RGBA channels of a hex colour, for the platform colour types.
public struct RGBA: Equatable, Sendable {
    public var r: Double, g: Double, b: Double, a: Double

    public init(r: Double, g: Double, b: Double, a: Double = 1) {
        self.r = r; self.g = g; self.b = b; self.a = a
    }

    public init?(hex: String) {
        var s = hex.trimmingCharacters(in: .whitespaces)
        guard s.hasPrefix("#") else { return nil }
        s.removeFirst()
        if s.count == 3 || s.count == 4 {
            s = s.map { "\($0)\($0)" }.joined()
        }
        guard s.count == 6 || s.count == 8, let v = UInt64(s, radix: 16) else { return nil }
        a = s.count == 8 ? Double(v & 0xFF) / 255 : 1
        let shift: UInt64 = s.count == 8 ? 8 : 0
        r = Double((v >> (16 + shift)) & 0xFF) / 255
        g = Double((v >> (8 + shift)) & 0xFF) / 255
        b = Double((v >> shift) & 0xFF) / 255
    }

    public var color: Color { Color(.sRGB, red: r, green: g, blue: b, opacity: a) }

    /// Relative luminance (sRGB, for dark/light decisions).
    public var luminance: Double { 0.2126 * r + 0.7152 * g + 0.0722 * b }
}

public extension RGBA {
    /// An HSL colour (hue in degrees), like the React renderer's avatar tint.
    init(hue: Double, saturation: Double, lightness: Double, alpha: Double = 1) {
        let h = (hue.truncatingRemainder(dividingBy: 360) + 360).truncatingRemainder(dividingBy: 360) / 360
        let s = saturation, l = lightness
        let c = (1 - abs(2 * l - 1)) * s
        let x = c * (1 - abs((h * 6).truncatingRemainder(dividingBy: 2) - 1))
        let m = l - c / 2
        let (r1, g1, b1): (Double, Double, Double)
        switch h * 6 {
        case ..<1: (r1, g1, b1) = (c, x, 0)
        case ..<2: (r1, g1, b1) = (x, c, 0)
        case ..<3: (r1, g1, b1) = (0, c, x)
        case ..<4: (r1, g1, b1) = (0, x, c)
        case ..<5: (r1, g1, b1) = (x, 0, c)
        default: (r1, g1, b1) = (c, 0, x)
        }
        self.init(r: r1 + m, g: g1 + m, b: b1 + m, a: alpha)
    }
}
