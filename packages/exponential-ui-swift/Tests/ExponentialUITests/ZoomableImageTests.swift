import XCTest
import SwiftUI
import ExponentialUICore
import ExponentialUIPrimitives
@testable import ExponentialUI

/// EXP-1586: the ZoomableImage native. `fixtures/zoom.json` replayed
/// through the facade (`zoomJson`) and through the typed `ZoomMaths.core`
/// the primitive calls; the `ZoomController` sequencing (steps, double tap,
/// pinch rubber band, the pan-room hand-off, resize, the pill); the
/// kitchen-sink node measured, painted, focusable, keyed and named.
@MainActor
final class ZoomableImageTests: XCTestCase {
    override func setUp() async throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
    }

    // MARK: - the fixture

    /// The fixture's groups with no frame.
    static let frameless: Set<String> = ["rubberBand", "momentumActive", "percent"]

    /// Numbers within 1e-6, everything else equal.
    static func near(_ a: JSONValue, _ b: JSONValue, _ path: String = "") -> String? {
        switch (a, b) {
        case let (.number(x), .number(y)):
            return abs(x - y) <= 1e-6 ? nil : "\(path): \(x) ≠ \(y)"
        case let (.array(x), .array(y)):
            guard x.count == y.count else { return "\(path): \(x.count) items ≠ \(y.count)" }
            for (i, (p, q)) in zip(x, y).enumerated() { if let d = near(p, q, "\(path)/\(i)") { return d } }
            return nil
        case let (.object(x), .object(y)):
            guard Set(x.keys) == Set(y.keys) else { return "\(path): keys \(x.keys.sorted()) ≠ \(y.keys.sorted())" }
            for (k, v) in x { if let d = near(v, y[k]!, "\(path)/\(k)") { return d } }
            return nil
        default:
            return a == b ? nil : "\(path): \(a) ≠ \(b)"
        }
    }

    /// The case's request: its fields (minus name / expected), the op, the
    /// named frame with the case's `viewport` override.
    static func request(_ group: String, _ c: JSONValue, frames: JSONValue) -> JSONValue {
        if group == "percent" { return .object(["op": .string("percent"), "scale": c.array![0]]) }
        var o = c.object ?? [:]
        o["name"] = nil
        o["expected"] = nil
        o["op"] = .string(group)
        if let name = o["frame"]?.string {
            var f = frames[name]?.object ?? [:]
            if let v = o["viewport"] { f["viewport"] = v }
            o["viewport"] = nil
            o["frame"] = .object(f)
        }
        return .object(o)
    }

    static func expected(_ group: String, _ c: JSONValue) -> JSONValue {
        group == "percent" ? c.array![1] : (c["expected"] ?? .null)
    }

    static func groups(_ f: JSONValue) -> [(String, [JSONValue])] {
        (f.object ?? [:]).filter { !$0.key.hasPrefix("$") && $0.key != "frames" }.compactMap { k, v in v.array.map { (k, $0) } }.sorted { $0.0 < $1.0 }
    }

    func testEveryZoomFixtureCaseReplaysThroughTheFacade() throws {
        let f = try Fixtures.json("zoom.json")
        let frames = f["frames"] ?? .object([:])
        var count = 0
        var failures: [String] = []
        let groups = Self.groups(f)
        XCTAssertEqual(Set(groups.map(\.0)), ["rest", "clamp", "zoomTo", "wheel", "scroll", "ladder", "steps", "doubleTap", "keys", "rubberBand", "drag", "pinch", "momentum", "momentumActive", "panRoom", "resize", "percent", "menu", "bounds"])
        for (group, cases) in groups {
            for c in cases {
                count += 1
                let got = JSONValue.parse(zoomJson(requestJson: Self.request(group, c, frames: frames).json))
                if let d = Self.near(got, Self.expected(group, c)) { failures.append("\(group) \(c["name"]?.string ?? c.json): \(d)") }
            }
        }
        XCTAssertEqual(count, 92)
        XCTAssertEqual(failures, [])
    }

    /// The typed calls the primitive makes land on the fixture's numbers too.
    func testTheTypedZoomMathsAgreeWithTheFixture() throws {
        let f = try Fixtures.json("zoom.json")
        let frames = f["frames"] ?? .object([:])
        let maths = ZoomMaths.core
        func size(_ v: JSONValue?) -> CGSize { CGSize(width: v?["width"]?.number ?? 0, height: v?["height"]?.number ?? 0) }
        func point(_ v: JSONValue?) -> CGPoint { CGPoint(x: v?["x"]?.number ?? 0, y: v?["y"]?.number ?? 0) }
        func frame(_ c: JSONValue) -> ZoomFrame {
            let named = frames[c["frame"]?.string ?? ""]
            return ZoomFrame(natural: size(named?["natural"]), viewport: size(c["viewport"] ?? named?["viewport"]), initial: ZoomInitial(prop: named?["initial"]?.string ?? ""), maxScale: named?["maxScale"]?.number)
        }
        func from(_ c: JSONValue, _ f: ZoomFrame) -> ZoomView {
            if let o = c["from"], o.object != nil { return ZoomView(scale: o["scale"]!.number!, x: o["x"]!.number!, y: o["y"]!.number!) }
            return maths.rest(f).view
        }
        func json(_ v: ZoomView) -> JSONValue { .object(["scale": .number(v.scale), "x": .number(v.x), "y": .number(v.y)]) }
        var failures: [String] = []
        func check(_ group: String, _ c: JSONValue, _ got: JSONValue) {
            if let d = Self.near(got, Self.expected(group, c)) { failures.append("\(group) \(c["name"]?.string ?? ""): \(d)") }
        }
        for (group, cases) in Self.groups(f) {
            for c in cases {
                let fr = Self.frameless.contains(group) ? ZoomFrame(natural: .zero, viewport: .zero) : frame(c)
                switch group {
                case "rest":
                    let r = maths.rest(fr)
                    check(group, c, .object(["fit": .number(r.fit), "limits": .object(["min": .number(r.min), "max": .number(r.max)]), "restScale": .number(r.restScale), "view": json(r.view), "tall": .bool(r.tall)]))
                case "clamp":
                    let v = c["view"]!
                    check(group, c, json(maths.clamp(ZoomView(scale: v["scale"]!.number!, x: v["x"]!.number!, y: v["y"]!.number!), fr)))
                case "zoomTo": check(group, c, json(maths.zoomTo(from(c, fr), scale: c["scale"]!.number!, anchor: point(c["anchor"]), fr)))
                case "wheel": check(group, c, json(maths.wheel(from(c, fr), deltaY: c["deltaY"]!.number!, lines: c["mode"]?.string == "line", anchor: point(c["anchor"]), fr)))
                case "scroll": check(group, c, json(maths.scroll(from(c, fr), deltaX: c["deltaX"]!.number!, deltaY: c["deltaY"]!.number!, fr)))
                case "ladder": check(group, c, .array(maths.ladder(fr).map { .number($0) }))
                case "steps":
                    // The step scale = the view `step` lands on (about the centre).
                    let start = maths.zoomTo(maths.rest(fr).view, scale: c["scale"]!.number!, anchor: CGPoint(x: fr.viewport.width / 2, y: fr.viewport.height / 2), fr)
                    if abs(start.scale - c["scale"]!.number!) < 1e-6 {
                        check(group, c, .number(maths.step(start, direction: Int(c["direction"]!.number!), fr).scale))
                    }
                case "doubleTap": check(group, c, json(maths.doubleTap(from(c, fr), at: point(c["point"]), fr)))
                case "keys": check(group, c, maths.key(from(c, fr), key: c["key"]!.string!, fr).map(json) ?? .null)
                case "rubberBand": check(group, c, .number(maths.rubberBand(overshoot: c["overshoot"]!.number!, extent: c["extent"]!.number!)))
                case "drag": check(group, c, json(maths.drag(from(c, fr), dx: c["dx"]!.number!, dy: c["dy"]!.number!, fr)))
                case "pinch": check(group, c, json(maths.pinch(from(c, fr), factor: c["factor"]!.number!, anchor: point(c["anchor"]), fr)))
                case "momentum": check(group, c, json(maths.momentum(from(c, fr), velocity: CGVector(dx: c["vx"]!.number!, dy: c["vy"]!.number!), t: c["t"]!.number!, fr)))
                case "momentumActive": check(group, c, .bool(maths.momentumActive(velocity: CGVector(dx: c["vx"]!.number!, dy: c["vy"]!.number!), t: c["t"]!.number!)))
                case "panRoom":
                    let r = maths.panRoom(from(c, fr), fr)
                    check(group, c, .object(["x": .bool(r.x), "y": .bool(r.y)]))
                case "resize":
                    let old = size(c["fromViewport"])
                    let start = c["from"]?.string == "rest" ? maths.rest(ZoomFrame(natural: fr.natural, viewport: old, initial: fr.initial, maxScale: fr.maxScale)).view : from(c, fr)
                    check(group, c, json(maths.resize(start, from: old, fr)))
                case "percent": check(group, c, .string(maths.percent(c.array![0].number!)))
                case "menu":
                    check(group, c, .array(maths.menu(from(c, fr), fr, fitLabel: "Fit").map { .object(["scale": $0.scale.map(JSONValue.number) ?? .null, "label": .string($0.label), "checked": .bool($0.checked)]) }))
                case "bounds":
                    let b = maths.bounds(from(c, fr), fr)
                    check(group, c, .object(["atMin": .bool(b.atMin), "atMax": .bool(b.atMax)]))
                default: failures.append("unknown group \(group)")
                }
            }
        }
        XCTAssertEqual(failures, [])
    }

    /// `ZoomPillMetrics` mirrors style.json (`zoomableImage.pill` + autoHideMs).
    func testThePillMetricsMirrorStyleJson() throws {
        let style = JSONValue.parse(try String(contentsOf: Fixtures.dir.deletingLastPathComponent().appendingPathComponent("catalog/style.json"), encoding: .utf8))
        let z = try XCTUnwrap(style["zoomableImage"])
        let p = try XCTUnwrap(z["pill"])
        let m = ZoomPillMetrics.standard
        XCTAssertEqual(Double(m.height), p["height"]?.number)
        XCTAssertEqual(Double(m.inset), p["inset"]?.number)
        XCTAssertEqual(Double(m.padding), p["padding"]?.number)
        XCTAssertEqual(Double(m.gap), p["gap"]?.number)
        XCTAssertEqual(Double(m.percentMinWidth), p["percentMinWidth"]?.number)
        XCTAssertEqual(Double(m.dividerHeight), p["dividerHeight"]?.number)
        XCTAssertEqual(m.autoHideMs, z["autoHideMs"]?.number)
        XCTAssertEqual(z["wheelLinePx"]?.number, 16, "ZoomController.wheel's line pan")
    }

    // MARK: - the controller

    private func controller(_ natural: CGSize, _ viewport: CGSize, initial: ZoomInitial = .auto, controls: ZoomControls = .always) -> ZoomController {
        let c = ZoomController(maths: .core)
        c.reducedMotion = true
        c.controls = controls
        c.configure(natural: natural, initial: initial, maxScale: nil)
        c.setViewport(viewport)
        return c
    }

    func testStepsFitDoubleTapAndKeysSettleAndReport() {
        let c = controller(CGSize(width: 2880, height: 1800), CGSize(width: 1000, height: 700))
        var events: [ZoomEvent] = []
        c.onZoom = { events.append($0) }
        XCTAssertEqual(c.view.scale, 0.347222, accuracy: 1e-6)
        XCTAssertEqual(c.view.y, 37.5, accuracy: 1e-6)
        XCTAssertTrue(c.atRest)
        XCTAssertEqual(c.percent, "35%")
        XCTAssertEqual(c.bounds, ZoomBounds(atMin: true, atMax: false))
        XCTAssertFalse(c.canPan, "at fit nothing pans: a swipe is the host's")
        c.zoomIn()
        XCTAssertEqual(c.view, ZoomView(scale: 0.5, x: -220.000461, y: -100.000288))
        XCTAssertEqual(events, [ZoomEvent(scale: 0.5, atFit: false)])
        XCTAssertTrue(c.canPan)
        c.fit()
        XCTAssertEqual(events.last, ZoomEvent(scale: 0.347222, atFit: true))
        c.doubleTap(at: CGPoint(x: 250, y: 200))
        XCTAssertEqual(c.view, ZoomView(scale: 1, x: -470.000461, y: -268.0003))
        c.doubleTap(at: CGPoint(x: 10, y: 10))
        XCTAssertTrue(c.atRest)
        XCTAssertTrue(c.key("1"))
        XCTAssertEqual(c.view, ZoomView(scale: 1, x: -940.000922, y: -550.000576))
        XCTAssertFalse(c.key("ArrowLeft"))
        XCTAssertTrue(c.key("0"))
        XCTAssertTrue(c.atRest)
        XCTAssertEqual(events.count, 6)
        // The same scale twice reports once.
        c.fit()
        XCTAssertEqual(events.count, 6)
        // The bound scale from outside: about the centre, reported.
        c.setScale(2)
        XCTAssertEqual(c.view.scale, 2)
        XCTAssertEqual(c.bounds.atMax, false)
    }

    func testTheMenuIsTheCoresRows() {
        let c = controller(CGSize(width: 2880, height: 1800), CGSize(width: 1000, height: 700))
        c.fitLabel = "Einpassen"
        XCTAssertEqual(c.menuItems.map(\.label), ["Einpassen", "50%", "100%", "200%", "400%"])
        XCTAssertEqual(c.menuItems.first?.checked, true)
        c.toggleMenu()
        XCTAssertTrue(c.menuOpen)
        c.select(c.menuItems[2])
        XCTAssertFalse(c.menuOpen)
        XCTAssertEqual(c.view.scale, 1)
        XCTAssertEqual(c.menuItems.map(\.checked), [false, false, true, false, false])
        c.toggleMenu()
        XCTAssertTrue(c.key("escape"))
        XCTAssertFalse(c.menuOpen)
        c.select(c.menuItems[0])
        XCTAssertTrue(c.atRest)
    }

    func testAPinchRubberBandsWhileHeldAndClampsOnRelease() {
        let c = controller(CGSize(width: 1440, height: 900), CGSize(width: 390, height: 460))
        let start = c.view
        c.pinchChanged(factor: 3, anchor: CGPoint(x: 195, y: 230))
        XCTAssertEqual(c.view, ZoomMaths.core.pinch(start, factor: 3, anchor: CGPoint(x: 195, y: 230), c.frame))
        c.pinchChanged(factor: 80, anchor: CGPoint(x: 195, y: 230))
        XCTAssertGreaterThan(c.view.scale, 8, "past the max it rubber-bands")
        c.pinchEnded()
        XCTAssertEqual(c.view.scale, 8)
        c.pinchChanged(factor: 0.001, anchor: .zero)
        XCTAssertLessThan(c.view.scale, c.rest.min)
        c.pinchEnded()
        XCTAssertEqual(c.view.scale, c.rest.min, accuracy: 1e-6)
    }

    /// A TALL picture opens at fit-width on its top: only y pans, a
    /// horizontal swipe is left to the host; a drag past the top springs back.
    func testATallPictureHandsHorizontalSwipesToTheHost() {
        let c = controller(CGSize(width: 1440, height: 9000), CGSize(width: 390, height: 460))
        XCTAssertTrue(c.rest.tall)
        XCTAssertEqual(c.view.y, 0)
        XCTAssertEqual(c.panRoom, ZoomPanRoom(x: false, y: true))
        XCTAssertTrue(c.canPan)
        let rest = c.view
        c.dragChanged(translation: CGSize(width: -120, height: 10))
        XCTAssertEqual(c.view, rest)
        c.dragEnded(velocity: CGSize(width: -900, height: 0))
        XCTAssertEqual(c.view, rest)
        c.dragChanged(translation: CGSize(width: 4, height: -300))
        XCTAssertEqual(c.view.x, rest.x, accuracy: 1e-3, "x has no room: the core keeps it centred")
        XCTAssertEqual(c.view.y, -300, accuracy: 1e-6)
        c.dragEnded(velocity: .zero)
        c.dragChanged(translation: CGSize(width: 0, height: 900))
        XCTAssertGreaterThan(c.view.y, 0, "past the top it rubber-bands")
        c.dragEnded(velocity: .zero)
        XCTAssertEqual(c.view.y, 0)
    }

    func testAResizeRefitsAtRestAndKeepsAZoomAboutTheCentre() {
        let c = controller(CGSize(width: 2880, height: 1800), CGSize(width: 1400, height: 900))
        c.setViewport(CGSize(width: 1000, height: 700))
        XCTAssertEqual(c.view, ZoomView(scale: 0.347222, x: 0, y: 37.5))
        c.setScale(1)
        c.setViewport(CGSize(width: 800, height: 600))
        XCTAssertEqual(c.view.scale, 1)
        c.configure(natural: CGSize(width: 400, height: 300), initial: .auto, maxScale: nil)
        XCTAssertEqual(c.view, ZoomView(scale: 1, x: 200, y: 150), "a new picture opens at rest")
    }

    func testThePillShowsTogglesAndHides() async throws {
        let c = controller(CGSize(width: 2880, height: 1800), CGSize(width: 1000, height: 700), controls: .auto)
        var taps = 0
        c.onTap = { taps += 1 }
        c.metrics.autoHideMs = 50
        c.appeared()
        XCTAssertTrue(c.pillShown)
        try await eventually { !c.pillShown }
        XCTAssertFalse(c.pillShown)
        c.tap()
        XCTAssertTrue(c.pillShown)
        XCTAssertEqual(taps, 1)
        c.tap()
        XCTAssertFalse(c.pillShown)
        let always = controller(CGSize(width: 10, height: 10), CGSize(width: 100, height: 100), controls: .always)
        always.appeared()
        always.tap()
        XCTAssertTrue(always.pillShown)
        let none = controller(CGSize(width: 10, height: 10), CGSize(width: 100, height: 100), controls: .none)
        none.appeared()
        XCTAssertFalse(none.pillShown)
    }

    // MARK: - the painter

    private func kitchenSinkNode() throws -> JSONValue {
        let ks = try Fixtures.json("kitchen-sink.json")
        return try XCTUnwrap(Round2Suites.find(ks["root"] ?? ks, "zoomable"))
    }

    func testTheKitchenSinkNodeMeasuresPaintsAndReads() throws {
        let node = try kitchenSinkNode()
        XCTAssertEqual(node["props"]?["controls"]?.string, "always")
        let host = RecordingHost()
        var options = SurfaceOptions()
        options.theme = ThemeHandle.builtin("neutral")
        options.mode = .dark
        let m = try SurfaceModel(id: "zoom", options: options, host: host)
        var tree = node.object ?? [:]
        tree["on"] = .object(["zoom": .object(["event": .object(["name": .string("zoomed")])]), "tap": .object(["event": .object(["name": .string("tapped")])])])
        try m.setNested(json: JSONValue.object(["id": .string("col"), "component": .string("Stack"), "children": .array([.object(tree)])]).json)
        m.setViewport(width: 390, height: 800)
        let i = try XCTUnwrap(m.index(of: "zoomable"))
        let n = try XCTUnwrap(m.node(i))
        // THE BOX: the parent's width, 1440 × 900 → 390 × 243.75.
        XCTAssertEqual(m.frame(i).width, 390, accuracy: 0.5)
        XCTAssertEqual(m.frame(i).height, 243.75, accuracy: 0.5)
        XCTAssertTrue(n.isFocusable)
        XCTAssertTrue(A11yInfo.of(n, model: m, style: m.style(i)).passthrough)
        // The recipes the pill paints with.
        let cx = LeafContext(model: m, node: n, size: m.frame(i).size, style: m.style(i), ink: m.ink(i), textStyle: m.textStyle(i))
        let style = ZoomableImageLeaf.pillStyle(cx)
        XCTAssertTrue(style.glass)
        XCTAssertEqual(style.button.height, 28)
        XCTAssertNotEqual(style.fit.selectedBackground, style.fit.background, "fit is drawn selected at rest")
        XCTAssertEqual(style.metrics.padding, 4)
        XCTAssertEqual(style.metrics.dividerHeight, 16)
        XCTAssertGreaterThan(style.menu.itemHeight, 0)
        XCTAssertGreaterThan(cx.part("ZoomableImage", "image", props: n.props).style.radius, 0)
        XCTAssertEqual(ZoomableImageLeaf.strings(m), ZoomStrings.english)
        XCTAssertEqual(BuiltinIcons.name("ZoomableImage.fit"), "ui-fullscreen")
        XCTAssertTrue(BuiltinIcons.isKnown("ui-fullscreen"))
        // Keys reach the painter through the focus routing.
        m.requestFocus("zoomable", keyboard: true)
        XCTAssertTrue(m.handleKey(key: "+", typed: "+"))
        XCTAssertEqual(m.zoomCommands[i], ZoomCommand(serial: 1, key: "+"))
        XCTAssertTrue(m.handleKey(key: "0", typed: "0"))
        XCTAssertEqual(m.zoomCommands[i]?.serial, 2)
        XCTAssertFalse(m.handleKey(key: "x", typed: "x"))
        // The events go out through the core to the `on` handlers.
        m.fire(i, "zoom", payload: .object(["scale": .number(1), "atFit": .bool(false)]))
        m.fire(i, "tap")
        XCTAssertEqual(host.actions.map(\.name), ["zoomed", "tapped"])
        XCTAssertEqual(host.actions.first?.payload?["scale"]?.number, 1)
        #if canImport(AppKit)
        var painted = Set<Int>()
        m.paintProbe = { painted.insert($0) }
        let view = NSHostingView(rootView: ExponentialSurface(model: m).frame(width: 390, height: 300))
        view.frame = CGRect(x: 0, y: 0, width: 390, height: 300)
        view.layoutSubtreeIfNeeded()
        XCTAssertTrue(painted.contains(i))
        XCTAssertNotNil(view.bitmapImageRepForCachingDisplay(in: view.bounds))
        #endif
    }

    /// The primitive's public forms build and render (an app's lightbox).
    func testThePrimitivesConveniencesRender() {
        let size = CGSize(width: 1440, height: 900)
        let views: [AnyView] = [
            AnyView(ZoomableImageView(image: Image(systemName: "photo"), naturalSize: size, maths: .core)),
            AnyView(ZoomableImageView(naturalSize: size, maths: .core, resetID: "a", load: { nil })),
            AnyView(ZoomableImageView(naturalSize: size, maths: .core, controls: .always) { Color.red }),
            AnyView(ZoomableImageView(naturalSize: size, maths: .core, controls: .always, content: { Color.blue }, pill: { c in Text(c.percent) })),
        ]
        #if canImport(AppKit)
        for v in views {
            let host = NSHostingView(rootView: v.frame(width: 390, height: 300))
            host.frame = CGRect(x: 0, y: 0, width: 390, height: 300)
            host.layoutSubtreeIfNeeded()
            XCTAssertNotNil(host.bitmapImageRepForCachingDisplay(in: host.bounds))
        }
        #else
        XCTAssertEqual(views.count, 4)
        #endif
    }

    func testTheBoxCapsATallPictureAndFallsBackTo16By9() {
        XCTAssertEqual(SurfaceMeasurer.zoomableBox(["naturalWidth": .number(780), "naturalHeight": .number(25094)], wrap: 390), CGSize(width: 390, height: 390))
        XCTAssertEqual(SurfaceMeasurer.zoomableBox(["naturalWidth": .number(600), "naturalHeight": .number(1200)], wrap: 300), CGSize(width: 300, height: 600))
        XCTAssertEqual(SurfaceMeasurer.zoomableBox([:], wrap: 320), CGSize(width: 320, height: 180))
        XCTAssertEqual(SurfaceMeasurer.zoomableBox(["naturalWidth": .number(1440), "naturalHeight": .number(900)], wrap: nil).width, 320)
        XCTAssertEqual(ZoomableImageLeaf.naturalSize(["naturalWidth": .number(1440), "naturalHeight": .number(900)], image: nil), CGSize(width: 1440, height: 900))
        XCTAssertNil(ZoomableImageLeaf.naturalSize([:], image: nil))
    }
}
