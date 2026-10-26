import SwiftUI
import ExponentialUICore
import ExponentialUIPrimitives

/// NEW-77: one frame of the `VoiceWave` native (catalog/style.json
/// `voiceWave`), computed by the core (`voiceWaveJson`): per ring (outer,
/// mid, inner) its points in px from the box's top-left, alphas and stroke
/// width; the centre child's edge (0 = not painted), the phase and, when
/// the input gave the child's size, its `child` box.
public struct VoiceWaveFrame: Equatable, Sendable {
    public struct Ring: Equatable, Sendable {
        public var part: String
        public var points: [CGPoint]
        public var fillAlpha: Double
        public var strokeAlpha: Double
        public var strokeWidth: CGFloat
    }

    /// THE CHILD RULE's box (`voiceWaveChildBox`): where the child is drawn
    /// in the wave's box (pt from its top-left) and the scale about the box
    /// centre (min(1, childSize / its longer edge), never up; 0 = kept in
    /// the tree, not painted).
    public struct ChildBox: Equatable, Sendable {
        public var rect: CGRect
        public var scale: CGFloat
    }

    public var rings: [Ring]
    public var childSize: CGFloat
    public var phase: Double
    public var child: ChildBox?

    /// The activities that animate (the phase advances with time).
    static let animated: Set<String> = ["listening", "thinking", "speaking"]

    /// The catalog's activities (`voiceWaveActivity`).
    static let activities: Set<String> = ["idle", "listening", "thinking", "speaking", "muted"]

    /// The `activity` prop as the core takes it: an unknown or missing
    /// value falls back to `idle`.
    static func activity(_ raw: String) -> String { activities.contains(raw) ? raw : "idle" }

    /// The frame for a `size` px box at `elapsed` seconds.
    /// `child` = the child's own laid-out size (the core then answers its box).
    public static func compute(size: CGFloat, activity: String, level: Double, bands: [Double]?, elapsed: Double, reducedMotion: Bool, child: CGSize? = nil) -> VoiceWaveFrame? {
        var input: [String: JSONValue] = [
            "size": .number(Double(size)),
            "activity": .string(activity),
            "level": .number(level.isFinite ? level : 0),
            "elapsed": .number(elapsed.isFinite ? elapsed : 0),
            "reducedMotion": .bool(reducedMotion),
        ]
        if let bands { input["bands"] = .array(bands.map { .number($0.isFinite ? $0 : 0) }) }
        if let child { input["child"] = .object(["width": .number(Double(child.width)), "height": .number(Double(child.height))]) }
        return parse(voiceWaveJson(inputJson: JSONValue.object(input).json))
    }

    static func parse(_ json: String) -> VoiceWaveFrame? {
        let v = JSONValue.parse(json)
        guard let rings = v["rings"]?.array else { return nil }
        return VoiceWaveFrame(
            rings: rings.map { r in
                Ring(
                    part: r["part"]?.string ?? "",
                    points: (r["points"]?.array ?? []).map { p in CGPoint(x: p[0]?.number ?? 0, y: p[1]?.number ?? 0) },
                    fillAlpha: r["fillAlpha"]?.number ?? 0,
                    strokeAlpha: r["strokeAlpha"]?.number ?? 0,
                    strokeWidth: CGFloat(r["strokeWidth"]?.number ?? 1)
                )
            },
            childSize: CGFloat(v["childSize"]?.number ?? 0),
            phase: v["phase"]?.number ?? 0,
            child: v["child"].map { c in
                ChildBox(
                    rect: CGRect(x: c["x"]?.number ?? 0, y: c["y"]?.number ?? 0, width: c["width"]?.number ?? 0, height: c["height"]?.number ?? 0),
                    scale: CGFloat(c["scale"]?.number ?? 0)
                )
            }
        )
    }

    /// The core's child box for a child of `child` pt in a wave of edge `size`.
    public static func childBox(size: CGFloat, child: CGSize) -> ChildBox? {
        compute(size: size, activity: "idle", level: 0, bands: nil, elapsed: 0, reducedMotion: true, child: child)?.child
    }
}

private extension JSONValue {
    subscript(_ i: Int) -> JSONValue? {
        guard let a = array, i >= 0, i < a.count else { return nil }
        return a[i]
    }
}

/// `VoiceWave`: the rings behind the one child, coloured by the recipe
/// parts (`outer` / `mid` / `inner`); the root recipe's opacity applies to
/// the node. Animates while listening, thinking or speaking (the seconds
/// since it appeared); reduced motion draws the rest phase.
struct VoiceWaveLeaf: View {
    let cx: LeafContext
    @Environment(\.xuiReducedMotion) private var surfaceReduced
    @Environment(\.accessibilityReduceMotion) private var platformReduced
    @State private var start = Date()

    var body: some View {
        let activity = VoiceWaveFrame.activity(cx.props.str("activity"))
        let level = cx.props.num("level") ?? 0
        let bands = cx.props["bands"]?.array.map { $0.compactMap { $0.number } }
        let reduced = surfaceReduced || platformReduced
        let edge = min(cx.size.width, cx.size.height)
        let colors = ["outer", "mid", "inner"].reduce(into: [String: Color]()) { $0[$1] = cx.part("VoiceWave", $1).color ?? cx.ink }
        let animates = VoiceWaveFrame.animated.contains(activity) && !reduced
        TimelineView(.animation(paused: !animates)) { tl in
            let elapsed = animates ? tl.date.timeIntervalSince(start) : 0
            let frame = VoiceWaveFrame.compute(size: edge, activity: activity, level: level, bands: bands, elapsed: elapsed, reducedMotion: reduced)
            VoiceWaveView(rings: (frame?.rings ?? []).map { r in
                VoiceWaveView.Ring(points: r.points, color: colors[r.part] ?? cx.ink, fillAlpha: r.fillAlpha, strokeAlpha: r.strokeAlpha, strokeWidth: r.strokeWidth)
            })
            .frame(width: edge, height: edge)
        }
        .frame(width: cx.size.width, height: cx.size.height)
    }
}
