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
/// EXP-1605: a state machine (`MotionDriver`) takes the pointer and drives
/// the playback; a pressable Motion is the node's Button (NodeView), whose
/// press / pointer down / up reach the machine through the model.
struct MotionLeaf: View {
    let cx: LeafContext
    @Environment(\.xuiReducedMotion) private var surfaceReduced
    @Environment(\.scenePhase) private var scenePhase
    @State private var source: MotionSource?
    @State private var sourceKey: String?
    @State private var visible = false
    @State private var driver: MotionDriver

    init(cx: LeafContext) {
        self.cx = cx
        _driver = State(initialValue: MotionDriver(id: cx.node.id))
    }

    /// The source identity: what is loaded, in which theme and mode.
    private var key: String {
        var p: [String: JSONValue] = [:]
        for k in MotionSource.documentKeys + ["lottie", "src", "slots", "stateMachine"] { if let v = cx.props[k] { p[k] = v } }
        return "\(JSONValue.object(p).json)|\(cx.dark)|\(cx.model.theme.map { ObjectIdentifier($0).hashValue } ?? 0)"
    }

    var body: some View {
        let props = cx.props
        let progress = props.num("progress")
        let play = props["play"]?.bool ?? true
        // A running machine's state owns autoplay (the clock holds its poster).
        let still = surfaceReduced || cx.model.reducedMotion || (!driver.running && props["autoplay"]?.bool == false) || !visible
            || scenePhase == .background || !cx.model.motionMayAnimate(cx.index)
        let ticking = source != nil && progress == nil && !still && play && !driver.done
        let generation = driver.generation
        let driver = driver
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
                    let t = source.player.tick(elapsedMs: driver.clock.elapsed(at: tl.date, running: ticking))
                    if t.done { DispatchQueue.main.async { driver.finished(generation) } }
                    frame = t.frame
                }
                guard let list = MotionList(packed: source.player.sample(frame: frame)) else { return }
                MotionPainter.paint(list, in: &ctx, box: box, fit: fit.isEmpty ? "contain" : fit, images: source.images)
            }
        }
        .frame(width: size.width, height: size.height)
        .contentShape(Rectangle())
        .modifier(MotionPointer(driver: driver, on: driver.running && !cx.node.pressable, size: size))
        .onHover { driver.hover($0) }
        .onAppear {
            visible = true
            driver.attach(cx.model)
            driver.appeared()
        }
        .onDisappear {
            visible = false
            driver.clock.reset()
            driver.detach()
        }
        .onChange(of: play) { _, now in if !now { driver.clock.pause(at: Date()) } }
        .onChange(of: still) { _, now in if now { driver.rewind() } }
        .onChange(of: props["inputs"]) { _, now in driver.setInputs(now) }
        .task(id: key) { await load(key) }
    }

    @MainActor
    private func load(_ key: String) async {
        let props = cx.props
        driver.attach(cx.model)
        driver.rewind()
        do {
            var lottie = try MotionSource.inlineLottie(props)
            // README › Interactivity §1: `stateMachine`, else the .lottie's first.
            var machine = props["stateMachine"].flatMap { $0.object != nil ? $0.json : nil }
            if lottie == nil, case let src = props.str("src"), !src.isEmpty, let request = cx.model.mediaRequest(src) {
                let file = try await Self.fetch(request)
                lottie = file?.lottie
                machine = machine ?? file?.machine
            }
            guard let lottie else {
                source = nil
                sourceKey = key
                driver.start(nil, machine: nil)
                return
            }
            let theme = cx.model.theme
            let mode = cx.model.mode
            let slots = MotionSource.slots(lottie: lottie, props: props, color: { theme?.colorHex($0, mode: mode) }, dark: cx.dark)
            let player = try MotionPlayer(lottieJson: lottie, slotsJson: slots)
            player.setPlayback(playbackJson: MotionSource.playbackJson(props))
            source = MotionSource(player: player)
            driver.start(player, machine: machine)
            driver.setInputs(props["inputs"])
        } catch {
            // A load failure paints the box empty (contract §1), never a crash.
            source = nil
            driver.start(nil, machine: nil)
        }
        sourceKey = key
    }

    /// An attached `.json` or `.lottie` (the first animation and state
    /// machine), ≤ 2 MB.
    static func fetch(_ request: URLRequest) async throws -> (lottie: String, machine: String?)? {
        let (data, _) = try await URLSession.shared.data(for: request)
        guard data.count <= 2 * 1024 * 1024 else { return nil }
        if data.starts(with: [0x50, 0x4b]) {
            let machine = try motionReadDotlottieMachine(data: data)
            return (try motionReadDotlottie(data: data), machine.isEmpty ? nil : machine)
        }
        return String(data: data, encoding: .utf8).map { ($0, nil) }
    }
}

