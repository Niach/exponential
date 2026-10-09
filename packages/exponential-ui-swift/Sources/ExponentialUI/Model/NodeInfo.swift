import SwiftUI
import ExponentialUICore

/// One placed node of the surface (the facade's `FfiNode`, props parsed).
/// Indices are STABLE SLOTS: `nodes[i].index == i`; a removed node stays a
/// tombstone (`removed`, hidden, out of the id lookup) until the core
/// compacts (`delta.renumbered`). Patched per pass from the layout's node
/// delta (the core re-resolves props without a structure-version bump).
public struct NodeInfo: Identifiable, Sendable {
    public let index: Int
    public let id: String
    public let component: String
    public let part: String?
    public let owner: String?
    public let ownerComponent: String?
    public let catalogId: String?
    public let extensionKind: String?
    public let depth: Int
    public let parent: Int?
    /// The children in PAINT order (= accessibility order).
    public let children: [Int]
    public let layer: Int
    public let isLeaf: Bool
    public let props: Props
    public let lines: Int?
    public let pressable: Bool
    /// Not painted: hidden by the core, or a removed slot.
    public let hidden: Bool
    public let triggerFor: String?
    public let accessibility: Props?
    /// The parsed `accessibility` (macro `$a11y` + the author's override).
    public let a11y: NodeAccessibility
    /// Every state the core decided for the node (`selected`, `open`,
    /// `checked`, `invalid`, `disabled`): the part query's and its own.
    public let states: [String]
    /// The states the core resolved into the PART recipe query only (a tab
    /// `selected`, an accordion trigger `open`, a check part `checked`).
    public let partStates: [String]
    /// The enclosing Form's id.
    public let form: String?
    /// A live region: `polite | assertive` (nil = none).
    public let live: String?
    /// A freed slot (a tombstone).
    public let removed: Bool
    public let macroName: String?
    /// The node restyles under the pointer (round 2 `FfiNode.hoverStyled`):
    /// the painter tracks a mouse over it and reports the hover.
    public let hoverStyled: Bool
    /// The core holds the pointer's `hover` on it (`FfiNode.hovered`).
    public let hovered: Bool

    init(_ n: FfiNode) {
        index = Int(n.index)
        id = n.id
        component = n.component
        part = n.part
        owner = n.owner
        ownerComponent = n.ownerComponent
        catalogId = n.catalogId
        extensionKind = n.extensionKind
        depth = Int(n.depth)
        parent = n.removed ? nil : n.parent.map { Int($0) }
        children = n.removed ? [] : n.children.map { Int($0) }
        layer = Int(n.layer)
        isLeaf = n.isLeaf
        props = JSONValue.parse(n.propsJson).object ?? [:]
        lines = n.lines.map { Int($0) }
        pressable = n.pressable && !n.removed
        hidden = n.hidden || n.removed
        triggerFor = n.triggerFor
        let a = n.accessibilityJson.flatMap { JSONValue.parse($0).object }
        accessibility = a
        a11y = NodeAccessibility(a)
        states = n.states
        partStates = n.partStates
        form = n.form
        live = n.live
        removed = n.removed
        macroName = n.macroName
        hoverStyled = n.hoverStyled && !n.removed
        hovered = n.hovered
    }

    public var isContainer: Bool { !isLeaf }
    /// The component whose recipe this node paints with (the owner of a part).
    public var recipeComponent: String { ownerComponent ?? component }
    /// A state the core decided (node states or part states).
    public func has(_ state: String) -> Bool { states.contains(state) || partStates.contains(state) }
    public var selected: Bool { has("selected") }
    public var open: Bool { has("open") }
    public var checked: Bool { has("checked") }
    /// A field whose checks failed (a Form submit, `validateOn`).
    public var invalid: Bool { has("invalid") }
    public var disabled: Bool { props.flag("disabled") || states.contains("disabled") }

    /// Is this a host-owned text field (`Input`/`Textarea` `.field`, the
    /// `Composer`, the NumberField / ChipInput `input`, the Select `search`)?
    public var isTextField: Bool {
        switch (component, part) {
        case ("Input", "field"), ("Textarea", "field"), ("Composer", nil), ("NumberField", "input"), ("ChipInput", "input"), ("Select", "search"): true
        default: false
        }
    }

    /// A single-line host field: Enter submits (Textarea / Composer keep it).
    public var isSingleLineField: Bool { isTextField && component != "Textarea" && component != "Composer" }

