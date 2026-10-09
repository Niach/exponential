import SwiftUI
import ExponentialUICore
import ExponentialUIPrimitives

/// A node's accessibility as VoiceOver gets it (contract §6): the ROLE
/// (the node's `accessibility.role`, else the component's `a11y.json` role
/// for a main object, else the part's), the name, value, hint, traits and
/// the adjustable kind. Pure over the model, so the mapping is testable
/// without a view.
@MainActor
struct A11yInfo: Equatable {
    /// One of `a11y.json` `roles` (ARIA names + `text` / `hidden`), or nil.
    var role: String?
    var label: String?
    var value: String?
    var hint: String?
    var traits: AccessibilityTraits = []
    var heading: AccessibilityHeadingLevel?
    var hidden = false
    /// The node keeps its own element (a host text field, a Segmented).
    var passthrough = false
    /// A container the reader walks into (`.contain`), else one element.
    var container = false
    /// `slider` | `spinbutton`: increment / decrement.
    var adjustable: String?
    var autoFocus = false
    /// A live region (`Text.live`, a Toast): reads updates.
    var live = false

    /// The `a11y.json` role of a component's main object (cached; the core
    /// embeds the catalog: `componentA11yJson`).
    static func componentRole(_ component: String) -> String? {
        if let hit = roleCache[component] { return hit }
        let role = componentA11yJson(component: component).flatMap { JSONValue.parse($0)["role"]?.string }
        roleCache[component] = role
        return role
    }

    private static var roleCache: [String: String?] = [:]

    /// The role of a node: the expanded tree's `accessibility.role` (macro
    /// parts carry theirs), else the component's for a non-part node (a
    /// macro root reads its macro's), else the part's semantics (the gpui
    /// painter's `role_of`, in ARIA names).
    static func role(of n: NodeInfo, model: SurfaceModel) -> String? {
        if let r = n.a11y.role, !r.isEmpty { return r }
        let owner = n.ownerComponent ?? ""
        switch (n.component, n.part) {
        case ("Text", "tab"): return "tab"
        case ("Box", "list") where owner == "Tabs": return "tablist"
        case ("Text", "trigger"): return "button"
        case ("Select", "trigger"), ("TimePicker", "trigger"), ("DatePicker", "trigger"), ("DateRangePicker", "trigger"): return "combobox"
        case ("Select", "search"): return "textbox"
        case ("Box", "list") where owner == "Select" || owner == "TimePicker": return "listbox"
        case ("Text", "item") where owner == "Select" || owner == "TimePicker": return "option"
        case ("Box", "content") where owner == "Menu": return "menu"
        case ("Box", "item") where owner == "Menu": return n.props.str("kind") == "checkbox" ? "menuitemcheckbox" : "menuitem"
        case ("Text", "item") where owner == "Menu": return "menuitem"
        case ("Box", "content") where owner == "Tooltip": return "tooltip"
        case ("Box", "content") where owner == "Dialog" || owner == "Drawer" || owner == "Popover":
            return model.ownerProps(n.index)["dismissible"]?.bool == false ? "alertdialog" : "dialog"
        case ("Box", "root") where owner == "Toast": return n.props.str("live") == "assertive" || model.ownerProps(n.index).str("live") == "assertive" ? "alert" : "status"
        case ("Text", "headerCell"), ("Text", "weekday"): return "columnheader"
        case ("Text", "day"): return "gridcell"
        case ("Box", "dropzone"): return "button"
        case ("Checkbox", "box"), ("Checkbox", "checkbox"): return "checkbox"
        case ("Switch", "track"): return "switch"
        case ("Radio", "dot"): return "radio"
        case ("Slider", "track"): return "slider"
        case ("NumberField", "input"): return "spinbutton"
        case ("ChipInput", "input"), ("Input", "field"), ("Textarea", "field"): return "textbox"
        case ("Box", "indicator"): return "tablist"
        case ("List", "divider"): return "hidden"
        case ("Box", _) where n.pressable: return "button"
        default: break
        }
        if n.part == nil {
            let main = n.macroName ?? n.component
            if main == "Text", n.props["live"] != nil { return "status" }
            if let r = componentRole(main), r != "none" || n.macroName != nil { return r }
            if n.component == "Box", n.macroName == nil { return nil }
        }
        return nil
    }