/// EXP-1605 (eui-motion README › Interactivity): a Motion's state machine
/// on the core's interpreter. Interactions go in (`sm_post`), inputs on
/// change (`sm_set_inputs`); every state change restarts the clock at 0;
/// `on.state` per entered state, `on.complete` once per state (its play
/// finished or it is final), `on.hover`, `on.visible` once.
@MainActor
@Observable
final class MotionDriver {
    @ObservationIgnored let id: String
    @ObservationIgnored weak var model: SurfaceModel?
    @ObservationIgnored private(set) var player: MotionPlayer?
    @ObservationIgnored let clock = MotionClock()
    @ObservationIgnored private var inputs: String?
    @ObservationIgnored private var machine: String?
    /// No state change since the machine started (a reload of it stays quiet).
    @ObservationIgnored private var pristine = false
    @ObservationIgnored private var completeSent = false
    @ObservationIgnored private var seen = false
    @ObservationIgnored var pointerDown = false
    /// A state machine drives the playback.
    private(set) var running = false
    /// The play finished: the last frame holds until the clock restarts.
    private(set) var done = false
    /// Bumped on every restart (a stale `done` report is dropped).
    private(set) var generation = 0

    init(id: String) { self.id = id }

    func attach(_ model: SurfaceModel) {
        self.model = model
        model.motionDrivers[id] = self
    }

    func detach() {
        if model?.motionDrivers[id] === self { model?.motionDrivers[id] = nil }
    }

    /// A loaded player (nil = nothing) and its machine (nil = plain playback).
    func start(_ player: MotionPlayer?, machine: String?) {
        // A reload (theme, mode) of an untouched machine does not announce its initial state again.
        let quiet = pristine && machine != nil && machine == self.machine
        self.player = player
        running = player.flatMap { p in machine.map { p.setStateMachine(json: $0) } } ?? false
        if player != nil { self.machine = machine }
        pristine = running
        inputs = nil
        rewind()
        drain(quiet: quiet)
    }

    /// An interaction (click, pointerDown, pointerUp, pointerEnter, pointerExit, complete).
    func post(_ kind: String) {
        guard running, let player else { return }
        if player.smPost(kind: kind, layer: "") {
            pristine = false
            rewind()
        }
        drain()
    }

    /// The `inputs` prop (literal or bound), written on change.
    func setInputs(_ value: JSONValue?) {
        guard running, let player, let value, value.object != nil else { return }
        let json = value.json
        guard json != inputs else { return }
        inputs = json
        if player.smSetInputs(json: json) {
            pristine = false
            rewind()
        }
        drain()
    }

    /// The clock reported `done` for the play of `generation`.
    func finished(_ generation: Int) {
        guard generation == self.generation, !done else { return }
        done = true
        if !completeSent {
            completeSent = true
            fire("complete")
        }
        post("complete")
    }

    func hover(_ inside: Bool) {
        post(inside ? "pointerEnter" : "pointerExit")
        if inside { fire("hover") }
    }

    /// First on screen: `on.visible`, once.
    func appeared() {
        guard !seen else { return }
        seen = true
        fire("visible")
    }

    /// Restart the clock at 0.
    func rewind() {
        clock.reset()
        done = false
        completeSent = false
        generation += 1
    }

    /// The states entered since the last read, and a final state.
    private func drain(quiet: Bool = false) {
        guard running, let player else { return }
        let status = JSONValue.parse(player.smStatus())
        for s in quiet ? [] : status["entered"]?.array ?? [] {
            if let name = s.string { fire("state", .object(["state": .string(name)])) }
        }
        if status["final"]?.bool == true, !completeSent {
            completeSent = true
            fire("complete")
        }
    }

    /// The node's `on.<event>` through the core (nothing without a handler).
    private func fire(_ event: String, _ payload: JSONValue? = nil) {
        guard let model, let i = model.byId[id],
              let events = try? model.surface.event(index: UInt32(i), name: event, payloadJson: payload?.json), !events.isEmpty else { return }
        model.dispatch(events)
    }
}

/// A Motion whose machine listens without `on.press` (no Button around it):
/// pointer down / up, and a click when released inside.
private struct MotionPointer: ViewModifier {
    let driver: MotionDriver
    let on: Bool
    let size: CGSize

    func body(content: Content) -> some View {
        if on {
            content.gesture(DragGesture(minimumDistance: 0)
                .onChanged { _ in
                    guard !driver.pointerDown else { return }
                    driver.pointerDown = true
                    driver.post("pointerDown")
                }
                .onEnded { e in
                    driver.pointerDown = false
                    driver.post("pointerUp")
                    if CGRect(origin: .zero, size: size).contains(e.location) { driver.post("click") }
                })
        } else {
            content
        }
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