    /// A Chart drawn as a bare sparkline (no axes, no tooltip, no keys).
    public var isSparkline: Bool { component == "Chart" && props.str("kind") == "sparkline" }

    /// Keyboard-focusable (gpui `is_focusable`): pressables, controls,
    /// fields, carousel dots, a Radio's dot (not its row or label), a
    /// Checkbox / Switch root, a pressable Table row, a non-sparkline Chart.
    public var isFocusable: Bool {
        if removed || hidden || disabled { return false }
        let o = ownerComponent ?? ""
        switch (component, part) {
        case ("Box", "item"), ("Box", "label"), ("Text", "item"), ("Text", "label"):
            if o == "Radio" || part == "label" && component == "Text" { return false }
        case ("Checkbox", "box"), ("Checkbox", "track"), ("Switch", "box"), ("Switch", "track"):
            return false
        case ("Box", "row") where o == "Table":
            return pressable
        default:
            break
        }
        if isTextField { return true }
        switch (component, part) {
        case ("Slider", "track"), ("Box", "indicator"), ("Segmented", _), ("Composer", _): return true
        case ("Chart", nil): return !isSparkline
        case ("Button", _), ("Link", _), ("Toggle", _): return true
        default: return pressable
        }
    }

    /// The accessible name: `accessibility.label`, else text/label/alt.
    public var accessibilityLabel: String? {
        if let l = accessibility?["label"]?.string, !l.isEmpty { return l }
        func s(_ k: String) -> String? { let v = props.text(k); return v.isEmpty ? nil : v }
        switch component {
        case "Image", "Video": return s("alt") ?? s("title")
        case "Avatar": return s("name")
        case "Box", "Extension": return s("label") ?? s("title") ?? s("text")
        case "Spinner": return s("label") ?? "Loading"
        case "Link": return s("label") ?? s("href")
        case "Chart": return s("title") ?? s("kind")
        case "Icon": return s("label")
        case "Markdown": return props.str("text").isEmpty ? nil : props.str("text")
        default: return s("text") ?? s("label") ?? s("title") ?? s("placeholder") ?? s("alt")
        }
    }
}

/// A node's `accessibility` (the macro parts' `$a11y` plus the author's
/// override; round-1 contract §6), typed for the platform mapping
/// (traits, `accessibilityValue`, headings, `autoFocus`).
public struct NodeAccessibility: Sendable, Equatable {
    /// One of `catalog/a11y.json` `roles` (`heading`, `button`, `hidden`…).
    public var role: String?
    public var label: String?
    public var description: String?
    /// The heading level (1–6).
    public var level: Int?
    public var expanded: Bool?
    /// `page` | `step` (nil = not current).
    public var current: String?
    public var selected: Bool?
    public var pressed: Bool?
    public var checked: Bool?
    public var valueNow: Double?
    public var valueMin: Double?
    public var valueMax: Double?
    /// Out of the accessibility tree (`hidden: true` or role `hidden`).
    public var hidden: Bool
    /// Focus lands here when the enclosing dialog opens.
    public var autoFocus: Bool

    init(_ a: Props?) {
        let a = a ?? [:]
        role = a["role"]?.string
        label = a["label"]?.string
        description = a["description"]?.string
        level = a["level"]?.number.map { Int($0) }
        expanded = a["expanded"]?.bool
        switch a["current"] {
        case let .string(s)?: current = s.isEmpty || s == "false" ? nil : s
        case let .bool(b)?: current = b ? "page" : nil
        default: current = nil
        }
        selected = a["selected"]?.bool
        pressed = a["pressed"]?.bool
        checked = a["checked"]?.bool
        valueNow = a["valueNow"]?.number
        valueMin = a["valueMin"]?.number
        valueMax = a["valueMax"]?.number
        hidden = a["hidden"]?.bool ?? (role == "hidden")
        autoFocus = a["autoFocus"]?.bool ?? false
    }
}

/// An open overlay layer (the facade's `FfiLayer`).
public struct LayerInfo: Identifiable, Sendable, Equatable {
    public var id: String { owner }
    public let layer: Int
    /// `Dialog` | `Drawer` | `Popover` | `Tooltip` | `Menu` |
    /// `Select` | `DatePicker` | `Toast`…
    public let kind: String
    public let owner: String
    public let root: Int
    public let anchorFrame: CGRect?
    public let placementSide: String?
    public let flipped: Bool
    /// `centered`, a viewport edge, the side an anchored layer landed on,
    /// `point` (a context menu) or `toast`.
    public let position: String
    /// `overlay` | `toast` (layers stack base < overlay < toast).
    public let layerClass: String
    /// A scrim under it and the focus trapped inside (Dialog, Drawer).
    public let modal: Bool
    /// Escape / a scrim press / a drag closes it (`dismiss` on its root).
    public let dismissible: Bool
    /// The layer root's frame (surface coordinates).
    public let frame: CGRect
    /// The layer's nodes in paint order (root first).
    public let order: [Int]

