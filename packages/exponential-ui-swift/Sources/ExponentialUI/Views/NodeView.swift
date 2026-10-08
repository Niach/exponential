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
        Font(ExponentialUIFonts.font(family: textStyle.fontFamily, weight: textStyle.fontWeight, size: textStyle.fontSize, italic: style.italic) as CTFont)
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
/// `search`), the slider track, a ToggleGroup, the carousel dots.
private func selfHandling(_ n: NodeInfo) -> Bool {
    if n.isTextField { return true }
    switch (n.component, n.part) {
    case ("Slider", "track"), ("ToggleGroup", _), ("Box", "indicator"): return true
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
        let hovers = interactive && (node.pressable || node.isFocusable || node.triggerFor != nil || style.transition != nil)
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
        .modifier(TooltipLongPress(node: node, model: model))
        .modifier(NativeHooks(node: node, model: model))
        .modifier(HoverReporting(id: node.id, model: model, on: hovers))
        .modifier(CursorModifier(cursor: interactive ? style.cursor : nil))
        .modifier(AccessibilityModifier(node: node, model: model, style: style))
        .modifier(PaintOnly(style: style, animation: animation, mirror: mirrorIcon))
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
                    .layoutValue(key: NodeIndexKey.self, value: k)
                    // VoiceOver reads in PAINT order (slots are not pre-order).
                    .accessibilitySortPriority(model.accessibilityPriority(k))
            }
        }
    }
}
