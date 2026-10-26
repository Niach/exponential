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
        VoiceWaveRings(rings: rings)
    }

    /// A wave of `size` pt with its ONE `child` centred at its own size and
    /// drawn at `childScale` about the centre (the core's `child.scale` for
    /// the child's size, `voiceWaveChild`).
    public static func with<Child: View>(rings: [Ring], size: CGFloat, childScale: CGFloat, @ViewBuilder child: () -> Child) -> some View {
        ZStack {
            VoiceWaveRings(rings: rings)
            child().voiceWaveChild(scale: childScale)
        }
        .frame(width: size, height: size)
    }
}

/// The rings alone (outer first).
struct VoiceWaveRings: View {
    let rings: [VoiceWaveView.Ring]

    var body: some View {
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

public extension View {
    /// THE CHILD RULE (catalog/style.json `voiceWave`) on a child laid out
    /// at its own size: drawn scaled by `scale` (the core's `child.scale` =
    /// min(1, childSize / its longer edge), never up) about `anchor` (the
    /// wave box's centre in the child's own fractions; the centre for a
    /// centred child). At scale 0 (`sm`) it stays in the tree but paints
    /// nothing, takes no hits and is hidden from VoiceOver.
    func voiceWaveChild(scale: CGFloat, anchor: UnitPoint = .center) -> some View {
        let s = min(1, max(0, scale.isFinite ? scale : 0))
        return self
            .scaleEffect(s > 0 ? s : 1, anchor: anchor)
            .opacity(s > 0 ? 1 : 0)
            .allowsHitTesting(s > 0)
            .accessibilityHidden(s <= 0)
    }

    /// `voiceWaveChild(scale:anchor:)` for a child at `frame` in a wave
    /// whose box centre is `centre` (both in the wave's coordinates).
    func voiceWaveChild(scale: CGFloat, frame: CGRect, centre: CGPoint) -> some View {
        voiceWaveChild(scale: scale, anchor: UnitPoint(
            x: frame.width > 0 ? (centre.x - frame.minX) / frame.width : 0.5,
            y: frame.height > 0 ? (centre.y - frame.minY) / frame.height : 0.5
        ))
    }
}