    /// Everything VoiceOver reads for node `n`.
    static func of(_ n: NodeInfo, model: SurfaceModel, style: PaintStyle) -> A11yInfo {
        var info = A11yInfo()
        let a = n.a11y
        let role = role(of: n, model: model)
        info.role = role
        info.autoFocus = a.autoFocus
        if style.invisible || a.hidden || role == "hidden" || role == "separator" {
            info.hidden = true
            return info
        }
        switch (n.component, n.part) {
        case ("Skeleton", _), ("TreeGuides", _):
            info.hidden = true
            return info
        case ("Input", "field"), ("Textarea", "field"), ("Composer", _), ("Segmented", _), ("ChipInput", "input"), ("Select", "search"):
            info.passthrough = true
            return info
        case ("NumberField", "input"):
            info.passthrough = true
            info.adjustable = "spinbutton"
            return info
        default:
            break
        }
        let label = Self.label(n, model: model, role: role)
        info.label = label
        info.hint = Self.hint(n, model: model)
        info.live = n.props["live"] != nil || n.ownerComponent == "Toast"
        if let level = a.level ?? (role == "heading" ? n.props.num("level").map { Int($0) } : nil) {
            info.heading = Self.headingLevel(level)
        }
        info.traits = Self.traits(role: role)
        if info.live { info.traits.formUnion(.updatesFrequently) }
        // States (`accessibility` first, then the part states the core set).
        let selected = a.selected ?? (n.selected || ((n.part == "dot") && model.radioChecked(n.index)))
        if selected { info.traits.formUnion(.isSelected) }
        var values: [String] = []
        if let current = a.current {
            info.traits.formUnion(.isSelected)
            values.append(current == "step" ? "current step" : "current page")
        }
        if let pressed = a.pressed {
            info.traits.formUnion(.isToggle)
            if pressed { info.traits.formUnion(.isSelected) }
        }
        if let expanded = a.expanded ?? (role == "combobox" || n.part == "trigger" ? n.open : nil) {
            values.append(expanded ? "expanded" : "collapsed")
        }
        switch role {
        case "checkbox", "switch", "menuitemcheckbox":
            let checked = a.checked ?? (n.part == nil && n.component != "Checkbox" && n.component != "Switch" ? n.checked : model.checked(n.index))
            values.append(checked ? "on" : "off")
        case "progressbar", "meter", "slider":
            let now = a.valueNow ?? (role == "slider" ? model.sliderValue(n.index) : n.props.num("value"))
            let lo = a.valueMin ?? n.props.num("min") ?? 0
            let hi = a.valueMax ?? n.props.num("max") ?? 100
            if let now {
                if role == "progressbar", hi > lo {
                    values.append("\(Int(((now - lo) / (hi - lo) * 100).rounded()))%")
                } else {
                    values.append(JSONValue.number(now).displayText)
                }
            }
            if role == "slider" { info.adjustable = "slider" }
        default:
            if let c = a.checked { values.append(c ? "on" : "off"); info.traits.formUnion(.isToggle) }
        }
        info.value = values.isEmpty ? nil : values.joined(separator: ", ")
        // Containers keep their children as elements; a pressable
        // container, and a macro part whose `$a11y` names an ATOMIC role
        // (a Progress bar, a Rating star, a Heading), is ONE element named
        // by its leaves.
        if !n.isLeaf && !n.pressable {
            if a.role != nil, let role, Self.atomicRoles.contains(role) {
                if info.label == nil || info.label?.isEmpty == true {
                    let combined = model.combinedLabel(n.index)
                    info.label = combined.isEmpty ? nil : combined
                }
            } else {
                info.container = true
                // A native's own object (a Checkbox row, a Slider) lives on
                // its part; the container only groups and names.
                info.traits = info.traits.intersection([.isModal, .isHeader])
                info.value = nil
            }
        }
        if !info.container && info.label == nil && info.value == nil {
            // A leaf with nothing to read (a decorative icon).
            info.hidden = n.isLeaf && !n.pressable
        }
        return info
    }

