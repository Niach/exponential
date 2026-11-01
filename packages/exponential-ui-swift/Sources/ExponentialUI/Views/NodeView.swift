import SwiftUI
import ExponentialUIPrimitives

/// Everything a leaf painter reads.
@MainActor
struct LeafContext {
    let model: SurfaceModel
    let node: NodeInfo
    let size: CGSize
    let style: PaintStyle
    let ink: Color
    let textStyle: TextStyle

    var index: Int { node.index }
    var props: Props { node.props }
    var ownerProps: Props { model.ownerProps(node.index) }
    var states: [String] { model.states(of: node.id) }
    var dark: Bool { model.mode == .dark }
    var rtl: Bool { model.isRTL }

    /// The content box inside padding + border (per side).
    var inner: CGRect {
        let e = style.insets
        return CGRect(x: e.leading, y: e.top, width: max(0, size.width - e.leading - e.trailing), height: max(0, size.height - e.top - e.bottom))
    }

    func part(_ component: String, _ part: String, states: [String] = []) -> PartStyle {
        model.part(component, part, props: ownerProps, states: states)
    }

    func part(_ component: String, _ part: String, props: Props, states: [String] = []) -> PartStyle {
        model.part(component, part, props: props, states: states)
    }

    func themeColor(_ name: String) -> Color? { model.color(name) }
    func spacing(_ name: String) -> CGFloat { model.spacing(name) }
    func control(_ name: String, _ fallback: CGFloat) -> CGFloat { model.control(name, fallback) }

    /// The leaf's font: the face the measurer shaped with.
    var font: Font {
        Font(ExponentialUIFonts.font(family: textStyle.fontFamily, weight: textStyle.fontWeight, size: textStyle.fontSize, italic: style.italic, tabular: textStyle.tabular) as CTFont)
    }

    /// The semantic tone colour of a `tone` prop.
    func tone(_ name: String) -> Color? {
        switch name {
        case "primary": themeColor("primary")
        case "success": themeColor("success")
        case "warning": themeColor("warning")
        case "danger", "destructive": themeColor("destructive")
        case "info": themeColor("info")
        case "neutral", "muted": themeColor("mutedForeground")
        default: nil
        }
    }
}

extension SurfaceModel {
    /// The accessible name of a pressable container: its visible leaves'
    /// labels in pre-order, comma-joined (what VoiceOver reads for a row).
    public func combinedLabel(_ index: Int) -> String {
        var parts: [String] = []
        func walk(_ i: Int) {
            guard let n = node(i), !n.hidden, !style(i).invisible, !n.a11y.hidden else { return }
            if n.isLeaf {
                if let l = n.accessibilityLabel, !l.isEmpty, n.component != "Icon" || n.props["label"] != nil { parts.append(l) }
            } else {
                for c in children[safe: i] ?? [] { walk(c) }
            }
        }
        walk(index)
        return parts.joined(separator: ", ")
    }

}

/// Which pressables handle their own gestures (no Button wrapper): every
/// host text field (incl. a NumberField / ChipInput `input`, a Select
/// `search`), the slider track, a Segmented, the carousel dots.
private func selfHandling(_ n: NodeInfo) -> Bool {
    if n.isTextField { return true }
    switch (n.component, n.part) {
    case ("Slider", "track"), ("Segmented", _), ("Box", "indicator"): return true
    default: return false
    }
}

/// One node of the surface at its frame: the box (background, gradient,
/// border, radii, shadow, opacity, clip), the leaf content or the nested
/// container layout, the paint-only transform, motion, hover / press /
/// focus-visible, the cursor and the accessibility shape.
struct NodeView: View {
    let index: Int
    @Environment(SurfaceModel.self) private var model

    var body: some View {
        if let node = model.node(index), !node.hidden {
            let _ = model.paintProbe?(index)
            NodeBody(index: index, node: node, model: model)
        }
    }
}

private struct NodeBody: View {
    let index: Int
    let node: NodeInfo
    let model: SurfaceModel

