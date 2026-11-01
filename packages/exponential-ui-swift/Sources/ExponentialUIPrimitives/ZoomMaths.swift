import Foundation
import CoreGraphics

/// EXP-1586: one view of a `ZoomableImage` (catalog/style.json
/// `zoomableImage`): `scale` = displayed px per image px (1 = 100%: one
/// image pixel per point), `x` / `y` = the picture's top-left in the
/// viewport.
public struct ZoomView: Equatable, Sendable, Codable {
    public var scale: Double
    public var x: Double
    public var y: Double

    public init(scale: Double, x: Double, y: Double) {
        self.scale = scale
        self.x = x
        self.y = y
    }

    public static let identity = ZoomView(scale: 1, x: 0, y: 0)
}

/// The `initial` prop (catalog enum `zoomInitial`): `auto` = fitWidth for a
/// TALL picture, else fit.
public enum ZoomInitial: String, Sendable, CaseIterable {
    case auto, fit, fitWidth, actual

    /// An unknown value = `auto` (the core's rule).
    public init(prop: String) { self = ZoomInitial(rawValue: prop) ?? .auto }
}

/// The `controls` prop (catalog enum `zoomControls`): THE pill shown on
/// appear and hidden after `autoHideMs` idle (`auto`), always, or never.
public enum ZoomControls: String, Sendable, CaseIterable {
    case auto, always, none

    public init(prop: String) { self = ZoomControls(rawValue: prop) ?? .auto }
}

/// What every zoom computation needs: the picture's natural size (px), the
/// viewport (pt), the opening mode and the largest zoom (nil = style.json's
/// `maxScale`).
public struct ZoomFrame: Equatable, Sendable {
    public var natural: CGSize
    public var viewport: CGSize
    public var initial: ZoomInitial
    public var maxScale: Double?

    public init(natural: CGSize, viewport: CGSize, initial: ZoomInitial = .auto, maxScale: Double? = nil) {
        self.natural = natural
        self.viewport = viewport
        self.initial = initial
        self.maxScale = maxScale
    }
}

/// The core's `rest` answer: fit (= the minimum), the limits, the opening
/// scale and view, and whether the picture is TALL.
public struct ZoomRest: Equatable, Sendable {
    public var fit: Double
    public var min: Double
    public var max: Double
    public var restScale: Double
    public var view: ZoomView
    public var tall: Bool

    public init(fit: Double, min: Double, max: Double, restScale: Double, view: ZoomView, tall: Bool) {
        self.fit = fit
        self.min = min
        self.max = max
        self.restScale = restScale
        self.view = view
        self.tall = tall
    }
}

/// The pill's disabled ends.
public struct ZoomBounds: Equatable, Sendable {
    public var atMin: Bool
    public var atMax: Bool

    public init(atMin: Bool, atMax: Bool) {
        self.atMin = atMin
        self.atMax = atMax
    }
}

/// The axes the picture can pan on (an axis without room belongs to the host).
public struct ZoomPanRoom: Equatable, Sendable {
    public var x: Bool
    public var y: Bool

    public init(x: Bool, y: Bool) {
        self.x = x
        self.y = y
    }
}

/// One row of the percent button's Menu: nil scale = Fit (the rest view).
public struct ZoomMenuItem: Equatable, Sendable, Identifiable {
    public var scale: Double?
    public var label: String
    public var checked: Bool
    public var id: String { scale.map { "\($0)" } ?? "fit" }

    public init(scale: Double?, label: String, checked: Bool) {
        self.scale = scale
        self.label = label
        self.checked = checked
    }
}

/// THE ZOOM MATHS, answered by the eUI core (`eui-core::zoom`, mirrored by
/// `src/zoom.ts`, locked by `fixtures/zoom.json`): a JSON request in, a
/// JSON answer out. This module has no binary, so the caller hands the
/// core's entry: `ExponentialUI` ships `ZoomMaths.core` (the FFI's
/// `zoomJson`). Every gesture of `ZoomableImageView` goes through it, so a
/// pinch, a wheel or a double tap lands on the same view on every platform.
public struct ZoomMaths: Sendable {
    /// `{op, frame, from?, …}` → the op's answer (see `zoom_request`).
    public let request: @Sendable (String) -> String

    public init(_ request: @escaping @Sendable (String) -> String) {
        self.request = request
    }

    // MARK: transport

    /// The raw call: `op` + its fields (`frame` and `from` added when given;
    /// a nil `from` = the rest view). nil = an answer that is not JSON.
    public func call(_ op: String, frame: ZoomFrame?, from: ZoomView? = nil, _ fields: [String: Any] = [:]) -> Any? {
        var body = fields
        body["op"] = op
        if let frame { body["frame"] = Self.encode(frame) }
        if let from { body["from"] = Self.encode(from) }
        guard JSONSerialization.isValidJSONObject(body),
              let data = try? JSONSerialization.data(withJSONObject: body),
              let text = String(data: data, encoding: .utf8) else { return nil }
        let answer = request(text)
        return try? JSONSerialization.jsonObject(with: Data(answer.utf8), options: [.fragmentsAllowed])
    }