    /// Roles read as one object even when the node has children.
    static let atomicRoles: Set<String> = ["progressbar", "meter", "slider", "img", "button", "link", "checkbox", "switch", "radio", "tab", "menuitem", "menuitemcheckbox", "option", "heading"]

    static func traits(role: String?) -> AccessibilityTraits {
        switch role {
        case "button", "menuitem", "option", "combobox", "radio", "gridcell": return .isButton
        case "tab": return .isButton
        case "checkbox", "switch", "menuitemcheckbox": return [.isButton, .isToggle]
        case "link": return .isLink
        case "heading": return .isHeader
        case "img": return .isImage
        case "text", "status", "alert", "tooltip", "columnheader": return .isStaticText
        case "dialog", "alertdialog": return .isModal
        case "textbox", "spinbutton": return []
        default: return []
        }
    }

    static func headingLevel(_ l: Int) -> AccessibilityHeadingLevel {
        switch l {
        case 1: .h1
        case 2: .h2
        case 3: .h3
        case 4: .h4
        case 5: .h5
        case 6: .h6
        default: .unspecified
        }
    }

    /// The accessible name: the author's `accessibility.label`, a pressable
    /// container's leaves, a control's owner label, else the node's own.
    static func label(_ n: NodeInfo, model: SurfaceModel, role: String?) -> String? {
        if let l = n.a11y.label, !l.isEmpty { return l }
        if !n.isLeaf {
            if n.pressable { return model.combinedLabel(n.index) }
            // A macro root reads its title (a Card / Group / Alert).
            if n.macroName != nil || n.part == "content" {
                return model.macroTitle(n.index) ?? n.accessibilityLabel
            }
            return n.accessibilityLabel
        }
        switch (n.component, n.part) {
        case ("Checkbox", "box"), ("Switch", "track"), ("Slider", "track"):
            return model.ownerProps(n.index).str("label")
        case ("Radio", "dot"):
            let suffix = n.id.split(separator: ".").last.map(String.init) ?? ""
            let owner = model.owner(of: n.index)
            return model.index(of: "\(owner?.id ?? "").label.\(suffix)").flatMap { model.node($0) }?.props.str("text")
        case ("Markdown", _):
            let t = Markdown.plainText(n.props.str("text"))
            return t.isEmpty ? nil : t
        case ("Icon", _):
            return n.props["label"]?.string
        case ("Image", _), ("Avatar", _), ("Video", _), ("Chart", _):
            return n.accessibilityLabel ?? "image"
        case ("Spinner", _), ("Ring", _):
            return n.accessibilityLabel ?? model.builtinString("loading")
        default:
            return n.accessibilityLabel
        }
    }

    /// The description and the linked error (`<id>.error`, contract §6).
    static func hint(_ n: NodeInfo, model: SurfaceModel) -> String? {
        var parts: [String] = []
        if let d = n.a11y.description, !d.isEmpty { parts.append(d) }
        let ids = [n.id, n.owner].compactMap { $0 }
        for id in ids {
            if let e = model.index(of: "\(id).error").flatMap({ model.node($0) }), !e.hidden {
                let t = e.props.str("text")
                if !t.isEmpty, !parts.contains(t) { parts.append(t) }
            }
        }
        return parts.isEmpty ? nil : parts.joined(separator: ". ")
    }
}

