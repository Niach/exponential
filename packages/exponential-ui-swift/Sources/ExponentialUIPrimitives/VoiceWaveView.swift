import SwiftUI

/// NEW-77: three closed polar waves (the catalog `VoiceWave`; the app's
/// voice overlay). Draws the rings it is given, outer first: each filled
/// with its colour at `fillAlpha`, stroked at `strokeAlpha`, `strokeWidth`
/// wide. The points come from the core (`voiceWaveJson`), so this view
/// computes nothing.
public struct VoiceWaveView: View {
    public struct Ring: Equatable, Sendable {
        public var points: [CGPoint]
        public var color: Color
        public var fillAlpha: Double
        public var strokeAlpha: Double
        public var strokeWidth: CGFloat

        public init(points: [CGPoint], color: Color, fillAlpha: Double, strokeAlpha: Double, strokeWidth: CGFloat) {
            self.points = points
            self.color = color
            self.fillAlpha = fillAlpha
            self.strokeAlpha = strokeAlpha
            self.strokeWidth = strokeWidth
        }

        /// The closed path through the points (`M … L … Z`).
        public var path: Path {
            var p = Path()
            guard let first = points.first else { return p }
            p.move(to: first)
            for pt in points.dropFirst() { p.addLine(to: pt) }
            p.closeSubpath()
            return p
        }
    }

    let rings: [Ring]

    public init(rings: [Ring]) {
        self.rings = rings
    }

    public var body: some View {
        Canvas { ctx, _ in
            for ring in rings {
                let path = ring.path
                ctx.fill(path, with: .color(ring.color.opacity(ring.fillAlpha)))
                ctx.stroke(path, with: .color(ring.color.opacity(ring.strokeAlpha)), style: StrokeStyle(lineWidth: ring.strokeWidth, lineJoin: .round))
            }
        }
        .accessibilityHidden(true)
    }
}
