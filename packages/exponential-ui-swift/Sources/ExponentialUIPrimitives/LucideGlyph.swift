import SwiftUI

/// One Lucide path in its 24-unit box, pre-normalised to absolute
/// `M x y`, `L x y`, `C x1 y1 x2 y2 x y`, `Q x1 y1 x y` and `Z`, every token
/// space-separated (what `Example/scripts/generate-lucide.ts` writes), scaled
/// to fit and centred in the rect.
public struct LucidePath: Shape {
    public let d: String

    public init(_ d: String) { self.d = d }

    public func path(in rect: CGRect) -> Path {
        let s = min(rect.width, rect.height) / 24
        let ox = rect.midX - 12 * s, oy = rect.midY - 12 * s
        var p = Path()
        let tokens = d.split(separator: " ")
        var i = 0
        func n() -> CGFloat {
            defer { i += 1 }
            return i < tokens.count ? CGFloat(Double(tokens[i]) ?? 0) : 0
        }
        func pt() -> CGPoint {
            let x = n(), y = n()
            return CGPoint(x: ox + x * s, y: oy + y * s)
        }
        while i < tokens.count {
            let cmd = tokens[i]
            i += 1
            switch cmd {
            case "M": p.move(to: pt())
            case "L": p.addLine(to: pt())
            case "C":
                let c1 = pt(), c2 = pt()
                p.addCurve(to: pt(), control1: c1, control2: c2)
            case "Q":
                let c = pt()
                p.addQuadCurve(to: pt(), control: c)
            case "Z": p.closeSubpath()
            default: break
            }
        }
        return p
    }
}

/// Several `LucidePath`s as one shape (one stroke pass per glyph).
public struct LucidePaths: Shape {
    public let paths: [String]

    public init(_ paths: [String]) { self.paths = paths }

    public func path(in rect: CGRect) -> Path {
        var p = Path()
        for d in paths { p.addPath(LucidePath(d).path(in: rect)) }
        return p
    }
}

/// A Lucide glyph `size` square in the current foreground style: its
/// paths stroked at 2/24 of the size with round caps and joins (Lucide's
/// `stroke-width="2"`), a path prefixed `F ` filled only and `B ` filled
/// and stroked.
public struct LucideGlyph: View {
    public let paths: [String]
    public let size: CGFloat

    public init(paths: [String], size: CGFloat) {
        self.paths = paths
        self.size = size
    }

    public var body: some View {
        let style = StrokeStyle(lineWidth: size * 2 / 24, lineCap: .round, lineJoin: .round)
        let parts = paths.map(Self.split)
        let fills = parts.filter(\.0).map(\.2)
        let strokes = parts.filter(\.1).map(\.2)
        ZStack {
            if !fills.isEmpty { LucidePaths(fills).fill() }
            LucidePaths(strokes).stroke(style: style)
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }

    /// `(filled, stroked, path)` of one encoded path.
    static func split(_ raw: String) -> (Bool, Bool, String) {
        if raw.hasPrefix("F ") { return (true, false, String(raw.dropFirst(2))) }
        if raw.hasPrefix("B ") { return (true, true, String(raw.dropFirst(2))) }
        return (false, true, raw)
    }
}