    static func finite(_ v: Double) -> Double { v.isFinite ? v : 0 }

    static func encode(_ s: CGSize) -> [String: Any] { ["width": finite(Double(s.width)), "height": finite(Double(s.height))] }
    static func encode(_ p: CGPoint) -> [String: Any] { ["x": finite(Double(p.x)), "y": finite(Double(p.y))] }
    static func encode(_ v: ZoomView) -> [String: Any] { ["scale": finite(v.scale), "x": finite(v.x), "y": finite(v.y)] }
    static func encode(_ f: ZoomFrame) -> [String: Any] {
        var o: [String: Any] = ["natural": encode(f.natural), "viewport": encode(f.viewport), "initial": f.initial.rawValue]
        if let m = f.maxScale, m.isFinite { o["maxScale"] = m }
        return o
    }

    static func number(_ v: Any?) -> Double? { (v as? NSNumber).flatMap { CFGetTypeID($0) == CFBooleanGetTypeID() ? nil : $0.doubleValue } }
    static func bool(_ v: Any?) -> Bool { (v as? NSNumber)?.boolValue ?? false }
    static func view(_ v: Any?) -> ZoomView? {
        guard let o = v as? [String: Any], let s = number(o["scale"]) else { return nil }
        return ZoomView(scale: s, x: number(o["x"]) ?? 0, y: number(o["y"]) ?? 0)
    }

    private func view(_ op: String, _ frame: ZoomFrame, from: ZoomView?, _ fields: [String: Any] = [:], fallback: ZoomView) -> ZoomView {
        Self.view(call(op, frame: frame, from: from, fields)) ?? fallback
    }

    // MARK: the ops (named like the fixtures/zoom.json groups)

    /// Fit, the limits, the rest scale + view and TALL.
    public func rest(_ f: ZoomFrame) -> ZoomRest {
        let o = call("rest", frame: f) as? [String: Any] ?? [:]
        let limits = o["limits"] as? [String: Any] ?? [:]
        return ZoomRest(
            fit: Self.number(o["fit"]) ?? 1,
            min: Self.number(limits["min"]) ?? 1,
            max: Self.number(limits["max"]) ?? 1,
            restScale: Self.number(o["restScale"]) ?? 1,
            view: Self.view(o["view"]) ?? .identity,
            tall: Self.bool(o["tall"])
        )
    }

    /// The view inside its bounds (scale within the limits, no letterbox).
    public func clamp(_ v: ZoomView, _ f: ZoomFrame) -> ZoomView {
        Self.view(call("clamp", frame: f, ["view": Self.encode(v)])) ?? v
    }

    /// Zoom to `scale` (clamped) keeping the point under `anchor` fixed.
    public func zoomTo(_ v: ZoomView, scale: Double, anchor: CGPoint, _ f: ZoomFrame) -> ZoomView {
        view("zoomTo", f, from: v, ["scale": Self.finite(scale), "anchor": Self.encode(anchor)], fallback: v)
    }

    /// One ⌘/ctrl-wheel (or trackpad-pinch-as-wheel) event about the cursor;
    /// `deltaY` in the web's sign (positive = scrolling down = zoom out).
    public func wheel(_ v: ZoomView, deltaY: Double, lines: Bool = false, anchor: CGPoint, _ f: ZoomFrame) -> ZoomView {
        view("wheel", f, from: v, ["deltaY": Self.finite(deltaY), "mode": lines ? "line" : "pixel", "anchor": Self.encode(anchor)], fallback: v)
    }

    /// A plain wheel / two-finger scroll: pans by the (web-signed) deltas.
    public func scroll(_ v: ZoomView, deltaX: Double, deltaY: Double, _ f: ZoomFrame) -> ZoomView {
        view("scroll", f, from: v, ["deltaX": Self.finite(deltaX), "deltaY": Self.finite(deltaY)], fallback: v)
    }

    /// The ladder `step` walks (presets + rest, inside the limits).
    public func ladder(_ f: ZoomFrame) -> [Double] {
        (call("ladder", frame: f) as? [Any] ?? []).compactMap(Self.number)
    }

    /// + (`1`) / − (`-1`) about the viewport centre.
    public func step(_ v: ZoomView, direction: Int, _ f: ZoomFrame) -> ZoomView {
        view("step", f, from: v, ["direction": direction < 0 ? -1 : 1], fallback: v)
    }

    /// Double tap / click: at rest → zoom in about the point; else → rest.
    public func doubleTap(_ v: ZoomView, at point: CGPoint, _ f: ZoomFrame) -> ZoomView {
        view("doubleTap", f, from: v, ["point": Self.encode(point)], fallback: v)
    }