    var body: some View {
        let frame = model.frame(index)
        let style = model.style(index)
        let size = frame.size
        let interactive = !style.pointerNone && !style.invisible
        let pressable = node.pressable && !selfHandling(node) && interactive
        let animation = style.transition?.animation(reduceMotion: model.reducedMotion)
        // An Icon NODE mirrors at its box, outside its own transform
        // (contract §4); its content then never flips again.
        let mirrorIcon = node.component == "Icon" && RTLGlyphs.mirrors(node.props.str("name"), rtl: model.isRTL)
        // The pointer (gpui `interactive`): pressables, focusables, fields,
        // hover triggers and boxes with a transition report `hover`.
        // Round 2: the core marks nodes that restyle under the pointer
        // (`hoverStyled`: a `:hover` block or a recipe `hover` rule).
        let hovers = interactive && (node.pressable || node.isFocusable || node.triggerFor != nil || node.hoverStyled || style.transition != nil)
        Group {
            if pressable, model.isHoldButton(node) {
                // VAPP-162: a hold Button never presses; the gesture drives
                // the core's hold (it owns `pressed` and the label).
                painted(style: style, size: size, animation: animation)
                    .contentShape(style.shape(size))
                    .scaleEffect(!model.reducedMotion && model.isHeld(id: node.id) ? CGFloat(model.theme?.scale("press") ?? 1) : 1)
                    .modifier(HoldGesture(index: index, model: model))
            } else if pressable {
                Button {
                    model.press(index)
                } label: {
                    painted(style: style, size: size, animation: animation)
                        .contentShape(style.shape(size))
                }
                .buttonStyle(PressReportingStyle(id: node.id, model: model, pressScale: node.component == "Button" && !model.reducedMotion ? CGFloat(model.theme?.scale("press") ?? 1) : 1))
                .disabled(model.isDisabled(index))
            } else {
                painted(style: style, size: size, animation: animation)
            }
        }
        .modifier(TooltipLongPress(node: node, model: model))
        .modifier(NativeHooks(node: node, model: model))
        .modifier(HoverReporting(id: node.id, model: model, on: hovers))
        .modifier(ResizeHandleModifier(node: node, model: model, enabled: interactive && !model.isDisabled(index)))
        .modifier(CursorModifier(cursor: interactive ? style.cursor : nil))
        .modifier(AccessibilityModifier(node: node, model: model, style: style))
        .modifier(PaintOnly(style: style, animation: animation, mirror: mirrorIcon))
        // Round 2: keyframe animations from the core's frames, outside the
        // node's own transform; reduced motion = the rest frame.
        .modifier(KeyframeAnimationModifier(animation: style.animation, reducedMotion: model.reducedMotion, size: size, bandColor: style.background ?? model.color("muted")))
        .environment(\.xuiGlyphMirrored, mirrorIcon)
        .transformEnvironment(\.xuiTextPaint) { $0 = $0.merged(style) }
    }

    private func painted(style: PaintStyle, size: CGSize, animation: Animation?) -> some View {
        Group {
            if node.isLeaf {
                let leaf = LeafContent(context: LeafContext(model: model, node: node, size: size, style: style, ink: model.ink(index), textStyle: model.textStyle(index)))
                if (model.children[safe: index] ?? []).isEmpty {
                    leaf
                } else {
                    // A measured leaf that also carries nodes (a Menu
                    // owner sized by its trigger): the nodes paint over it.
                    ZStack(alignment: .topLeading) {
                        leaf
                        ContainerContent(index: index, node: node, model: model, style: style, size: size)
                    }
                }
            } else if node.component == "VoiceWave", node.part == nil {
                // NEW-77: the rings behind the one child, centred. VAPP-162:
                // the `bars` meter keeps its child in the tree, never painted.
                let cx = LeafContext(model: model, node: node, size: size, style: style, ink: model.ink(index), textStyle: model.textStyle(index))
                ZStack(alignment: .topLeading) {
                    VoiceWaveLeaf(cx: cx)
                    VoiceWaveChild(index: index, model: model, size: size, hidden: node.props.str("variant") == "bars")
                }
            } else {
                ContainerContent(index: index, node: node, model: model, style: style, size: size)
            }
        }
        // Motion (contract §2): only THIS node's box animates, with its
        // own `transition` and easing; nothing animates without one.
        .animation(animation) { box in
            box
                .frame(width: size.width, height: size.height, alignment: .topLeading)
                .paintedBox(style, size: size)
                .modifier(BackdropBlurModifier(style: style, size: size))
                .modifier(SkeletonPulse(on: node.component == "Skeleton" && node.isLeaf))
                .modifier(ClipModifier(style: style, size: size))
        }
        .overlay {
            if model.focusVisible(node.id), style.shadows.isEmpty {
                FocusRing(style: style, size: size, color: model.color("ring") ?? model.ink(index).opacity(0.5))
            }
        }
    }
}

