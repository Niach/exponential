import Foundation
import CoreGraphics

/// VAPP-160: the `VoiceWave` rings in pure Swift (src/voice-wave.ts
/// `voiceWave`, the Rust core's `voice_wave`), so the painter draws a frame
/// without a JSON round trip through the core. One value per box edge holds
/// what only the edge decides (per ring the base radius, the stroke width,
/// per point cos t / sin t and every harmonic's h × t); `frame(...)` adds the
/// per-frame part (activity, level, bands, phase). The constants are
/// catalog/style.json `voiceWave` (gated by `VoiceWaveGeometryTests`) and
/// fixtures/voice-wave.json `cases` replays through it.
public struct VoiceWaveGeometry: Sendable {
    /// catalog/style.json `voiceWave` (the rings' constants).
    public enum Constants {
        public static let steps = 120
        public static let radius: [Double] = [0.33, 0.3, 0.26]
        public static let harmonics: [[Double]] = [[11, 17, 23], [9, 15, 21], [7, 13]]
        public static let phase0: [Double] = [0.4, 1.9, 3.1]
        public static let ringAmp: [Double] = [1, 0.85, 0.45]
        public static let ampBase: Double = 2
        public static let ampLevel: Double = 6
        public static let thinkingAmp: Double = 2.5
        public static let phaseSpeed: Double = 0.9
        public static let fillAlpha: [Double] = [0.1, 0.1, 0.15]
        public static let strokeAlpha: [Double] = [0.7, 0.8, 0.4]
        public static let strokeWidth: [Double] = [1.5, 1.5, 1]
        public static let childScale: Double = 0.3
        public static let childScaleLarge: Double = 0.22
        public static let maxBands = 8
    }

    /// The ring parts, outer first (the recipe parts that colour them).
    public static let parts = ["outer", "mid", "inner"]

    /// The catalog's activities (`voiceWaveActivity`).
    public static let activities: Set<String> = ["idle", "listening", "thinking", "speaking", "muted"]

    /// The activities whose phase advances with time.
    public static let animated: Set<String> = ["listening", "thinking", "speaking"]

    public struct Ring: Equatable, Sendable {
        public var part: String
        /// `steps + 1` points in px from the box's top-left (the last repeats the first).
        public var points: [CGPoint]
        public var fillAlpha: Double
        public var strokeAlpha: Double
        public var strokeWidth: CGFloat
    }

    public struct Frame: Equatable, Sendable {
        public var rings: [Ring]
        /// The phase the rings were drawn at (radians, rounded to 0.01).
        public var phase: Double
    }

    /// What one ring keeps per edge.
    struct RingTable: Sendable {
        /// size × radius[k].
        var base: Double
        /// ringAmp[k] × size (× drive / 200 per frame, in the core's order).
        var ampSize: Double
        /// Per harmonic j, per point i: harmonics[k][j] × t_i.
        var ht: [[Double]]
        var strokeWidth: CGFloat
    }

    /// The box edge (px) these tables are for.
    public let size: Double
    let centre: Double
    let cosT: [Double]
    let sinT: [Double]
    let tables: [RingTable]

    public init(size: CGFloat) {
        let s = Double(size)
        let edge = s.isFinite ? max(0, s) : 0
        self.size = edge
        centre = edge / 2
        let n = Constants.steps
        let t = (0...n).map { i in (Double(i) / Double(n)) * Double.pi * 2 }
        cosT = t.map { cos($0) }
        sinT = t.map { sin($0) }
        tables = (0..<Self.parts.count).map { k in
            RingTable(
                base: edge * Constants.radius[k],
                ampSize: Constants.ringAmp[k] * edge,
                ht: Constants.harmonics[k].map { h in t.map { h * $0 } },
                strokeWidth: CGFloat(Self.round2(max(1, (Constants.strokeWidth[k] * edge) / 200)))
            )
        }
    }

    /// The activity as the catalog names it; anything else is `idle`.
    public static func activity(_ raw: String) -> String { activities.contains(raw) ? raw : "idle" }

    /// The phase (radians) at `elapsed` seconds: 0 when idle, muted or under
    /// reduced motion (`voiceWavePhase`).
    public static func phase(activity raw: String, elapsed: Double, reducedMotion: Bool) -> Double {
        if reducedMotion || !animated.contains(activity(raw)) { return 0 }
        return (elapsed.isFinite ? max(0, elapsed) : 0) * Constants.phaseSpeed
    }

    /// JS `Math.round(n * 100) / 100`: halves round toward +∞ (Swift's
    /// `.rounded()` takes -0.125 to -0.13, the core to -0.12).
    public static func round2(_ n: Double) -> Double {
        let x = n * 100
        let f = x.rounded(.down)
        return (x - f >= 0.5 ? f + 1 : f) / 100
    }

    static func clamp01(_ n: Double) -> Double { n.isFinite ? min(1, max(0, n)) : 0 }

    /// The three rings at one frame.
    public func frame(activity raw: String, level rawLevel: Double, bands rawBands: [Double]?, elapsed: Double, reducedMotion: Bool) -> Frame {
        let activity = Self.activity(raw)
        let level = Self.clamp01(rawLevel)
        let bands = (rawBands ?? []).prefix(Constants.maxBands).map(Self.clamp01)
        let phase = Self.phase(activity: activity, elapsed: elapsed, reducedMotion: reducedMotion)
        let drive: Double
        switch activity {
        case "listening", "speaking": drive = Constants.ampBase + Constants.ampLevel * level
        case "thinking": drive = Constants.thinkingAmp
        default: drive = 0
        }
        let flat = bands.isEmpty || activity == "thinking"
        let c = centre
        var rings: [Ring] = []
        rings.reserveCapacity(tables.count)
        for (k, table) in tables.enumerated() {
            let amp = (table.ampSize * drive) / 200
            let count = Constants.harmonics[k].count
            // Per harmonic: amp / (j + 1) × band, and the phase offset.
            var weight = [Double](repeating: 0, count: count)
            var offset = [Double](repeating: 0, count: count)
            for j in 0..<count {
                let band = flat ? 1 : bands[(3 * k + j) % bands.count]
                weight[j] = (amp / Double(j + 1)) * band
                offset[j] = (Constants.phase0[k] + phase) * Double(j + 1)
            }
            var points = [CGPoint]()
            points.reserveCapacity(cosT.count)
            for i in 0..<cosT.count {
                var r = table.base
                if drive != 0 {
                    for j in 0..<count { r += weight[j] * sin(table.ht[j][i] + offset[j]) }
                }
                points.append(CGPoint(x: Self.round2(c + r * cosT[i]), y: Self.round2(c + r * sinT[i])))
            }
            rings.append(Ring(
                part: Self.parts[k],
                points: points,
                fillAlpha: Constants.fillAlpha[k],
                strokeAlpha: Constants.strokeAlpha[k],
                strokeWidth: table.strokeWidth
            ))
        }
        return Frame(rings: rings, phase: Self.round2(phase))
    }
}

/// The last edge's `VoiceWaveGeometry`: a painter keeps one (e.g. in
/// `@State`) and asks it per frame; the tables are rebuilt only when the
/// edge changes.
public final class VoiceWaveGeometryCache: @unchecked Sendable {
    private let lock = NSLock()
    private var last: VoiceWaveGeometry?

    public init() {}

    public func geometry(size: CGFloat) -> VoiceWaveGeometry {
        lock.lock()
        defer { lock.unlock() }
        let s = Double(size)
        let edge = s.isFinite ? max(0, s) : 0
        if let last, last.size == edge { return last }
        let g = VoiceWaveGeometry(size: size)
        last = g
        return g
    }
}
