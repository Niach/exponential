import SwiftUI
import ExponentialUICore

/// EXP-1604: a loaded Motion: the core's player (`MotionPlayer` over the
/// FFI: the shared clock + the packed display list) and its decoded images.
final class MotionSource: @unchecked Sendable {
    let player: MotionPlayer
    let size: CGSize
    let images: [CGImage?]

    init(player: MotionPlayer) {
        self.player = player
        let s = player.size()
        size = CGSize(width: CGFloat(s.first ?? 0), height: CGFloat(s.count > 1 ? s[1] : 0))
        images = player.images().map(MotionPainter.decodeDataURL)
    }

    /// The props that make the document (contract §1: `layers` + these).
    static let documentKeys = ["size", "fps", "duration", "start", "time", "layers", "assets", "markers"]
    /// The playback props (`set_playback`).
    static let playbackKeys = ["autoplay", "loop", "plays", "direction", "speed", "segment", "marker", "poster"]

    /// Lottie JSON from `layers` (compiled) or `lottie`; nil for `src` or nothing.
    static func inlineLottie(_ props: Props) throws -> String? {
        if props["layers"]?.array != nil {
            var doc: [String: JSONValue] = [:]
            for k in documentKeys { if let v = props[k] { doc[k] = v } }
            return try motionCompile(dialectJson: JSONValue.object(doc).json)
        }
        if let l = props["lottie"], l.object != nil { return l.json }
        return nil
    }

    /// The slot overrides (contract §2): every `$color.<token>` slot of the
    /// file gets the theme's colour, `$mode` the mode, the node's `slots` last.
    static func slots(lottie: String, props: Props, color: (String) -> String?, dark: Bool) -> String {
        var out: [String: JSONValue] = [:]
        for sid in (JSONValue.parse(lottie)["slots"]?.object ?? [:]).keys where sid.hasPrefix("$color.") {
            guard let hex = color(String(sid.dropFirst("$color.".count))), let c = rgba(hex) else { continue }
            out[sid] = .object(["p": .object(["a": .number(0), "k": .array(c.map(JSONValue.number))])])
        }
        if dark { out["$mode"] = .string("dark") }
        for (k, v) in props["slots"]?.object ?? [:] { out[k] = v }
        return JSONValue.object(out).json
    }

    /// `#rgb`, `#rrggbb`, `#rrggbbaa` → [r, g, b, a] in 0…1.
    static func rgba(_ hex: String) -> [Double]? {
        var h = hex.trimmingCharacters(in: .whitespaces)
        guard h.hasPrefix("#") else { return nil }
        h.removeFirst()
        if h.count == 3 || h.count == 4 { h = h.map { "\($0)\($0)" }.joined() }
        guard h.count == 6 || h.count == 8, let v = UInt64(h, radix: 16) else { return nil }
        let n = h.count == 8 ? v : (v << 8) | 0xff
        return [Double((n >> 24) & 0xff) / 255, Double((n >> 16) & 0xff) / 255, Double((n >> 8) & 0xff) / 255, Double(n & 0xff) / 255]
    }

    static func playbackJson(_ props: Props) -> String {
        var p: [String: JSONValue] = [:]
        for k in playbackKeys { if let v = props[k] { p[k] = v } }
        return JSONValue.object(p).json
    }
}

/// EXP-1604: `Motion` (contract §3–6): a `Canvas` under
/// `TimelineView(.animation)` (≤ 60 fps) painting the core's display list
/// for the clock's frame; the poster under reduced motion, `autoplay:
/// false`, off-screen, in the background and past the surface's first 8
/// animating Motions; a bound `progress` scrubs; `play: false` freezes.
struct MotionLeaf: View {
    let cx: LeafContext
    @Environment(\.xuiReducedMotion) private var surfaceReduced
    @Environment(\.scenePhase) private var scenePhase
    @State private var source: MotionSource?
    @State private var sourceKey: String?
    @State private var visible = false
    @State private var done = false
    @State private var clock = MotionClock()

    /// The source identity: what is loaded, in which theme and mode.
    private var key: String {
        var p: [String: JSONValue] = [:]
        for k in MotionSource.documentKeys + ["lottie", "src", "slots"] { if let v = cx.props[k] { p[k] = v } }
        return "\(JSONValue.object(p).json)|\(cx.dark)|\(cx.model.theme.map { ObjectIdentifier($0).hashValue } ?? 0)"
    }

