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
    var rtl: Bool { model.paintsRTL }

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

    var font: Font {
        let base: Font
        if let family = textStyle.fontFamily, ExponentialUIFonts.isAvailable(family) {
            base = .custom(family, size: textStyle.fontSize).weight(ExponentialUIFonts.swiftUIWeight(textStyle.fontWeight))
        } else {
            base = .system(size: textStyle.fontSize, weight: ExponentialUIFonts.swiftUIWeight(textStyle.fontWeight))
        }
        return style.italic ? base.italic() : base
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
            guard let n = node(i), !n.hidden, !style(i).invisible, n.accessibility?["hidden"]?.bool != true else { return }
            if n.isLeaf {
                if let l = n.accessibilityLabel, !l.isEmpty, n.component != "Icon" || n.props["label"] != nil { parts.append(l) }
            } else {
                for c in children[safe: i] ?? [] { walk(c) }
            }
        }
        walk(index)
        return parts.joined(separator: ", ")
    }

    /// The box style a node paints with; mirrored controls (unbound
    /// checkboxes, switches, radios, toggles, open triggers) re-resolve theirs.
    func boxStyle(_ index: Int) -> PaintStyle {
        _ = mirrorGeneration
        let base = style(index)
        guard let n = node(index), theme != nil else { return base }
        let states = self.states(of: n.id)
        switch (n.recipeComponent, n.part) {
        case ("Checkbox", "box"), ("Switch", "track"):
            let checked = self.checked(index)
            let external = (owner(of: index)?.props["checked"] ?? .bool(false)).bool ?? false
            if checked == external { return base }
            var props = ownerProps(index)
            props["checked"] = .bool(checked)
            var st = states
            if checked { st.append("checked") }
            return part(n.recipeComponent, n.part ?? "box", props: props, states: st).style
        case ("Radio", "dot"):
            var st = states
            if radioChecked(index) { st.append("checked") }
            return part("Radio", "item", props: ownerProps(index), states: st).style
        case ("Toggle", nil):
            let pressed = togglePressed(index)
            if pressed == (n.props["pressed"]?.bool ?? false) { return base }
            var props = n.props
            props["pressed"] = .bool(pressed)
            return part("Toggle", "root", props: props, states: states).style
        case ("Select", "field"), ("DatePicker", "field"):
            var st = states
            if popup == n.id { st.append("open") }
            return part(n.recipeComponent, "trigger", props: ownerProps(index), states: st).style
        default:
            return base
        }
    }

    /// Is node `id` keyboard-focused (`:focus-visible`, contract §2): the
    /// ring paints for keyboard focus only, never for a press.
    func focusVisible(_ id: String) -> Bool {
        states(of: id).contains("focus-visible")
    }
}

/// Which pressables handle their own gestures (no Button wrapper).
private func selfHandling(_ n: NodeInfo) -> Bool {
    if n.isTextField { return true }
    switch (n.component, n.part) {
    case ("Select", "field"), ("Slider", "track"), ("ToggleGroup", _), ("Box", "indicator"): return true
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
    @Environment(\.accessibilityReduceMotion) private var platformReduced

    var body: some View {
        let frame = model.frame(index)
        let style = model.boxStyle(index)
        let size = frame.size
        let interactive = !style.pointerNone && !style.invisible
        let pressable = node.pressable && !selfHandling(node) && interactive
        let rtl = model.paintsRTL
        let reduced = platformReduced || model.paintReducedMotion
        let animation = style.transition?.animation(reduceMotion: reduced)
        // An Icon NODE mirrors at its box, outside its own transform
        // (contract §4); its content then never flips again.
        let mirrorIcon = node.component == "Icon" && RTLGlyphs.mirrors(node.props.str("name"), rtl: rtl)
        let isRoot = node.parent == nil || index == 0
        Group {
            if pressable {
                Button {
                    model.press(index)
                } label: {
                    painted(style: style, size: size, animation: animation)
                        .contentShape(style.shape(size))
                }
                .buttonStyle(PressReportingStyle(id: node.id, model: model))
                .disabled(model.isDisabled(index))
            } else {
                painted(style: style, size: size, animation: animation)
            }
        }
        .modifier(TriggerModifier(index: index, node: node, model: model))
        .modifier(HoverReporting(id: node.id, model: model, on: interactive && (node.isFocusable || node.pressable)))
        .modifier(CursorModifier(cursor: interactive ? style.cursor : nil))
        .modifier(NodeAccessibility(node: node, model: model, style: style))
        .modifier(PaintOnly(style: style, animation: animation, mirror: mirrorIcon))
        .environment(\.xuiGlyphMirrored, mirrorIcon)
        .transformEnvironment(\.xuiTextPaint) { $0 = $0.merged(style) }
        .modifier(SurfaceEnvironment(on: isRoot, rtl: rtl, reduced: model.paintReducedMotion))
    }

    private func painted(style: PaintStyle, size: CGSize, animation: Animation?) -> some View {
        Group {
            if node.isLeaf {
                let leaf = LeafContent(context: LeafContext(model: model, node: node, size: size, style: style, ink: model.ink(index), textStyle: model.textStyle(index)))
                if (model.children[safe: index] ?? []).isEmpty {
                    leaf
                } else {
                    // A measured leaf that also carries nodes (a DropdownMenu
                    // owner sized by its trigger): the nodes paint over it.
                    ZStack(alignment: .topLeading) {
                        leaf
                        ContainerContent(index: index, node: node, model: model, style: style, size: size)
                    }
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
                .modifier(SkeletonPulse(on: node.component == "Skeleton"))
                .modifier(ClipModifier(style: style, size: size))
        }
        .overlay {
            if model.focusVisible(node.id), style.shadows.isEmpty {
                FocusRing(style: style, size: size, color: model.color("ring") ?? model.ink(index).opacity(0.5))
            }
        }
    }
}

/// The surface-level environment, set at the tree's root and each layer
/// root: the direction text and glyphs read, the surface's reduced motion.
struct SurfaceEnvironment: ViewModifier {
    let on: Bool
    let rtl: Bool
    let reduced: Bool

    func body(content: Content) -> some View {
        if on {
            content.environment(\.xuiRTL, rtl).environment(\.xuiReducedMotion, reduced)
        } else {
            content
        }
    }
}

/// Reports press-down / up to the model (the `pressed` state → the
/// `:pressed` style through the core).
struct PressReportingStyle: ButtonStyle {
    let id: String
    let model: SurfaceModel

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
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
        if let scroll = model.paintScroll(index) {
            ScrollBox(index: index, model: model, size: size, kids: kids, origin: origin, scroll: scroll)
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
        let total = model.nodes.count
        return FrameLayout(size: size, frames: rel) {
            ForEach(kids, id: \.self) { k in
                NodeView(index: k)
                    .layoutValue(key: NodeIndexKey.self, value: k)
                    .accessibilitySortPriority(Double(total - k))
            }
        }
    }
}