/// VAPP-162 (round-5 contract §9): a hold Button's pointer: a zero-distance
/// drag claimed over the platform's long-press and scroll gestures
/// (`highPriorityGesture`), its locations (the button's own coordinates)
/// handed to the core as pointer down / move / up; the slop (24 pt) and
/// the slide-away cancel are the core's. A drag the system took back (the
/// gesture state reset without an end) is a pointer cancel.
struct HoldGesture: ViewModifier {
    let index: Int
    let model: SurfaceModel
    @GestureState private var tracking = false
    @State private var down = false

    func body(content: Content) -> some View {
        content
            .highPriorityGesture(
                DragGesture(minimumDistance: 0, coordinateSpace: .local)
                    .updating($tracking) { _, state, _ in state = true }
                    .onChanged { v in
                        if !down {
                            down = true
                            model.holdPointer(index, "pointerdown", at: v.startLocation)
                            if v.location != v.startLocation { model.holdPointer(index, "pointermove", at: v.location) }
                        } else {
                            model.holdPointer(index, "pointermove", at: v.location)
                        }
                    }
                    .onEnded { v in
                        guard down else { return }
                        down = false
                        model.holdPointer(index, "pointerup", at: v.location)
                    }
            )
            .onChange(of: tracking) { _, now in
                guard !now else { return }
                // `onEnded` runs first on a normal release; a reset without it
                // = the gesture was cancelled.
                Task { @MainActor in
                    if down {
                        down = false
                        model.holdPointerCancel(index)
                    }
                }
            }
            .accessibilityAction { model.holdToggle(index) }
    }
}

/// Reports press-down / up to the model (the `pressed` state → the
/// `:pressed` style through the core).
struct PressReportingStyle: ButtonStyle {
    let id: String
    let model: SurfaceModel
    /// EXP-1259: a pressed Button scales to the theme's `scale.press` (1 =
    /// none; never under reduced motion).
    var pressScale: CGFloat = 1

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed ? pressScale : 1)
            .animation(pressScale == 1 ? nil : .timingCurve(0.2, 0, 0, 1, duration: (model.theme?.motion("fast") ?? 120) / 1000), value: configuration.isPressed)
            .onChange(of: configuration.isPressed) { _, pressed in
                if pressed { model.pressDown(id) } else { model.pressUp(id) }
            }
    }
}

/// A container: its children nested at their frames (relative to it); a
/// scroll container (the core's `overflow: scroll | auto` per axis, or a
/// windowed List) in a scroll view.
struct ContainerContent: View {
    let index: Int
    let node: NodeInfo
    let model: SurfaceModel
    let style: PaintStyle
    let size: CGSize

    var body: some View {
        let kids = (model.children[safe: index] ?? []).filter { k in
            guard let c = model.node(k), !c.hidden else { return false }
            // Layer roots paint in their layer, never nested in the tree.
            return c.layer == node.layer
        }
        let origin = model.frame(index).origin
        if model.scroll(index) != nil || model.lists[node.id]?.windowed == true {
            ScrollContainer(index: index, node: node, model: model, size: size, kids: kids, origin: origin)
        } else {
            ChildrenLayout(size: size, kids: kids, origin: origin, model: model)
        }
    }
}

struct ChildrenLayout: View {
    let size: CGSize
    let kids: [Int]
    let origin: CGPoint
    let model: SurfaceModel

