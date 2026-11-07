import SwiftUI
import CoreText
import ExponentialUICore

/// Round 8 (docs/round-8-contract.md §1): a compiled `Canvas`: the core's
/// `CanvasPlayer` over uniffi (one static frame of Motion's display list +
/// the `text` ops) and the last sampled frame. Recompiled when `size` or
/// `shapes` change, resampled when the theme or the mode change; never
/// played.
final class CanvasSource {
    private var compiledKey: String?
    private var player: CanvasPlayer?
    private var sampledKey: String?
    private(set) var list: MotionList?

    /// The document the engine compiles: the node's `size` and `shapes`.
    static func document(_ props: Props) -> String {
        JSONValue.object(["size": props["size"] ?? .null, "shapes": props["shapes"] ?? .null]).json
    }

    /// The host's slot overrides (Motion's contract): every `$color.<token>`
    /// slot the drawing declares gets the theme's colour, `$mode` the mode.
    static func slots(_ ids: [String], color: (String) -> String?, dark: Bool) -> String {
        var out: [String: JSONValue] = [:]
        for sid in ids where sid.hasPrefix("$color.") {
            guard let hex = color(String(sid.dropFirst("$color.".count))), let c = MotionSource.rgba(hex) else { continue }
            out[sid] = .object(["p": .object(["a": .number(0), "k": .array(c.map(JSONValue.number))])])
        }
        if dark { out["$mode"] = .string("dark") }
        return JSONValue.object(out).json
    }

    /// The frame for these props in this theme and mode (nil = refused or
    /// empty: the box paints nothing, never a crash).
    func frame(props: Props, theme: ThemeHandle?, mode: Mode) -> MotionList? {
        let doc = Self.document(props)
        if doc != compiledKey {
            compiledKey = doc
            sampledKey = nil
            list = nil
            player = try? CanvasPlayer(canvasJson: doc)
        }
        guard let player else { return nil }
        let themeKey = "\(theme.map { ObjectIdentifier($0).hashValue } ?? 0)|\(mode.rawValue)"
        if themeKey != sampledKey {
            sampledKey = themeKey
            let json = Self.slots(player.slotIds(), color: { theme?.colorHex($0, mode: mode) }, dark: mode == .dark)
            list = MotionList(packed: player.sample(slotsJson: json))
        }
        return list
    }

    /// The text ops' fonts: the theme's `$type.family.sans` / `.mono` at the
    /// CSS weight (system faces without a theme).
    static func font(theme: ThemeHandle?) -> MotionPainter.TextFont {
        let sans = theme?.fontFamily("sans")
        let mono = theme?.fontFamily("mono") ?? "ui-monospace"
        return { isMono, weight, size in TextFonts.font(family: isMono ? mono : sans, weight: weight, size: size) }
    }
}

/// The `Canvas` leaf: the frame painted by `MotionPainter` (shapes, then
/// native text) under `fit`; an img named by `label` (AccessibilityModifier).
/// With `pointer: true`: `press {x, y}` on a click / tap released inside,
/// `move {x, y}` while hovered or dragging (at most once per frame), `leave`
/// when the pointer goes; x/y = `canvasToDrawing` (drawing units).
struct CanvasLeaf: View {
    let cx: LeafContext
    @State private var source = CanvasSource()
    @State private var pointer = CanvasPointerState()

    var body: some View {
        let props = cx.props
        let fit = props.str("fit").isEmpty ? "contain" : props.str("fit")
        let list = source.frame(props: props, theme: cx.model.theme, mode: cx.model.mode)
        let font = CanvasSource.font(theme: cx.model.theme)
        let size = cx.size
        let canvas = Canvas { ctx, box in
            guard let list else { return }
            MotionPainter.paint(list, in: &ctx, box: box, fit: fit, font: font)
        }
        .frame(width: size.width, height: size.height)
        if props["pointer"]?.bool == true, let list {
            let drawing = CGSize(width: list.width, height: list.height)
            canvas
                .contentShape(Rectangle())
                .modifier(CanvasPointer(state: pointer, send: { name, at in send(name, at, drawing: drawing, box: size, fit: fit) }, box: size))
        } else {
            canvas
        }
    }

    @MainActor
    private func send(_ name: String, _ at: CGPoint?, drawing: CGSize, box: CGSize, fit: String) {
        let model = cx.model
        guard let i = model.byId[cx.node.id] else { return }
        var payload: JSONValue?
        if let at {
            let p = canvasToDrawing(x: at.x, y: at.y, width: drawing.width, height: drawing.height, boxWidth: box.width, boxHeight: box.height, fit: fit)
            guard p.count == 2 else { return }
            payload = .object(["x": .number(p[0]), "y": .number(p[1])])
        }
        guard let events = try? model.surface.event(index: UInt32(i), name: name, payloadJson: payload?.json), !events.isEmpty else { return }
        model.dispatch(events)
    }
}

/// The pointer's coalescing: the latest `move` point waits for the next
/// frame (at most one `move` per frame; the last point is never dropped).
@MainActor
final class CanvasPointerState {
    var pending: CGPoint?
    var scheduled = false
    var dragging = false
    var hovering = false

    func move(_ p: CGPoint, send: @escaping @MainActor (String, CGPoint?) -> Void) {
        pending = p
        guard !scheduled else { return }
        scheduled = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.0 / 60.0) { [weak self] in
            guard let self else { return }
            self.scheduled = false
            if let p = self.pending {
                self.pending = nil
                send("move", p)
            }
        }
    }

    /// A pending move goes first, then `name`.
    func flush(_ name: String, _ p: CGPoint?, send: @MainActor (String, CGPoint?) -> Void) {
        if let q = pending {
            pending = nil
            send("move", q)
        }
        send(name, p)
    }
}

private struct CanvasPointer: ViewModifier {
    let state: CanvasPointerState
    let send: @MainActor (String, CGPoint?) -> Void
    let box: CGSize

    func body(content: Content) -> some View {
        content
            .gesture(DragGesture(minimumDistance: 0, coordinateSpace: .local)
                .onChanged { e in
                    state.dragging = true
                    state.move(e.location, send: send)
                }
                .onEnded { e in
                    state.dragging = false
                    let inside = CGRect(origin: .zero, size: box).contains(e.location)
                    if inside { state.flush("press", e.location, send: send) } else { state.pending = nil }
                    // A touch leaves with its finger (the web's pointerleave after
                    // pointerup); a hovering mouse leaves through the hover.
                    if !state.hovering { send("leave", nil) }
                })
            .onContinuousHover(coordinateSpace: .local) { phase in
                switch phase {
                case let .active(p):
                    state.hovering = true
                    state.move(p, send: send)
                case .ended:
                    state.hovering = false
                    if !state.dragging { state.flush("leave", nil, send: send) }
                }
            }
    }
}
