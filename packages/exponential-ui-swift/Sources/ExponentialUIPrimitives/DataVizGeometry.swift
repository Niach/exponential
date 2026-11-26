import Foundation

/// Group D (genUI round 4): the Ring arcs (`sweep`, `rings`) and the
/// AudioPlayer waveform (`peaks`), the SDK's src/data-viz.ts;
/// `DataVizGeometryTests` replays fixtures/data-viz.json through it.
public enum DataViz {
    /// The waveform's locked bar geometry (px).
    public static let barWidth: Double = 2
    public static let gap: Double = 2
    public static let minHeight: Double = 2
    public static let height: Double = 24

    /// One ring as authored (a non-number value or max = nil).
    public struct RingSpec: Equatable {
        public let value: Double?
        public let max: Double?
        public let tone: String?
        public init(value: Double?, max: Double?, tone: String?) {
            self.value = value
            self.max = max
            self.tone = tone
        }
    }

    /// One drawn ring: centre-line radius, filled fraction, tone.
    public struct RingArc: Equatable {
        public let r: Double
        public let fraction: Double
        public let tone: String
        public init(r: Double, fraction: Double, tone: String) {
            self.r = r
            self.fraction = fraction
            self.tone = tone
        }
    }

    /// `start` / `sweep` in degrees clockwise from 12 o'clock.
    public struct RingArcs: Equatable {
        public let start: Double
        public let sweep: Double
        public let rings: [RingArc]
        public init(start: Double, sweep: Double, rings: [RingArc]) {
            self.start = start
            self.sweep = sweep
            self.rings = rings
        }
    }

    public struct WaveBar: Equatable {
        public let x: Double
        public let y: Double
        public let width: Double
        public let height: Double
        public let played: Bool
        public init(x: Double, y: Double, width: Double, height: Double, played: Bool) {
            self.x = x
            self.y = y
            self.width = width
            self.height = height
            self.played = played
        }
    }

    private static func jsRound(_ n: Double) -> Double { (n + 0.5).rounded(.down) }
    private static func round2(_ n: Double) -> Double { jsRound(n * 100) / 100 }
    private static func finite(_ v: Double?, _ d: Double) -> Double { v.flatMap { $0.isFinite ? $0 : nil } ?? d }
    private static func clamp01(_ n: Double) -> Double { Swift.max(0, Swift.min(1, n)) }

    public static func ringArcs(size: Double, stroke: Double, value: Double?, tone: String?, sweep: Double?, rings: [RingSpec]?) -> RingArcs {
        let s = Swift.max(0, Swift.min(360, finite(sweep, 360)))
        let start = s >= 360 ? 0 : 180 + (360 - s) / 2
        let baseTone = (tone?.isEmpty == false ? tone : nil) ?? "primary"
        let list = (rings?.isEmpty == false ? rings : nil) ?? [RingSpec(value: value, max: 1, tone: nil)]
        var out: [RingArc] = []
        for (i, ring) in list.enumerated() {
            let r = size / 2 - stroke / 2 - Double(i) * stroke * 2
            if r < stroke { continue }
            let m = finite(ring.max, 1)
            let fraction = m > 0 ? clamp01(finite(ring.value, 0) / m) : 0
            out.append(RingArc(r: round2(r), fraction: jsRound(fraction * 10000) / 10000, tone: (ring.tone?.isEmpty == false ? ring.tone : nil) ?? baseTone))
        }
        return RingArcs(start: round2(start), sweep: round2(s), rings: out)
    }

    public static func waveformBars(width: Double, peaks: [Double?], progress: Double?) -> [WaveBar] {
        let p = peaks.map { clamp01(finite($0, 0)) }
        if p.isEmpty { return [] }
        let n = Int(Swift.max(0, ((width + gap) / (barWidth + gap)).rounded(.down)))
        if n == 0 { return [] }
        let x0 = (width - (Double(n) * (barWidth + gap) - gap)) / 2
        let at = clamp01(finite(progress, 0)) * width
        return (0..<n).map { i in
            let peak: Double
            if p.count > n {
                let from = i * p.count / n
                let to = Swift.max(from + 1, (i + 1) * p.count / n)
                peak = p[from..<to].max() ?? 0
            } else {
                peak = p[i * p.count / n]
            }
            let h = Swift.max(minHeight, peak * height)
            let x = x0 + Double(i) * (barWidth + gap)
            return WaveBar(x: round2(x), y: round2((height - h) / 2), width: barWidth, height: round2(h), played: at > 0 && x + barWidth / 2 <= at)
        }
    }

    /// A press or drag at `x` on a `width` track → the seek fraction 0...1.
    public static func seekFraction(x: Double, width: Double) -> Double { width > 0 ? clamp01(x / width) : 0 }
}
