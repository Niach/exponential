import SwiftUI
import ExponentialUICore

/// Interaction: presses, the OutEvent dispatch to the host, interaction
/// states, mirrors of unbound controls, overlay bookkeeping, sliders,
/// toggle groups, carousel pages and date picks. Every path ends in a
/// layout pass so the painted state follows the core's.
extension SurfaceModel {
    /// Forward the core's OutEvents to the host. `inputRevision` = the
    /// revision of a host-owned text edit (else a per-component counter).
    func dispatch(_ events: [FfiEvent], inputRevision: Int? = nil) {
        for e in events {
            let v = JSONValue.parse(e.json)
            switch e.kind {
            case "action":
                host.onAction(SurfaceActionEvent(surfaceId: id, event: v["event"]?.string ?? "", name: v["name"]?.string ?? "", componentId: v["component_id"]?.string ?? "", context: v["context"] ?? .object([:]), payload: v["payload"]))
            case "openUrl":
                host.openUrl(v["url"]?.string ?? "")
            case "functionCall":
                host.onFunctionCall(SurfaceFunctionCall(surfaceId: id, componentId: v["componentId"]?.string ?? v["component_id"]?.string ?? "", name: v["name"]?.string ?? "", args: v["args"]?.object ?? [:]))
            case "input":
                let component = v["component_id"]?.string ?? ""
                let revision: Int
                if let r = inputRevision {
                    revision = r
                } else {
                    revision = (revisions[component] ?? 0) + 1
                    revisions[component] = revision
                }
                host.onInput(SurfaceInputEvent(surfaceId: id, componentId: component, name: v["name"]?.string ?? "", path: v["path"]?.string, value: v["value"] ?? .null, revision: revision, kind: (v["commit"]?.bool ?? false) ? .commit : .change))
            default:
                break
            }
        }
        nodesDirty = true
        layoutNeeded = true
        pass()
    }

    /// Fire `event` on node `index` through the core and dispatch the result.
    public func fire(_ index: Int, _ event: String, payload: JSONValue? = nil) {
        guard let events = try? surface.event(index: UInt32(index), name: event, payloadJson: payload?.json) else { return }
        dispatch(events)
    }

    /// A mirrored control value: the local one while the prop is unchanged.
    func mirrored(_ ownerId: String, external: JSONValue) -> JSONValue {
        if let m = mirrors[ownerId], m.external == external { return m.local }
        return external
    }

    func setMirror(_ ownerId: String, external: JSONValue, local: JSONValue) {
        mirrors[ownerId] = Mirror(external: external, local: local)
        mirrorGeneration += 1
    }

    /// A full press of node `index` (a tap, Enter / Space).
    public func press(_ index: Int) {
        guard !isDisabled(index), let n = node(index) else { return }
        let owner = self.owner(of: index) ?? n
        if let target = n.triggerFor {
            if justDismissed == target {
                justDismissed = nil
                return
            }
            layerReturn[target] = n.id
            fire(index, "press")
            return
        }
        switch (n.component, n.part) {
        case ("DatePicker", "field"):
            popup = popup == n.id ? nil : n.id
            return
        case ("Select", "field"), ("Input", "field"), ("Textarea", "field"), ("Slider", "track"), ("Composer", nil), ("ToggleGroup", _):
            return
        default:
            break
        }
        switch owner.component {
        case "Checkbox", "Switch":
            let external = owner.props["checked"] ?? .bool(false)
            let current = mirrored(owner.id, external: external).bool ?? false
            setMirror(owner.id, external: external, local: .bool(!current))
            if let events = try? surface.event(index: UInt32(owner.index), name: "change", payloadJson: JSONValue.object(["checked": .bool(!current)]).json) {
                dispatch(events)
            }
            return
        case "Radio":
            let suffix = n.id.split(separator: ".").last.map(String.init) ?? ""
            if let row = byId["\(owner.id).item.\(suffix)"].flatMap({ node($0) }) {
                if let value = row.props["value"] {
                    setMirror(owner.id, external: owner.props["value"] ?? .null, local: value)
                }
                fire(row.index, "press")
                return
            }
        case "Toggle":
            let external = owner.props["pressed"] ?? .bool(false)
            let current = mirrored(owner.id, external: external).bool ?? false
            setMirror(owner.id, external: external, local: .bool(!current))
        default:
            break
        }
        fire(index, "press")
    }

    /// Press the node with this id.
    public func press(id: String) {
        if let i = byId[id] { press(i) }
    }

    // MARK: - states

