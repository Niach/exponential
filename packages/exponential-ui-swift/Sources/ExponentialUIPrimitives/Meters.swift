import SwiftUI

/// One segment of a meter track.
public struct MeterSegment: Identifiable, Equatable, Sendable {
    public var id: Int
    public var fraction: Double
    public var color: Color

    public init(id: Int, fraction: Double, color: Color) {
        self.id = id
        self.fraction = fraction
        self.color = color
    }
}

/// A horizontal track of stacked segments (the catalog `Meter` and
/// `Progress`; the app's usage track). Fractions are of the whole width.
public struct MeterTrack: View {
    let segments: [MeterSegment]
    let height: CGFloat
    let track: Color
    let radius: CGFloat?
    let capsuleSegments: Bool
    let ticks: [Double]
    let tickColor: Color
    let tickWidth: CGFloat

    /// - `capsuleSegments`: each segment is its own capsule and nothing is
    ///   clipped (a single-value bar whose fill keeps round ends);
    /// - `ticks`: fractions of the width marked over the fill.
    public init(
        segments: [MeterSegment],
        height: CGFloat = 8,
        track: Color = Color.gray.opacity(0.2),
        radius: CGFloat? = nil,
        capsuleSegments: Bool = false,
        ticks: [Double] = [],
        tickColor: Color = Color.black.opacity(0.45),
        tickWidth: CGFloat = 1
    ) {
        self.segments = segments
        self.height = height
        self.track = track
        self.radius = radius
        self.capsuleSegments = capsuleSegments
        self.ticks = ticks
        self.tickColor = tickColor
        self.tickWidth = tickWidth
    }

    public var body: some View {
        if capsuleSegments {
            bar.frame(height: height).accessibilityHidden(true)
        } else {
            bar
                .frame(height: height)
                .clipShape(RoundedRectangle(cornerRadius: radius ?? height / 2))
                .accessibilityHidden(true)
        }
    }

    private var bar: some View {
        GeometryReader { proxy in
            let w = proxy.size.width
            ZStack(alignment: .leading) {
                Capsule().fill(track)
                if capsuleSegments {
                    HStack(spacing: 0) {
                        ForEach(segments) { s in
                            Capsule().fill(s.color).frame(width: Self.width(s.fraction, w))
                        }
                    }
                } else {
                    HStack(spacing: 0) {
                        ForEach(segments) { s in
                            Rectangle().fill(s.color).frame(width: Self.width(s.fraction, w))
                        }
                    }
                    .clipShape(RoundedRectangle(cornerRadius: radius ?? height / 2))
                }
                ForEach(Array(ticks.enumerated()), id: \.offset) { _, tick in
                    Rectangle()
                        .fill(tickColor)
                        .frame(width: tickWidth)
                        .offset(x: Self.width(tick, w))
                }
            }
        }
    }

    /// A fraction of `total`, clamped to the track.
    public static func width(_ fraction: Double, _ total: CGFloat) -> CGFloat {
        max(0, total * min(1, max(0, fraction)))
    }
}

/// A ring: a track circle and a value arc from 12 o'clock (the catalog
/// `Ring`; the app's context and progress rings).
public struct RingView: View {
    let value: Double
    let lineWidth: CGFloat
    let fillWidth: CGFloat?
    let track: Color
    let fill: Color
    let lineCap: CGLineCap

    public init(value: Double, lineWidth: CGFloat = 2, fillWidth: CGFloat? = nil, track: Color = Color.gray.opacity(0.2), fill: Color = .accentColor, lineCap: CGLineCap = .butt) {
        self.value = value
        self.lineWidth = lineWidth
        self.fillWidth = fillWidth
        self.track = track
        self.fill = fill
        self.lineCap = lineCap
    }

    public var body: some View {
        let fw = fillWidth ?? lineWidth
        let arc = value.isFinite ? min(1, max(0, value)) : 0
        ZStack {
            Circle().strokeBorder(track, lineWidth: lineWidth)
            // An empty arc draws nothing (a round cap would leave a dot).
            if arc > 0 {
                Circle()
                    .inset(by: fw / 2)
                    .trim(from: 0, to: arc)
                    .stroke(fill, style: StrokeStyle(lineWidth: fw, lineCap: lineCap))
                    .rotationEffect(.degrees(-90))
            }
        }
        .accessibilityHidden(true)
    }
}