    /// A key (`+ = - − _ 0 1`); nil = not a zoom key.
    public func key(_ v: ZoomView, key: String, _ f: ZoomFrame) -> ZoomView? {
        Self.view(call("keys", frame: f, from: v, ["key": key]))
    }

    /// The damped overshoot past an edge.
    public func rubberBand(overshoot: Double, extent: Double) -> Double {
        Self.number(call("rubberBand", frame: nil, ["overshoot": Self.finite(overshoot), "extent": Self.finite(extent)])) ?? 0
    }

    /// A drag from `start` by the translation while held (rubber-banded
    /// past the edges); the release is `clamp` of the last view.
    public func drag(_ start: ZoomView, dx: Double, dy: Double, _ f: ZoomFrame) -> ZoomView {
        view("drag", f, from: start, ["dx": Self.finite(dx), "dy": Self.finite(dy)], fallback: start)
    }

    /// A pinch from `start` by the raw factor about `anchor` while held
    /// (rubber-banded past the min / max); the release is `clamp`.
    public func pinch(_ start: ZoomView, factor: Double, anchor: CGPoint, _ f: ZoomFrame) -> ZoomView {
        view("pinch", f, from: start, ["factor": Self.finite(factor), "anchor": Self.encode(anchor)], fallback: start)
    }

    /// The momentum `t` ms after a release at `velocity` (px/ms), stopping at the edges.
    public func momentum(_ v: ZoomView, velocity: CGVector, t: Double, _ f: ZoomFrame) -> ZoomView {
        view("momentum", f, from: v, ["vx": Self.finite(Double(velocity.dx)), "vy": Self.finite(Double(velocity.dy)), "t": Self.finite(t)], fallback: v)
    }

    /// Whether the momentum still moves `t` ms after the release.
    public func momentumActive(velocity: CGVector, t: Double) -> Bool {
        Self.bool(call("momentumActive", frame: nil, ["vx": Self.finite(Double(velocity.dx)), "vy": Self.finite(Double(velocity.dy)), "t": Self.finite(t)]))
    }

    /// The axes the picture can pan on.
    public func panRoom(_ v: ZoomView, _ f: ZoomFrame) -> ZoomPanRoom {
        let o = call("panRoom", frame: f, from: v) as? [String: Any] ?? [:]
        return ZoomPanRoom(x: Self.bool(o["x"]), y: Self.bool(o["y"]))
    }

    /// A new viewport (`f.viewport`; the view was laid out at `old`): at
    /// rest = the new rest view, zoomed = the same scale about the centre.
    public func resize(_ v: ZoomView, from old: CGSize, _ f: ZoomFrame) -> ZoomView {
        view("resize", f, from: v, ["fromViewport": Self.encode(old)], fallback: v)
    }

    /// The pill's label (`round(scale × 100)%`).
    public func percent(_ scale: Double) -> String {
        call("percent", frame: nil, ["scale": Self.finite(scale)]) as? String ?? ""
    }

    /// The percent button's Menu: Fit, then the presets inside the limits.
    public func menu(_ v: ZoomView, _ f: ZoomFrame, fitLabel: String) -> [ZoomMenuItem] {
        (call("menu", frame: f, from: v, ["fitLabel": fitLabel]) as? [Any] ?? []).compactMap { item in
            guard let o = item as? [String: Any] else { return nil }
            return ZoomMenuItem(scale: Self.number(o["scale"]), label: o["label"] as? String ?? "", checked: Self.bool(o["checked"]))
        }
    }

    /// The pill's disabled ends.
    public func bounds(_ v: ZoomView, _ f: ZoomFrame) -> ZoomBounds {
        let o = call("bounds", frame: f, from: v) as? [String: Any] ?? [:]
        return ZoomBounds(atMin: Self.bool(o["atMin"]), atMax: Self.bool(o["atMax"]))
    }
}

/// THE PILL's geometry and the auto-hide delay (style.json
/// `zoomableImage.pill` + `autoHideMs`; mirrored here because this module
/// reads no catalog, and checked against style.json by the tests).
public struct ZoomPillMetrics: Equatable, Sendable {
    public var height: CGFloat
    /// From the picture's bottom edge to the pill's.
    public var inset: CGFloat
    public var padding: CGFloat
    public var gap: CGFloat
    public var percentMinWidth: CGFloat
    public var dividerHeight: CGFloat
    public var autoHideMs: Double

    public init(height: CGFloat = 36, inset: CGFloat = 16, padding: CGFloat = 4, gap: CGFloat = 2, percentMinWidth: CGFloat = 56, dividerHeight: CGFloat = 16, autoHideMs: Double = 2000) {
        self.height = height
        self.inset = inset
        self.padding = padding
        self.gap = gap
        self.percentMinWidth = percentMinWidth
        self.dividerHeight = dividerHeight
        self.autoHideMs = autoHideMs
    }

    public static let standard = ZoomPillMetrics()
}