    @discardableResult
    func setInteraction(_ id: String, _ change: (inout InteractionState) -> Void) -> Bool {
        var s = interaction[id] ?? InteractionState()
        let before = s
        change(&s)
        if s == before { return false }
        interaction[id] = s
        if surface.setStates(id: id, states: s.states) {
            layoutNeeded = true
            pass()
        }
        return true
    }

    /// Pointer down on a pressable: the `pressed` state.
    public func pressDown(_ id: String) {
        if let prev = pressedId, prev != id { setInteraction(prev) { $0.pressed = false } }
        if let i = byId[id], isDisabled(i) { return }
        pressedId = id
        setInteraction(id) { $0.pressed = true }
    }

    /// Pointer up: clear the state; the press itself is the button's action.
    public func pressUp(_ id: String) {
        if pressedId == id { pressedId = nil }
        setInteraction(id) { $0.pressed = false }
    }

    public func hover(_ id: String, _ hovered: Bool) {
        setInteraction(id) { $0.hover = hovered }
    }

    public func focus(_ id: String, _ focused: Bool) {
        setInteraction(id) { $0.focus = focused }
    }

    // MARK: - overlays

    /// Open or close an overlay owner (Dialog, Drawer, Popover, Tooltip, DropdownMenu).
    public func setOpen(_ owner: String, _ open: Bool) {
        dispatch(surface.setOpen(id: owner, open: open))
    }

    /// Close the overlay `owner` (a swipe, the scrim, Escape).
    public func dismissLayer(_ owner: String) {
        guard layers.contains(where: { $0.owner == owner }) else { return }
        justDismissed = owner
        dispatch(surface.setOpen(id: owner, open: false))
        // The remembered dismissal only guards the trigger's own tap.
        Task { @MainActor [weak self] in
            try? await Task.sleep(for: .milliseconds(300))
            if self?.justDismissed == owner { self?.justDismissed = nil }
        }
    }

    /// Escape: close the date popup, else the TOP layer. Returns whether
    /// something closed.
    @discardableResult
    public func escape() -> Bool {
        if popup != nil {
            popup = nil
            return true
        }
        guard let top = layers.last else { return false }
        dispatch(surface.setOpen(id: top.owner, open: false))
        return true
    }

    /// Hover on a Tooltip anchor (macOS): open after 300 ms, close on leave.
    public func tooltipHover(_ owner: String, _ hovered: Bool) {
        tooltipTask?.cancel()
        if hovered {
            tooltipTask = Task { @MainActor [weak self] in
                try? await Task.sleep(for: .milliseconds(300))
                guard !Task.isCancelled, let self else { return }
                self.dispatch(self.surface.setOpen(id: owner, open: true))
            }
        } else if layers.contains(where: { $0.owner == owner }) {
            dispatch(surface.setOpen(id: owner, open: false))
        }
    }

    /// The modal layers (Dialog / Drawer) open, in order.
    public var modalLayers: [LayerInfo] { layers.filter(\.isModal) }

    func layersChanged() {
        let now = layers.map(\.owner)
        for owner in Array(layerReturn.keys) where !now.contains(owner) {
            layerReturn[owner] = nil
        }
    }

    // MARK: - controls

    /// A ToggleGroup item press: single = that value, multiple = toggled set.
    public func toggleGroupSelect(_ index: Int, value: JSONValue) {
        guard let n = node(index), !isDisabled(index) else { return }
        let multiple = n.props.str("type") == "multiple"
        let external = n.props["value"] ?? .null
        let current = mirrored(n.id, external: external)
        let next: JSONValue
        if multiple {
            var set: [JSONValue]
            switch current {
            case let .array(a): set = a
            case let .string(s) where !s.isEmpty: set = s.split(separator: ",").map { .string(String($0)) }
            default: set = []
            }
            if let p = set.firstIndex(of: value) { set.remove(at: p) } else { set.append(value) }
            next = .array(set)
        } else {
            next = value
        }
        setMirror(n.id, external: external, local: next)
        fire(index, "change", payload: .object(["value": next]))
    }

    /// The values a ToggleGroup shows selected.
    public func toggleGroupValues(_ index: Int) -> [String] {
        guard let n = node(index) else { return [] }
        let v = mirrored(n.id, external: n.props["value"] ?? .null)
        switch v {
        case let .array(a): return a.map(\.displayText)
        case let .string(s): return s.isEmpty ? [] : s.split(separator: ",").map(String.init)
        case .null: return []
        default: return [v.displayText]
        }
    }

