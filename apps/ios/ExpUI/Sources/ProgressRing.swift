import SwiftUI

/// EXP-1097 — the sub-issue COMPLETION ring that leads the issue detail's
/// "Sub-issues" band: `done` of `total` as an arc over a 20% track, ×4 (web
/// `@exp/ui` `ProgressRing`, desktop + Android twins). The same 16-unit box
/// and 2-unit stroke as `ContextRing`, so the two rings read as one family,
/// drawn at icon size (14pt by default) and painted in the team's COMPLETED
/// status colour.
public struct ProgressRing: View {
    let done: Int
    let total: Int
    let color: Color
    let size: CGFloat

    public init(done: Int, total: Int, color: Color, size: CGFloat = 14) {
        self.done = done
        self.total = total
        self.color = color
        self.size = size
    }

    /// The filled share, 0…1 — an empty or nonsensical total draws none.
    public static func fraction(done: Int, total: Int) -> Double {
        guard total > 0 else { return 0 }
        return Double(min(max(done, 0), total)) / Double(total)
    }

    public var body: some View {
        // The 16-unit geometry scaled to the drawn box.
        let lineWidth = ContextRingTokens.lineWidth * size / ContextRingTokens.size
        ZStack {
            Circle()
                .stroke(color.opacity(0.2), lineWidth: lineWidth)
            if done > 0 {
                Circle()
                    .trim(from: 0, to: Self.fraction(done: done, total: total))
                    .stroke(color, style: StrokeStyle(lineWidth: lineWidth, lineCap: .round))
                    .rotationEffect(.degrees(-90))
            }
        }
        .padding(lineWidth / 2)
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}