    var body: some View {
        var rel: [Int: CGRect] = [:]
        for k in kids { rel[k] = model.frame(k).offsetBy(dx: -origin.x, dy: -origin.y) }
        return FrameLayout(size: size, frames: rel) {
            ForEach(kids, id: \.self) { k in
                NodeView(index: k)
                    // Round 2: a sticky node / pinned section header moves by the core's offset.
                    .modifier(StickyModifier(offset: model.sticky[k]))
                    .layoutValue(key: NodeIndexKey.self, value: k)
                    // VoiceOver reads in PAINT order (slots are not pre-order).
                    .accessibilitySortPriority(model.accessibilityPriority(k))
            }
        }
    }
}

/// NEW-77 THE CHILD RULE: a VoiceWave's one child at the frame the core
/// centres it at (its own size), drawn scaled about the wave box's centre
/// by the core's `child.scale` (min(1, childSize / its longer edge), never
/// up); at `sm` (scale 0) kept in the tree but not painted, no hit target,
/// hidden from VoiceOver.
struct VoiceWaveChild: View {
    let index: Int
    let model: SurfaceModel
    let size: CGSize
    /// VAPP-162: the `bars` meter keeps its child like `sm` (scale 0).
    var hidden = false

    var body: some View {
        let edge = min(size.width, size.height)
        let kids = Array((model.children[safe: index] ?? []).filter { model.node($0)?.hidden == false }.prefix(1))
        let origin = model.frame(index).origin
        var rel: [Int: CGRect] = [:]
        for k in kids { rel[k] = model.frame(k).offsetBy(dx: -origin.x, dy: -origin.y) }
        return FrameLayout(size: size, frames: rel) {
            ForEach(kids, id: \.self) { k in
                let f = rel[k] ?? .zero
                let scale = hidden ? 0 : VoiceWaveFrame.childBox(size: edge, child: f.size)?.scale ?? 0
                NodeView(index: k)
                    .voiceWaveChild(scale: scale, frame: f, centre: CGPoint(x: size.width / 2, y: size.height / 2))
                    .layoutValue(key: NodeIndexKey.self, value: k)
            }
        }
    }
}

/// A Resizable `handle` (round 2 §1): a drag along the group's axis,
/// reported as `drag {phase, delta}` in pt of pointer travel since the drag
/// STARTED (the core resizes from the sizes at the start), measured in
/// GLOBAL coordinates (the handle moves with the split after each `move`);
/// an 8 pt hit area centred on the hairline; VoiceOver adjusts it with the
/// core's keys.
struct ResizeHandleModifier: ViewModifier {
    let node: NodeInfo
    let model: SurfaceModel
    let enabled: Bool
    @State private var dragging = false

    func body(content: Content) -> some View {
        if node.ownerComponent == "Resizable", node.part == "handle", enabled {
            // `orientation` = the LINE's: a horizontal line moves along y.
            let alongY = node.props.str("orientation") == "horizontal"
            let index = node.index
            let hit: CGFloat = 8
            content
                .contentShape(Rectangle().inset(by: -hit / 2))
                .gesture(
                    DragGesture(minimumDistance: 0, coordinateSpace: .global)
                        .onChanged { g in
                            let delta = alongY ? g.translation.height : g.translation.width
                            if !dragging {
                                dragging = true
                                model.pressDown(node.id)
                                model.resizeDrag(index, phase: "start", delta: 0)
                            }
                            model.resizeDrag(index, phase: "move", delta: delta)
                        }
                        .onEnded { g in
                            let delta = alongY ? g.translation.height : g.translation.width
                            dragging = false
                            model.resizeDrag(index, phase: "end", delta: delta)
                            model.pressUp(node.id)
                        }
                )
                .accessibilityAdjustableAction { direction in
                    switch direction {
                    case .increment: model.nodeKey(index, key: alongY ? "ArrowDown" : (model.isRTL ? "ArrowLeft" : "ArrowRight"))
                    case .decrement: model.nodeKey(index, key: alongY ? "ArrowUp" : (model.isRTL ? "ArrowRight" : "ArrowLeft"))
                    @unknown default: break
                    }
                }
        } else {
            content
        }
    }
}