    /// The checked state of a Checkbox / Switch (mirror over prop).
    public func checked(_ index: Int) -> Bool {
        guard let o = owner(of: index) else { return false }
        let external = o.props["checked"] ?? .bool(false)
        return mirrored(o.id, external: external).bool ?? false
    }

    /// Is this Radio dot / row the chosen one?
    public func radioChecked(_ index: Int) -> Bool {
        guard let n = node(index), let o = owner(of: index) else { return false }
        let external = o.props["value"] ?? .null
        let value = mirrored(o.id, external: external)
        let suffix = n.id.split(separator: ".").last.map(String.init) ?? ""
        guard let row = byId["\(o.id).item.\(suffix)"].flatMap({ node($0) }), let rv = row.props["value"] else { return false }
        return rv.displayText == value.displayText
    }

    /// A Toggle's pressed state (mirror over prop).
    public func togglePressed(_ index: Int) -> Bool {
        guard let n = node(index) else { return false }
        return mirrored(n.id, external: n.props["pressed"] ?? .bool(false)).bool ?? false
    }

    /// A Select's value (mirror over prop).
    public func selectValue(_ index: Int) -> JSONValue {
        guard let o = owner(of: index) else { return .null }
        return mirrored(o.id, external: o.props["value"] ?? .null)
    }

    /// A Select pick: single = the value, multiple = the toggled set.
    public func selectPick(_ fieldIndex: Int, value: JSONValue) {
        guard let o = owner(of: fieldIndex) else { return }
        let external = o.props["value"] ?? .null
        let next: JSONValue
        if o.props.flag("multiple") {
            var set = mirrored(o.id, external: external).array ?? (mirrored(o.id, external: external).isNull ? [] : [mirrored(o.id, external: external)])
            if let p = set.firstIndex(of: value) { set.remove(at: p) } else { set.append(value) }
            next = .array(set)
        } else {
            next = value
        }
        setMirror(o.id, external: external, local: next)
        fire(fieldIndex, "change", payload: .object(["value": next]))
    }

    /// A DatePicker's ISO value (mirror over prop).
    public func dateValue(_ index: Int) -> String {
        guard let o = owner(of: index) else { return "" }
        return mirrored(o.id, external: o.props["value"] ?? .null).displayText
    }

    /// A DatePicker day pick.
    public func pickDate(_ fieldIndex: Int, iso: String) {
        guard let o = owner(of: fieldIndex) else { return }
        setMirror(o.id, external: o.props["value"] ?? .null, local: .string(iso))
        popup = nil
        fire(fieldIndex, "change", payload: .object(["value": .string(iso)]))
    }

    /// The value a slider track shows (drag > mirror > prop > min).
    public func sliderValue(_ index: Int) -> Double {
        guard let n = node(index) else { return 0 }
        if let d = drags[n.id] { return d }
        let o = owner(of: index) ?? n
        let external = n.props["value"] ?? .null
        return mirrored(o.id, external: external).number ?? n.props.num("min") ?? 0
    }

    public func sliderDrag(_ index: Int, value: Double) {
        guard let n = node(index), !isDisabled(index) else { return }
        if drags[n.id] != value {
            drags[n.id] = value
            mirrorGeneration += 1
        }
    }

    public func sliderRelease(_ index: Int) {
        guard let n = node(index), let value = drags.removeValue(forKey: n.id) else { return }
        let o = owner(of: index) ?? n
        setMirror(o.id, external: n.props["value"] ?? .null, local: .number(value))
        fire(index, "change", payload: .object(["value": .number(value)]))
    }

    /// `v` snapped to `min + k·step` and clamped.
    public static func snap(_ v: Double, min: Double, max: Double, step: Double) -> Double {
        var x = step > 0 ? min + ((v - min) / step).rounded() * step : v
        x = Swift.min(Swift.max(x, Swift.min(min, max)), Swift.max(min, max))
        return (x * 1e9).rounded() / 1e9
    }

    /// A Carousel page change (`index` = the carousel or its indicator).
    public func carouselPage(_ index: Int, page: Int) {
        guard let o = owner(of: index) else { return }
        fire(o.index, "change", payload: .object(["page": .number(Double(page))]))
    }

    /// Scroll a windowed list to a content offset (relayouts only when the
    /// offset moved by at least a point).
    public func scroll(list id: String, offset: CGFloat) {
        if surface.scroll(listId: id, offset: Float(offset)) {
            // The window's rows change without a structure-version bump.
            nodesDirty = true
            layoutNeeded = true
            pass()
        }
    }
}