    init(_ l: FfiLayer) {
        layer = Int(l.layer)
        kind = l.kind
        owner = l.owner
        root = Int(l.root)
        anchorFrame = l.anchorFrame.map { CGRect($0) }
        placementSide = l.placement?.side
        flipped = l.placement?.flipped ?? false
        position = l.position
        layerClass = l.class
        modal = l.modal
        dismissible = l.dismissible
        frame = l.frames.first { $0.index == l.root }.map { CGRect($0) } ?? .zero
        order = l.frames.map { Int($0.index) }
    }

    /// Scrim + focus trap (the core's `modal`: Dialog, Drawer…).
    public var isModal: Bool { modal }
    /// The TOAST class (above every overlay).
    public var isToast: Bool { layerClass == "toast" }
    /// Takes focus and Escape: an overlay that is not a tooltip.
    public var isInteractive: Bool { layerClass == "overlay" && kind != "Tooltip" }
}

/// A scroll container (the facade's `FfiScroll`): the core's clamped
/// offset and the scrollable content size. Frames are UNSCROLLED: a painter
/// translates the descendants by `-offset` and clips to the frame (or lets a
/// native scroll view move them and reports the offset back).
public struct ScrollInfo: Sendable, Equatable {
    public let index: Int
    public let offset: CGPoint
    public let contentSize: CGSize
    public let scrollsX: Bool
    public let scrollsY: Bool

    init(_ s: FfiScroll) {
        index = Int(s.index)
        offset = CGPoint(x: CGFloat(s.offsetX), y: CGFloat(s.offsetY))
        contentSize = CGSize(width: CGFloat(s.contentWidth), height: CGFloat(s.contentHeight))
        scrollsX = s.scrollX
        scrollsY = s.scrollY
    }
}

/// An open toast the model times (`durationMs` 0 = sticky).
public struct ToastInfo: Sendable, Equatable, Identifiable {
    /// The Toast node's id.
    public let id: String
    public let durationMs: Double
    /// `info | success | warning | error`.
    public let kind: String

    init(_ t: FfiToast) {
        id = t.id
        durationMs = t.durationMs
        kind = t.kind
    }
}

/// A windowed list (the facade's `FfiList`).
public struct ListInfo: Sendable, Equatable {
    public let id: String
    public let node: Int
    public let contentHeight: CGFloat
    public let start: Int
    public let end: Int
    public let count: Int
    public let windowed: Bool
    /// Round 2: the list windows on x (`contentHeight` = the content WIDTH).
    public let horizontal: Bool
    /// The content extent along the list's axis.
    public var contentExtent: CGFloat { contentHeight }

    init(_ l: FfiList) {
        id = l.id
        node = Int(l.node)
        contentHeight = CGFloat(l.contentHeight)
        start = Int(l.start)
        end = Int(l.end)
        count = Int(l.count)
        windowed = l.windowed
        horizontal = l.horizontal
    }
}

/// What one layout pass cost.
public struct PassStats: Sendable, Equatable {
    public var layoutNs: UInt64 = 0
    public var wallNs: UInt64 = 0
    public var upcalls: Int = 0
    public var measureRounds: Int = 0
    public var measureCalls: Int = 0
    public var nodes: Int = 0
    /// Nodes the core restyled / built this pass; whether it rebuilt.
    public var restyled: Int = 0
    public var builtNodes: Int = 0
    public var rebuilt = false
}

/// The interaction states of one node, reported to the core as `states`
/// with the names its conditions and recipes read: `hover`, `pressed`,
/// `focus`, `focus-visible` (focus that came from the keyboard), `dragover`.
struct InteractionState: Equatable {
    var hover = false
    var pressed = false
    var focus = false
    var focusVisible = false
    var dragover = false

    var states: [String] {
        var s: [String] = []
        if hover { s.append("hover") }
        if pressed { s.append("pressed") }
        if focus { s.append("focus") }
        if focusVisible { s.append("focus-visible") }
        if dragover { s.append("dragover") }
        return s
    }
}
