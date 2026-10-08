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

    /// The content box inside padding + border.
    var inner: CGRect {
        let (ix, iy) = style.inset
        return CGRect(x: ix, y: iy, width: max(0, size.width - 2 * ix), height: max(0, size.height - 2 * iy))
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
        if let family = textStyle.fontFamily, ExponentialUIFonts.isAvailable(family) {
            return .custom(family, size: textStyle.fontSize).weight(ExponentialUIFonts.swiftUIWeight(textStyle.fontWeight))
        }
        return .system(size: textStyle.fontSize, weight: ExponentialUIFonts.swiftUIWeight(textStyle.fontWeight))
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
            guard let n = node(i), !n.hidden else { return }
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
}

/// Which pressables handle their own gestures (no Button wrapper).
private func selfHandling(_ n: NodeInfo) -> Bool {
    if n.isTextField { return true }
    switch (n.component, n.part) {
    case ("Select", "field"), ("Slider", "track"), ("ToggleGroup", _), ("Box", "indicator"): return true
    default: return false
    }
}

/// One node of the surface at its frame: the box (background, border,
/// radius, shadow, opacity, clip), the leaf content or the nested
/// container layout, the press handling and the accessibility shape.
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
        let style = model.boxStyle(index)
        let size = frame.size
        let pressable = node.pressable && !selfHandling(node)
        Group {
            if pressable {
                Button {
                    model.press(index)
                } label: {
                    painted(style: style, size: size)
                        .contentShape(RoundedRectangle(cornerRadius: style.radius))
                }
                .buttonStyle(PressReportingStyle(id: node.id, model: model))
                .disabled(model.isDisabled(index))
            } else {
                painted(style: style, size: size)
            }
        }
        .modifier(TriggerModifier(index: index, node: node, model: model))
        .modifier(AccessibilityModifier(node: node, model: model))
    }

    private func painted(style: PaintStyle, size: CGSize) -> some View {
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
        .frame(width: size.width, height: size.height, alignment: .topLeading)
        .paintedBox(style, size: size)
        .modifier(ClipModifier(on: style.overflowHidden || style.overflowScroll || (style.radius > 0 && !node.isLeaf), radius: style.radius))
    }
}

private struct ClipModifier: ViewModifier {
    let on: Bool
    let radius: CGFloat

    func body(content: Content) -> some View {
        if on {
            content.clipShape(RoundedRectangle(cornerRadius: radius, style: .continuous))
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
/// scrolling one in a scroll view.
struct ContainerContent: View {
    let index: Int
    let node: NodeInfo
    let model: SurfaceModel
    let style: PaintStyle
    let size: CGSize

    var body: some View {
        let kids = (model.children[safe: index] ?? []).filter { !(model.node($0)?.hidden ?? true) }
        let origin = model.frame(index).origin
        let scrolls = style.overflowScroll || (model.lists[node.id]?.windowed ?? false)
        if scrolls {
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

/// The accessibility shape of a node: VoiceOver reads pre-order (sort
/// priorities), containers group, pressables combine into one element,
/// leaves carry their label and trait.
private struct AccessibilityModifier: ViewModifier {
    let node: NodeInfo
    let model: SurfaceModel

    func body(content: Content) -> some View {
        if node.isLeaf {
            leaf(content)
        } else if node.pressable {
            // One element for the row: SwiftUI's `.combine` skips the UIKit
            // text labels, so the name is assembled from the leaves in order.
            content.accessibilityElement(children: .ignore).accessibilityLabel(model.combinedLabel(node.index)).accessibilityAddTraits(.isButton)
        } else if node.macroName == "Card" || node.macroName == "Group" || node.macroName == "Alert" || node.part == "content" {
            content.accessibilityElement(children: .contain).accessibilityLabel(node.accessibilityLabel ?? "")
        } else {
            content.accessibilityElement(children: .contain)
        }
    }

    @ViewBuilder
    private func leaf(_ content: Content) -> some View {
        let label = node.accessibilityLabel
        switch (node.component, node.part) {
        case ("Input", "field"), ("Textarea", "field"), ("Composer", _):
            content
        case ("Image", _), ("Avatar", _), ("Video", _), ("Chart", _):
            content.accessibilityElement(children: .ignore).accessibilityLabel(label ?? "image").accessibilityAddTraits(.isImage)
        case ("Icon", _):
            if let label { content.accessibilityElement(children: .ignore).accessibilityLabel(label).accessibilityAddTraits(.isImage) } else { content.accessibilityHidden(true) }
        case ("Markdown", _):
            content.accessibilityElement(children: .ignore).accessibilityLabel(Markdown.plainText(node.props.str("text"))).accessibilityAddTraits(.isStaticText)
        case ("Skeleton", _), ("TreeGuides", _), ("List", "divider"):
            content.accessibilityHidden(true)
        case ("Text", "tab"):
            content.accessibilityElement(children: .ignore).accessibilityLabel(label ?? "").accessibilityAddTraits(node.selected ? [.isButton, .isSelected] : [.isButton])
        case ("Text", "trigger"):
            content.accessibilityElement(children: .ignore).accessibilityLabel(label ?? "").accessibilityAddTraits(.isButton).accessibilityValue(node.open ? "expanded" : "collapsed")
        case ("Checkbox", "box"), ("Switch", "track"):
            content.accessibilityElement(children: .ignore).accessibilityLabel(model.ownerProps(node.index).str("label")).accessibilityValue(model.checked(node.index) ? "on" : "off").accessibilityAddTraits(.isToggle)
        case ("Radio", "dot"):
            content.accessibilityElement(children: .ignore).accessibilityLabel(radioLabel()).accessibilityAddTraits(model.radioChecked(node.index) ? [.isButton, .isSelected] : [.isButton])
        case ("Slider", "track"):
            content.accessibilityElement(children: .ignore).accessibilityLabel(model.ownerProps(node.index).str("label")).accessibilityValue(String(Int(model.sliderValue(node.index)))).accessibilityAdjustableAction { direction in
                let step = node.props.num("step") ?? 1
                let v = SurfaceModel.snap(model.sliderValue(node.index) + (direction == .increment ? step : -step), min: node.props.num("min") ?? 0, max: node.props.num("max") ?? 100, step: step)
                model.sliderDrag(node.index, value: v)
                model.sliderRelease(node.index)
            }
        case ("Spinner", _), ("Ring", _):
            content.accessibilityElement(children: .ignore).accessibilityLabel(label ?? "Loading")
        case ("Select", "field"), ("DatePicker", "field"), ("ToggleGroup", _):
            content
        case ("Link", _):
            content.accessibilityElement(children: .ignore).accessibilityLabel(label ?? "").accessibilityAddTraits(.isLink)
        default:
            if let label {
                content.accessibilityElement(children: .ignore).accessibilityLabel(label).accessibilityAddTraits(node.pressable || node.component == "Button" || node.component == "Toggle" ? .isButton : .isStaticText)
            } else {
                content.accessibilityHidden(true)
            }
        }
    }

    private func radioLabel() -> String {
        let suffix = node.id.split(separator: ".").last.map(String.init) ?? ""
        let owner = model.owner(of: node.index)
        return model.index(of: "\(owner?.id ?? "").label.\(suffix)").flatMap { model.node($0) }?.props.str("text") ?? ""
    }
}
