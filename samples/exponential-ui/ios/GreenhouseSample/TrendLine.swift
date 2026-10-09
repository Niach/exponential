import SwiftUI
import ExponentialUI

/// The sample extension's ONE native component, `TrendLine {values, color?,
/// height = 48}`, painted natively: a polyline of the bound readings in
/// `color` or the theme's primary, `height` tall, as wide as its frame.
final class TrendLinePainter: ExtensionPainter {
    func measure(_ leaf: ExtensionLeaf, wrap: CGFloat?) -> CGSize? {
        let height = CGFloat(leaf.props["height"]?.number ?? 48)
        // Any width fits: min-content 0, otherwise the width offered.
        return CGSize(width: wrap ?? 120, height: height)
    }

    func paint(_ context: ExtensionContext) -> AnyView {
        let values = (context.props["values"]?.array ?? []).compactMap(\.number)
        let color = context.props["color"]?.string.flatMap(Self.color(hex:)) ?? context.theme?.color("primary", mode: context.mode) ?? .accentColor
        return AnyView(
            TrendLine(values: values)
                .stroke(color, style: StrokeStyle(lineWidth: 2.5, lineCap: .round, lineJoin: .round))
                .frame(width: context.size.width, height: context.size.height)
                .accessibilityElement()
                .accessibilityLabel("\(values.count) readings")
        )
    }

    static func color(hex: String) -> Color? {
        var s = hex.trimmingCharacters(in: .whitespaces)
        guard s.hasPrefix("#") else { return nil }
        s.removeFirst()
        guard s.count == 6, let v = UInt64(s, radix: 16) else { return nil }
        return Color(red: Double((v >> 16) & 0xFF) / 255, green: Double((v >> 8) & 0xFF) / 255, blue: Double(v & 0xFF) / 255)
    }
}

/// The polyline: oldest reading on the left, min at the bottom, max at the top.
struct TrendLine: Shape {
    let values: [Double]

    func path(in rect: CGRect) -> Path {
        var path = Path()
        guard let lo = values.min(), let hi = values.max() else { return path }
        let inset = rect.insetBy(dx: 2, dy: 2)
        for (i, v) in values.enumerated() {
            let x = inset.minX + inset.width * CGFloat(i) / CGFloat(max(values.count - 1, 1))
            let y = hi == lo ? inset.midY : inset.maxY - inset.height * CGFloat((v - lo) / (hi - lo))
            if i == 0 { path.move(to: CGPoint(x: x, y: y)) } else { path.addLine(to: CGPoint(x: x, y: y)) }
        }
        return path
    }
}