    var body: some View {
        let props = cx.props
        let progress = props.num("progress")
        let play = props["play"]?.bool ?? true
        let still = surfaceReduced || cx.model.reducedMotion || props["autoplay"]?.bool == false || !visible
            || scenePhase == .background || !cx.model.motionMayAnimate(cx.index)
        let ticking = source != nil && progress == nil && !still && play && !done
        let fit = props.str("fit")
        let size = cx.size
        let key = key
        TimelineView(.animation(minimumInterval: 1.0 / 60.0, paused: !ticking)) { tl in
            Canvas { ctx, box in
                guard let source, sourceKey == key else { return }
                let frame: Double
                if let progress {
                    frame = source.player.progressFrame(progress: min(1, max(0, progress)))
                } else if still {
                    frame = source.player.posterFrame()
                } else {
                    let t = source.player.tick(elapsedMs: clock.elapsed(at: tl.date, running: ticking))
                    if t.done { DispatchQueue.main.async { done = true } }
                    frame = t.frame
                }
                guard let list = MotionList(packed: source.player.sample(frame: frame)) else { return }
                MotionPainter.paint(list, in: &ctx, box: box, fit: fit.isEmpty ? "contain" : fit, images: source.images)
            }
        }
        .frame(width: size.width, height: size.height)
        .onAppear { visible = true }
        .onDisappear {
            visible = false
            clock.reset()
        }
        .onChange(of: play) { _, now in if !now { clock.pause(at: Date()) } }
        .onChange(of: still) { _, now in if now { clock.reset(); done = false } }
        .task(id: key) { await load(key) }
    }

    @MainActor
    private func load(_ key: String) async {
        let props = cx.props
        clock.reset()
        done = false
        do {
            var lottie = try MotionSource.inlineLottie(props)
            if lottie == nil, case let src = props.str("src"), !src.isEmpty, let request = cx.model.mediaRequest(src) {
                lottie = try await Self.fetch(request)
            }
            guard let lottie else {
                source = nil
                sourceKey = key
                return
            }
            let theme = cx.model.theme
            let mode = cx.model.mode
            let slots = MotionSource.slots(lottie: lottie, props: props, color: { theme?.colorHex($0, mode: mode) }, dark: cx.dark)
            let player = try MotionPlayer(lottieJson: lottie, slotsJson: slots)
            player.setPlayback(playbackJson: MotionSource.playbackJson(props))
            source = MotionSource(player: player)
        } catch {
            // A load failure paints the box empty (contract §1), never a crash.
            source = nil
        }
        sourceKey = key
    }

    /// An attached `.json` or `.lottie` (the first animation), ≤ 2 MB.
    static func fetch(_ request: URLRequest) async throws -> String? {
        let (data, _) = try await URLSession.shared.data(for: request)
        guard data.count <= 2 * 1024 * 1024 else { return nil }
        if data.starts(with: [0x50, 0x4b]) { return try motionReadDotlottie(data: data) }
        return String(data: data, encoding: .utf8)
    }
}

/// The ms since play started, frozen while paused (`play: false`).
final class MotionClock {
    private var started: Date?
    private var banked: Double = 0

    func elapsed(at now: Date, running: Bool) -> Double {
        if running, started == nil { started = now }
        return banked + (started.map { now.timeIntervalSince($0) * 1000 } ?? 0)
    }

    func pause(at now: Date) {
        if let s = started { banked += now.timeIntervalSince(s) * 1000 }
        started = nil
    }

    func reset() {
        started = nil
        banked = 0
    }
}

extension SurfaceModel {
    /// At most 8 Motions animate per surface, in tree order (contract §3):
    /// the ones that would play (no `progress`, `autoplay` not false).
    func motionMayAnimate(_ index: Int) -> Bool {
        var before = 0
        for n in nodes where n.component == "Motion" && n.index < index {
            if n.props["progress"]?.number == nil, n.props["autoplay"]?.bool != false { before += 1 }
            if before >= 8 { return false }
        }
        return true
    }
}
