import SwiftUI
import ExponentialUICore

/// One placed node of the surface (the facade's `FfiNode`, props parsed):
/// static per structure version, re-read after every interaction (the
/// core may re-resolve props without a version bump).
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
    public let layer: Int
    public let isLeaf: Bool
    public let props: Props
    public let lines: Int?
    public let pressable: Bool
    public let hidden: Bool
    public let triggerFor: String?
    public let accessibility: Props?
    /// Part states the core resolved (`selected`, `open`, `checked`).
    public let partStates: [String]
    public let macroName: String?

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
        parent = n.parent.map { Int($0) }
        layer = Int(n.layer)
        isLeaf = n.isLeaf
        props = JSONValue.parse(n.propsJson).object ?? [:]
        lines = n.lines.map { Int($0) }
        pressable = n.pressable
        hidden = n.hidden
        triggerFor = n.triggerFor
        accessibility = n.accessibilityJson.flatMap { JSONValue.parse($0).object }
        partStates = n.partStates
        macroName = n.macroName
    }

    public var isContainer: Bool { !isLeaf }
    /// The component whose recipe this node paints with (the owner of a part).
    public var recipeComponent: String { ownerComponent ?? component }
    public var selected: Bool { partStates.contains("selected") }
    public var open: Bool { partStates.contains("open") }
    public var checked: Bool { partStates.contains("checked") }
    public var disabled: Bool { props.flag("disabled") }

    /// Is this a host-owned text field (`Input`/`Textarea` `.field`, `Composer`)?
    public var isTextField: Bool {
        switch (component, part) {
        case ("Input", "field"), ("Textarea", "field"), ("Composer", nil): true
        default: false
        }
    }

    /// Keyboard-focusable: pressables, controls, fields, carousel dots.
    public var isFocusable: Bool {
        if hidden || disabled { return false }
        if pressable { return true }
        switch (component, part) {
        case ("Button", _), ("Link", _), ("Toggle", _), ("ToggleGroup", _), ("Composer", _), ("Box", "indicator"): return true
        default: return false
        }
    }

    /// The accessible name: `accessibility.label`, else text/label/alt.
    public var accessibilityLabel: String? {
        if let l = accessibility?["label"]?.string, !l.isEmpty { return l }
        func s(_ k: String) -> String? { let v = props.str(k); return v.isEmpty ? nil : v }
        switch component {
        case "Image", "Video": return s("alt") ?? s("title")
        case "Avatar": return s("name")
        case "Box", "Extension": return s("label") ?? s("title")
        case "Spinner": return s("label") ?? "Loading"
        case "Link": return s("label") ?? s("href")
        case "Chart": return s("title") ?? s("kind")
        case "Markdown": return props.str("text").isEmpty ? nil : props.str("text")
        default: return s("text") ?? s("label") ?? s("title") ?? s("placeholder") ?? s("alt")
        }
    }
}

/// An open overlay layer (the facade's `FfiLayer`).
public struct LayerInfo: Identifiable, Sendable, Equatable {
    public var id: String { owner }
    public let layer: Int
    /// `Dialog` | `Drawer` | `Popover` | `Tooltip` | `DropdownMenu`.
    public let kind: String
    public let owner: String
    public let root: Int
    public let anchorFrame: CGRect?
    public let placementSide: String?
    public let flipped: Bool
    /// `centered`, a viewport edge, or the side an anchored layer landed on.
    public let position: String
    public let frame: CGRect

    init(_ l: FfiLayer) {
        layer = Int(l.layer)
        kind = l.kind
        owner = l.owner
        root = Int(l.root)
        anchorFrame = l.anchorFrame.map { CGRect($0) }
        placementSide = l.placement?.side
        flipped = l.placement?.flipped ?? false
        position = l.position
        frame = l.frames.first { $0.index == l.root }.map { CGRect($0) } ?? .zero
    }

    public var isModal: Bool { kind == "Dialog" || kind == "Drawer" }
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

    init(_ l: FfiList) {
        id = l.id
        node = Int(l.node)
        contentHeight = CGFloat(l.contentHeight)
        start = Int(l.start)
        end = Int(l.end)
        count = Int(l.count)
        windowed = l.windowed
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
}

/// The interaction states of one node (reported to the core as `states`).
struct InteractionState: Equatable {
    var hover = false
    var pressed = false
    var focus = false

    var states: [String] {
        var s: [String] = []
        if hover { s.append("hover") }
        if pressed { s.append("pressed") }
        if focus { s.append("focus") }
        return s
    }
}

/// A local mirror of an UNBOUND control value (the core reflects only bound
/// ones): the local value stands while the prop keeps its external value.
struct Mirror {
    var external: JSONValue
    var local: JSONValue
}