extension SurfaceModel {
    /// A macro root's title part text (its `.title` node), the name a
    /// Card / Group / Alert reads.
    func macroTitle(_ index: Int) -> String? {
        guard let n = node(index) else { return nil }
        for suffix in ["title", "heading"] {
            if let t = self.index(of: "\(n.id).\(suffix)").flatMap({ node($0) }), !t.hidden {
                let s = t.props.str("text")
                if !s.isEmpty { return s }
            }
        }
        return nil
    }
}

/// The accessibility shape of a node: VoiceOver reads in paint order (sort
/// priorities on the children), containers contain, pressables combine
/// into one element, leaves carry their name, value, hint and traits; the
/// model's focus requests (a dialog's `autoFocus` node, a Form's first
/// invalid field, a `focus` command, a keyboard move) move VoiceOver focus.
struct AccessibilityModifier: ViewModifier {
    let node: NodeInfo
    let model: SurfaceModel
    let style: PaintStyle

    func body(content: Content) -> some View {
        let info = A11yInfo.of(node, model: model, style: style)
        shaped(content, info)
            .modifier(Adjustable(kind: info.adjustable, node: node, model: model))
            .modifier(AccessibilityFocus(on: !info.hidden && !info.passthrough && (node.isFocusable || info.autoFocus), node: node, model: model))
    }

    @ViewBuilder
    private func shaped(_ content: Content, _ info: A11yInfo) -> some View {
        if info.hidden {
            content.accessibilityHidden(true)
        } else if info.passthrough {
            content
        } else if info.container {
            content
                .accessibilityElement(children: .contain)
                .accessibilityLabel(Text(info.label ?? ""))
                .accessibilityAddTraits(info.traits)
                .modifier(Heading(level: info.heading))
        } else {
            content
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(Text(info.label ?? ""))
                .accessibilityValue(Text(info.value ?? ""))
                .accessibilityHint(Text(info.hint ?? ""))
                .accessibilityAddTraits(info.traits)
                .modifier(Heading(level: info.heading))
        }
    }
}

private struct Heading: ViewModifier {
    let level: AccessibilityHeadingLevel?

    func body(content: Content) -> some View {
        if let level {
            content.accessibilityHeading(level).accessibilityAddTraits(.isHeader)
        } else {
            content
        }
    }
}

/// Increment / decrement for a Slider track (the step, snapped and
/// clamped) or a NumberField (its stepper parts through the core).
private struct Adjustable: ViewModifier {
    let kind: String?
    let node: NodeInfo
    let model: SurfaceModel

    func body(content: Content) -> some View {
        switch kind {
        case "slider":
            content.accessibilityAdjustableAction { direction in
                let step = node.props.num("step") ?? 1
                let v = SurfaceModel.snap(model.sliderValue(node.index) + (direction == .increment ? step : -step), min: node.props.num("min") ?? 0, max: node.props.num("max") ?? 100, step: step)
                model.sliderDrag(node.index, value: v)
                model.sliderRelease(node.index)
            }
        case "spinbutton":
            content.accessibilityAdjustableAction { direction in
                model.fieldKey(node.index, key: direction == .increment ? "up" : "down")
            }
        default:
            content
        }
    }
}

/// `@AccessibilityFocusState` bound to the model's `focusRequest`
/// (contract §6, §9 SwiftUI 6): VoiceOver focus follows every request that
/// names this node, also one made before the node appeared (a layer's
/// `autoFocus` node when the dialog opens).
private struct AccessibilityFocus: ViewModifier {
    let on: Bool
    let node: NodeInfo
    let model: SurfaceModel
    @AccessibilityFocusState private var focused: Bool

    func body(content: Content) -> some View {
        if on {
            content
                .accessibilityFocused($focused)
                .onChange(of: model.focusRequest) { _, r in
                    if r?.index == node.index { focused = true }
                }
                .onAppear {
                    guard let r = model.focusRequest, r.index == node.index, model.focusedId == r.id else { return }
                    // After the layer's presentation settles.
                    DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) { focused = true }
                }
        } else {
            content
        }
    }
}
