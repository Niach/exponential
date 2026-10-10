import SwiftUI
import ExponentialUICore
import UniformTypeIdentifiers
#if canImport(UIKit)
import UIKit
#elseif canImport(AppKit)
import AppKit
#endif

/// The tooltip / hover-card open delay (the platform's hover delay).
public let tooltipDelay: Duration = .milliseconds(300)
/// The pointer's travel time from a hover trigger to its card when the core
/// closes at once (`hoverCloseMs` 0).
let hoverGrace: Duration = .milliseconds(120)
/// How long a CodeBlock shows `copied`.
let copiedReset: Duration = .milliseconds(2000)

/// Interaction (gpui `view/events.rs`): presses, the OutEvent dispatch to
/// the host (actions, urls, host functions, input edits, focus, announce,
/// copy, file picks, hover timers), interaction states, overlay
/// bookkeeping, controls, scroll, host commands. Control VALUES are the
/// core's: a press / `change` goes in, the props come back on the next
/// pass (bound ones through the data model, unbound ones from the core's
/// local state). Every path ends in a layout pass.
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
                openLink(v["url"]?.string ?? "")
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
            case "focus":
                // A Form's first invalid field, a `focus` command: keyboard
                // focus (the ring shows), like gpui.
                if let target = v["id"]?.string { requestFocus(target, keyboard: true) }
            case "announce":
                announce(v["text"]?.string ?? "", live: v["live"]?.string ?? "polite")
            case "copy":
                copyToPasteboard(v["text"]?.string ?? "")
            case "pickFiles":
                pickFiles(componentId: v["component_id"]?.string ?? "", accept: v["accept"]?.string, multiple: v["multiple"]?.bool ?? false)
            case "hoverTimer":
                scheduleHoverTimeout(owner: v["owner"]?.string ?? "", delayMs: v["delay_ms"]?.number ?? 0)
            default:
                // `dataChanged` (the core already wrote it), `relayout`.
                break
            }
        }
        layoutNeeded = true
        pass()
    }

    /// Fire `event` on node `index` through the core and dispatch the result.
    public func fire(_ index: Int, _ event: String, payload: JSONValue? = nil) {
        guard let events = try? surface.event(index: UInt32(index), name: event, payloadJson: payload?.json) else { return }
        let copied = events.contains { $0.kind == "copy" }
        dispatch(events)
        if copied { scheduleCopyReset(index) }
    }

    /// Fire `event` on the node with this id.
    public func fire(id: String, _ event: String, payload: JSONValue? = nil) {
        if let i = byId[id] { fire(i, event, payload: payload) }
    }

    /// A CodeBlock copy button shows `copied`, then resets.
    private func scheduleCopyReset(_ index: Int) {
        guard let nid = node(index)?.id else { return }
        copyResets[nid]?.cancel()
        copyResets[nid] = Task { @MainActor [weak self] in
            try? await Task.sleep(for: copiedReset)
            guard !Task.isCancelled, let self else { return }
            self.copyResets[nid] = nil
            if let i = self.byId[nid] { self.fire(i, "reset") }
        }
    }

    /// A full press of node `index` (a tap, Enter / Space).
    public func press(_ index: Int) {
        guard !isDisabled(index), let n = node(index) else { return }
        if let target = n.triggerFor {
            if justDismissed == target {
                justDismissed = nil
                return
            }
            layerReturn[target] = n.id
        }
        switch (n.component, n.part) {
        case ("Slider", "track"), ("Segmented", _):
            return
        default:
            if n.isTextField { return }
        }
        fire(index, "press")
    }

    /// Press the node with this id.
    public func press(id: String) {
        if let i = byId[id] { press(i) }
    }

    // MARK: - states

    /// Change a node's interaction states and hand them to the core (which
    /// restyles THAT node, and opens / holds hover overlays).
    @discardableResult
    func setInteraction(_ id: String, _ change: (inout InteractionState) -> Void) -> Bool {
        var s = interaction[id] ?? InteractionState()
        let before = s
        change(&s)
        if s == before { return false }
        interaction[id] = s.states.isEmpty ? nil : s
        if surface.setStates(id: id, states: s.states) {
            layoutNeeded = true
            pass()
        }
        return true
    }

    /// Pointer down on a pressable: the `pressed` state; pointer focus is
    /// not `focus-visible`.
    public func pressDown(_ id: String) {
        keyboardFocus = false
        if let prev = pressedId, prev != id { setInteraction(prev) { $0.pressed = false } }
        if let i = byId[id], isDisabled(i) { return }
        pressedId = id
        setInteraction(id) {
            $0.pressed = true
            $0.focusVisible = false
        }
    }

    /// Pointer up: clear the state; the press itself is the button's action.
    public func pressUp(_ id: String) {
        if pressedId == id { pressedId = nil }
        setInteraction(id) { $0.pressed = false }
        // A dismissal remembered for the trigger under this press is spent
        // once the press resolved.
    }

    /// A press that left its node (a drag out): no `press`.
    public func pressCancel(_ id: String) {
        if pressedId == id { pressedId = nil }
        setInteraction(id) { $0.pressed = false }
    }

    /// The pointer entered / left a node (macOS, an iPad pointer). A trigger
    /// that opens on hover (Tooltip anchor, `openOn: hover` Popover) takes
    /// its `hover` after the platform delay; the core opens, holds and
    /// closes the overlay from the states (`hoverTimer` closes it late).
    public func setHover(id: String, _ hovered: Bool) {
        if hovered { pointerSeen() }
        if let i = byId[id], let target = nodes[i].triggerFor, opensOnHover(target) {
            hoverTrigger(id, target: target, hovered)
            return
        }
        setInteraction(id) { $0.hover = hovered }
    }

    /// A drag with files over a FileUpload drop zone (`dragover`).
    public func setDragover(id: String, _ over: Bool) {
        setInteraction(id) { $0.dragover = over }
    }

    /// Does overlay `target` open on hover (Tooltip, a hover Popover)?
    func opensOnHover(_ target: String) -> Bool {
        guard let t = node(id: target) else { return false }
        return t.component == "Tooltip" || (t.component == "Popover" && t.props.str("openOn") == "hover")
    }

    private func hoverTrigger(_ id: String, target: String, _ hovered: Bool) {
        hoverDelay?.cancel()
        let open = layers.contains { $0.owner == target }
        let delay: Duration = hovered ? (open ? .zero : tooltipDelay) : (settings.hoverCloseMs > 0 ? .zero : hoverGrace)
        if delay == .zero {
            setInteraction(id) { $0.hover = hovered }
            return
        }
        hoverDelay = Task { @MainActor [weak self] in
            try? await Task.sleep(for: delay)
            guard !Task.isCancelled, let self else { return }
            self.setInteraction(id) { $0.hover = hovered }
        }
    }

    /// The pointer over a hover-opened layer (a Tooltip, a hover Popover):
    /// the layer holds itself open (`<owner>.layer` hover) and keeps its
    /// trigger hovered; leaving it closes through the trigger's delay
    /// (gpui `hover_card`).
    public func layerHover(owner: String, _ hovered: Bool) {
        if hovered { pointerSeen() }
        setInteraction("\(owner).layer") { $0.hover = hovered }
        guard let trigger = trigger(of: owner) else { return }
        if hovered {
            hoverDelay?.cancel()
            setInteraction(trigger.id) { $0.hover = true }
        } else {
            hoverTrigger(trigger.id, target: owner, false)
        }
    }

    /// The core asked to re-check a hover overlay after `delayMs` (ONE
    /// timer per owner: a new one restarts it).
    func scheduleHoverTimeout(owner: String, delayMs: Double) {
        guard !owner.isEmpty else { return }
        hoverCloseTimers[owner]?.cancel()
        hoverCloseTimers[owner] = Task { @MainActor [weak self] in
            try? await Task.sleep(for: .milliseconds(Int(max(0, delayMs))))
            guard !Task.isCancelled, let self else { return }
            self.hoverCloseTimers[owner] = nil
            let events = self.surface.hoverTimeout(owner: owner)
            if !events.isEmpty { self.dispatch(events) }
        }
    }

    // MARK: - overlays

    /// Open or close an overlay owner (Dialog, Drawer, Popover, Tooltip,
    /// Menu, Select, the pickers, Toast).
    public func setOpen(_ owner: String, _ open: Bool) {
        dispatch(surface.setOpen(id: owner, open: open))
    }

    /// Close the overlay `owner` (a swipe, the scrim, an outside press): the
    /// core refuses a non-dismissible one.
    public func dismissLayer(_ owner: String) {
        guard let layer = layers.last(where: { $0.owner == owner }) else { return }
        guard layer.dismissible else { return }
        justDismissed = owner
        fire(layer.root, "dismiss")
        // The remembered dismissal only guards the trigger's own tap.
        Task { @MainActor [weak self] in
            try? await Task.sleep(for: .milliseconds(300))
            if self?.justDismissed == owner { self?.justDismissed = nil }
        }
    }

    /// Escape (gpui `escape`): the TOP interactive overlay (dismissible → `dismiss`; an AlertDialog presses
    /// its cancel), else a tooltip, else a focused toast. Returns whether
    /// something handled it.
    @discardableResult
    public func escape() -> Bool {
        if let top = layers.last(where: \.isInteractive) {
            if top.dismissible {
                justDismissed = nil
                fire(top.root, "dismiss")
            } else if let cancel = top.order.first(where: { i in node(i).map { $0.id.hasSuffix(".cancel") && $0.pressable } ?? false }) {
                press(cancel)
            }
            return true
        }
        if let tip = layers.last(where: { $0.kind == "Tooltip" }) {
            dispatch(surface.setOpen(id: tip.owner, open: false))
            return true
        }
        if let f = focusedId, let toast = toastOwner(of: f) {
            dispatch(surface.dismissToast(id: toast))
            return true
        }
        return false
    }

    /// The modal layers (Dialog / Drawer) open, in order.
    public var modalLayers: [LayerInfo] { layers.filter(\.isModal) }

    /// The node that opens overlay `target` (its trigger).
    public func trigger(of target: String) -> NodeInfo? {
        nodes.first { !$0.removed && $0.triggerFor == target }
    }

    /// Open-layer bookkeeping after a pass (gpui `layers_changed`): focus
    /// into a newly opened interactive layer (its `autoFocus` node, else
    /// the selected option / day, else its first focusable; never a toast
    /// or a tooltip), focus back to the trigger when one closes.
    func layersChanged(from before: [LayerInfo]) {
        let interactive = layers.filter(\.isInteractive)
        let now = interactive.map { "\($0.owner)#\($0.layer)" }
        if now == openLayerKeys {
            layersSettled()
            return
        }
        let opened = interactive.filter { !openLayerKeys.contains("\($0.owner)#\($0.layer)") }
        let closed = openLayerKeys.filter { !now.contains($0) }.map { String($0.split(separator: "#").first ?? "") }
        var target: String?
        if let top = opened.last {
            if layerReturn[top.owner] == nil, let t = trigger(of: top.owner) { layerReturn[top.owner] = t.id }
            let candidates = focusOrder(in: top)
            let auto = top.order.first { i in node(i)?.a11y.autoFocus ?? false }
            let autoTarget = auto.flatMap { a in candidates.first { $0 == a || isDescendant($0, of: a) } ?? a }
            let selected = candidates.first { i in node(i).map { $0.selected && !$0.isTextField } ?? false }
            let first = candidates.first { i in !(node(i)?.isTextField ?? true) }
            target = (autoTarget ?? selected ?? first).flatMap { node($0)?.id }
        } else if let o = closed.last {
            let inside = focusedId.flatMap { node(id: $0) }.map { $0.layer > 0 } ?? true
            if inside { target = layerReturn[o] }
        }
        openLayerKeys = now
        let open = Set(layers.map(\.owner))
        for owner in Array(layerReturn.keys) where !open.contains(owner) {
            layerReturn[owner] = nil
        }
        if let target { requestFocus(target, keyboard: keyboardFocus) }
    }

    /// The layers did not change: a calendar day paged into view by the
    /// keyboard takes focus now.
    func layersSettled() {
        if let (owner, iso) = pendingDay {
            pendingDay = nil
            if let i = dayNode(owner: owner, iso: iso), let n = node(i) { requestFocus(n.id, keyboard: true) }
        }
    }

    func isDescendant(_ index: Int, of ancestor: Int) -> Bool {
        var cur = node(index)?.parent
        var guardCount = 0
        while let c = cur, guardCount < 4096 {
            if c == ancestor { return true }
            cur = nodes[safe: c]?.parent
            guardCount += 1
        }
        return false
    }

    // MARK: - controls (values are the core's)

    /// A Segmented item press: single = that value, multiple = toggled set.
    public func segmentedSelect(_ index: Int, value: JSONValue) {
        guard let n = node(index), !isDisabled(index) else { return }
        let multiple = n.props.str("type") == "multiple"
        let current = n.props["value"] ?? .null
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
        let items = n.props.list("items")
        if !multiple, let k = items.firstIndex(where: { $0["value"] == next }) { groupFocus[n.id] = k }
        fire(index, "change", payload: .object(["value": next]))
    }

    /// The values a Segmented shows selected.
    public func segmentedValues(_ index: Int) -> [String] {
        guard let n = node(index) else { return [] }
        let v = n.props["value"] ?? .null
        switch v {
        case let .array(a): return a.map(\.displayText)
        case let .string(s): return s.isEmpty ? [] : s.split(separator: ",").map(String.init)
        case .null: return []
        default: return [v.displayText]
        }
    }

    /// A Segmented's roving item: the one the arrows moved to, else the
    /// first selected, else the first.
    public func segmentedFocusIndex(_ index: Int) -> Int {
        guard let n = node(index) else { return 0 }
        if let i = groupFocus[n.id] { return i }
        let chosen = segmentedValues(index)
        return n.props.list("items").firstIndex { chosen.contains(($0["value"] ?? .null).displayText) } ?? 0
    }

    /// The checked state of a Checkbox / Switch (the owner's resolved prop).
    public func checked(_ index: Int) -> Bool {
        guard let o = owner(of: index) else { return false }
        return o.props["checked"]?.bool ?? o.checked
    }

    /// Is this Radio dot / row the chosen one?
    public func radioChecked(_ index: Int) -> Bool {
        guard let n = node(index), let o = owner(of: index) else { return false }
        if n.checked { return true }
        let value = o.props["value"] ?? .null
        let suffix = n.id.split(separator: ".").last.map(String.init) ?? ""
        guard let row = byId["\(o.id).item.\(suffix)"].flatMap({ node($0) }), let rv = row.props["value"] else { return false }
        return !value.isNull && rv.displayText == value.displayText
    }

    /// A Toggle's pressed state.
    public func togglePressed(_ index: Int) -> Bool {
        guard let n = owner(of: index) else { return false }
        return n.props["pressed"]?.bool ?? false
    }

    /// A Select's value.
    public func selectValue(_ index: Int) -> JSONValue {
        owner(of: index)?.props["value"] ?? .null
    }

    /// A Select pick: single = the value, multiple = the toggled set.
    public func selectPick(_ fieldIndex: Int, value: JSONValue) {
        guard let o = owner(of: fieldIndex) else { return }
        let current = o.props["value"] ?? .null
        let next: JSONValue
        if o.props.flag("multiple") {
            var set = current.array ?? (current.isNull ? [] : [current])
            if let p = set.firstIndex(of: value) { set.remove(at: p) } else { set.append(value) }
            next = .array(set)
        } else {
            next = value
        }
        fire(fieldIndex, "change", payload: .object(["value": next]))
    }

    /// The value a slider track shows (an in-flight drag, else the prop, else min).
    public func sliderValue(_ index: Int) -> Double {
        guard let n = node(index) else { return 0 }
        if let d = drags[n.id] { return d }
        return n.props.num("value") ?? owner(of: index)?.props.num("value") ?? n.props.num("min") ?? 0
    }

    public func sliderDrag(_ index: Int, value: Double) {
        guard let n = node(index), !isDisabled(index) else { return }
        if drags[n.id] != value { drags[n.id] = value }
    }

    public func sliderRelease(_ index: Int) {
        guard let n = node(index), let value = drags.removeValue(forKey: n.id) else { return }
        fire(index, "change", payload: .object(["value": .number(value)]))
    }

    /// The platform slider's range and step: the author's real range
    /// (only an empty or inverted one widens to `min...min+1`), and the
    /// platform step ONLY when it divides the range evenly (≤ 1000 stops,
    /// Compose's `sliderRange`); otherwise continuous, the value snapped by
    /// `snap` (which still reaches `max`).
    public static func sliderRange(min: Double, max: Double, step: Double) -> (range: ClosedRange<Double>, step: Double?) {
        let hi = max > min ? max : min + 1
        guard step > 0 else { return (min...hi, nil) }
        let n = (hi - min) / step
        let stops = n.rounded()
        return (min...hi, stops >= 1 && stops <= 1000 && abs(n - stops) < 1e-6 ? step : nil)
    }

    /// `v` clamped and snapped to `min + k·step`; `max` itself when the step
    /// does not divide the range and `v` is nearer to it than to the last
    /// stop (so the end stays reachable).
    public static func snap(_ v: Double, min: Double, max: Double, step: Double) -> Double {
        let lo = Swift.min(min, max), hi = Swift.max(min, max)
        let c = Swift.min(Swift.max(v, lo), hi)
        var x = step > 0 ? min + ((c - min) / step).rounded() * step : c
        x = Swift.min(Swift.max(x, lo), hi)
        if step > 0, abs(hi - x) > 1e-9, abs(c - hi) < abs(c - x) { x = hi }
        return (x * 1e9).rounded() / 1e9
    }

    /// A slider value at pointer `x` (the minimum sits at the right edge in RTL).
    public func sliderValue(at x: CGFloat, trackIndex: Int) -> Double {
        guard let n = node(trackIndex) else { return 0 }
        let f = frame(trackIndex)
        var frac = f.width > 0 ? Double(min(max((x - f.minX) / f.width, 0), 1)) : 0
        if isRTL { frac = 1 - frac }
        let lo = n.props.num("min") ?? 0, hi = n.props.num("max") ?? 100
        return Self.snap(lo + frac * (hi - lo), min: lo, max: hi, step: n.props.num("step") ?? 1)
    }

    /// A Carousel page change (`index` = the carousel or its indicator).
    public func carouselPage(_ index: Int, page: Int) {
        guard let o = owner(of: index) else { return }
        fire(o.index, "change", payload: .object(["page": .number(Double(page))]))
    }

    /// A right click / long press (or Shift+F10) inside a context Menu (`openOn: contextmenu`): open
    /// it at `point` (surface coordinates; nil = at the node).
    public func contextMenu(_ index: Int, at point: CGPoint? = nil) {
        let p = point ?? CGPoint(x: frame(index).midX, y: frame(index).midY)
        fire(index, "contextmenu", payload: .object(["x": .number(Double(p.x)), "y": .number(Double(p.y))]))
    }

    /// Submit a Form by id (what a `submit` Button or Enter in a single-line
    /// field does): the checks run; a failure focuses the first invalid
    /// field and announces `invalidFields`; a busy form refuses.
    public func submitForm(id formId: String) {
        dispatch(surface.submitForm(id: formId))
    }

    /// The failing `checks` messages of a field (empty = valid).
    public func failingChecks(_ fieldId: String) -> [String] {
        surface.failingChecks(id: fieldId)
    }

    /// A Chart's tooltip category / slice (pointer or keyboard).
    public func setChartHover(_ index: Int, _ point: Int?) {
        if chartHover[index] != point { chartHover[index] = point }
    }

    // MARK: - scrolling

    /// Scroll a windowed list to a content offset (relayouts only when the
    /// window moved).
    public func scroll(list id: String, offset: CGFloat) {
        if let i = lists[id]?.node { reportedOffsets[i] = CGPoint(x: 0, y: offset) }
        if surface.scroll(listId: id, offset: Float(offset)) {
            // The window's rows change without a structure-version bump.
            layoutNeeded = true
            pass()
        }
    }

    /// A native scroll view moved container `index` to `offset` (the core
    /// keeps every container's offset: scrollIntoView, windowing).
    public func scrollReported(_ index: Int, offset: CGPoint) {
        guard let n = node(index) else { return }
        let last = reportedOffsets[index] ?? .zero
        reportedOffsets[index] = offset
        if let l = lists[n.id], l.windowed {
            if abs(last.y - offset.y) >= 1 || last == .zero { scroll(list: l.id, offset: offset.y) }
            return
        }
        if surface.scrollTo(id: n.id, x: Float(offset.x), y: Float(offset.y)) {
            layoutNeeded = true
            // An anchored layer inside the container follows it now.
            if !layers.isEmpty { pass() }
        }
    }

    /// Move container `id` to an offset from the model (keyboard, host):
    /// the core clamps it; a native scroll view follows `scrollJumps`.
    public func scrollTo(id: String, offset: CGPoint) {
        if surface.scrollTo(id: id, x: Float(offset.x), y: Float(offset.y)) {
            layoutNeeded = true
            pass()
        }
    }

    /// Round 2: scroll List / Table `id` so the item at DATA index `index`
    /// shows (`start | center | end | nearest`).
    public func scrollToIndex(id: String, index: Int, align: String? = nil) {
        if layoutNeeded { pass() }
        dispatch(surface.scrollToIndex(id: id, index: UInt32(max(0, index)), align: align))
    }

    /// Round 2: the host viewport's scroll of the whole surface (pt):
    /// unbounded lists window against it, sticky nodes pin against it.
    public func setSurfaceScroll(_ offset: CGPoint) {
        guard offset != surfaceScroll else { return }
        surfaceScroll = offset
        if surface.setSurfaceScroll(x: Float(offset.x), y: Float(offset.y)) {
            nodesDirty = true
            layoutNeeded = true
            pass()
        }
    }

    /// A Resizable handle drag (`phase` = start | move | end, `delta` = pt
    /// along the group's axis since the drag STARTED): the core resizes
    /// from the sizes at the start (round 2 §1).
    public func resizeDrag(_ index: Int, phase: String, delta: CGFloat) {
        fire(index, "drag", payload: .object(["phase": .string(phase), "delta": .number(Double(delta))]))
    }

    /// A key on a node the core handles itself (a Resizable handle: the
    /// arrows, Home, End, Enter).
    public func nodeKey(_ index: Int, key: String) {
        fire(index, "key", payload: .object(["key": .string(key)]))
    }

    /// Round 2: pin the clock relative times read (epoch ms; nil = the wall clock).
    public func setClock(nowMs: Double?) {
        surface.setClock(nowMs: nowMs)
        invalidate(structure: false)
    }

    /// Re-bind the clock-dependent text (`formatRelativeTime` without `now`):
    /// `ExponentialSurface` calls it once a minute while `usesClock`.
    public func tick() {
        guard surface.usesClock() else { return }
        surface.tick()
        invalidate(structure: false)
    }

    // MARK: - host → surface commands (`catalog/a11y.json` `commands`)

    /// `focus {id}`, `announce {text, live}`, `scrollIntoView {id}`.
    public func command(_ command: SurfaceCommand) {
        if layoutNeeded { pass() }
        let json: JSONValue
        switch command {
        case let .focus(id): json = .object(["focus": .object(["id": .string(id)])])
        case let .announce(text, live): json = .object(["announce": .object(["text": .string(text), "live": .string(live)])])
        case let .scrollIntoView(id): json = .object(["scrollIntoView": .object(["id": .string(id)])])
        case let .scrollToIndex(id, index, align):
            var o: [String: JSONValue] = ["id": .string(id), "index": .number(Double(index))]
            if let align { o["align"] = .string(align) }
            json = .object(["scrollToIndex": .object(o)])
        }
        guard let events = try? surface.commandJson(commandJson: json.json) else { return }
        dispatch(events)
    }

    // MARK: - platform effects

    /// Speak through the platform's live region (`polite` waits,
    /// `assertive` interrupts) and tell the host.
    func announce(_ text: String, live: String) {
        guard !text.isEmpty else { return }
        focusSerial += 1
        announcement = Announcement(text: text, live: live, serial: focusSerial)
        host.announce(text: text, live: live)
        #if os(iOS)
        var attributed = AttributedString(text)
        attributed.accessibilitySpeechAnnouncementPriority = live == "assertive" ? .high : .default
        AccessibilityNotification.Announcement(attributed).post()
        #else
        AccessibilityNotification.Announcement(text).post()
        #endif
    }

    func copyToPasteboard(_ text: String) {
        if host.copy(text) { return }
        #if canImport(UIKit)
        UIPasteboard.general.string = text
        #elseif canImport(AppKit)
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        #endif
    }

    // MARK: - files

    /// A FileUpload asked for files: the host's picker when it has one, else
    /// `filePickRequest` (`ExponentialSurface` presents `.fileImporter`).
    func pickFiles(componentId: String, accept: String?, multiple: Bool) {
        let request = FilePickRequest(surfaceId: id, componentId: componentId, accept: accept, multiple: multiple)
        if host.pickFiles(request) { return }
        filePickRequest = request
    }

    /// The picker closed (picked or cancelled).
    public func filePickFinished() {
        filePickRequest = nil
    }

    /// Files picked for (or dropped on) FileUpload `componentId`: their
    /// bytes are read (security-scoped), the host gets them (`onUpload`)
    /// and the surface `upload {files: [{name, size, type}]}`.
    public func filesPicked(componentId: String, urls: [URL]) {
        let files: [SurfaceUploadFile] = urls.map { url in
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            let data = try? Data(contentsOf: url)
            return SurfaceUploadFile(name: url.lastPathComponent, size: data?.count ?? 0, type: mimeType(of: url), url: url, data: data)
        }
        filesPicked(componentId: componentId, files: files)
    }

    /// Files (already read) for FileUpload `componentId`.
    public func filesPicked(componentId: String, files: [SurfaceUploadFile]) {
        guard !files.isEmpty else { return }
        guard let zone = byId["\(componentId).dropzone"] ?? byId[componentId] else { return }
        let name = node(id: componentId)?.props.str("name") ?? ""
        host.onUpload(SurfaceUploadEvent(surfaceId: id, componentId: componentId, name: name, files: files))
        let meta: [JSONValue] = files.map { .object(["name": .string($0.name), "size": .number(Double($0.size)), "type": .string($0.type)]) }
        fire(zone, "upload", payload: .object(["files": .array(meta)]))
    }

    /// Files dropped on a drop zone node (its owner FileUpload takes them).
    public func filesDropped(on dropzoneId: String, urls: [URL]) {
        let owner = node(id: dropzoneId)?.owner ?? dropzoneId
        setDragover(id: dropzoneId, false)
        filesPicked(componentId: owner, urls: urls)
    }

    /// The UTTypes a FileUpload's `accept` allows (`image/*`, `.pdf`…; empty = any).
    public static func contentTypes(accept: String?) -> [UTType] {
        guard let accept, !accept.trimmingCharacters(in: .whitespaces).isEmpty else { return [.item] }
        var out: [UTType] = []
        for raw in accept.split(separator: ",") {
            let a = raw.trimmingCharacters(in: .whitespaces).lowercased()
            if a.hasPrefix(".") {
                if let t = UTType(filenameExtension: String(a.dropFirst())) { out.append(t) }
            } else if a.hasSuffix("/*") {
                switch a.dropLast(2) {
                case "image": out.append(.image)
                case "video": out.append(.movie)
                case "audio": out.append(.audio)
                case "text": out.append(.text)
                default: out.append(.item)
                }
            } else if let t = UTType(mimeType: a) {
                out.append(t)
            }
        }
        return out.isEmpty ? [.item] : out
    }
}

/// What a browser would report as a file's MIME type.
func mimeType(of url: URL) -> String {
    let ext = url.pathExtension.lowercased()
    if let t = UTType(filenameExtension: ext), let m = t.preferredMIMEType { return m }
    switch ext {
    case "md", "txt": return "text/plain"
    default: return "application/octet-stream"
    }
}

/// What a host may ask of a live surface (`catalog/a11y.json` `commands`).
public enum SurfaceCommand: Sendable, Equatable {
    /// Move keyboard focus to the node (its first focusable descendant).
    case focus(id: String)
    /// Speak through the live region (`polite` | `assertive`).
    case announce(text: String, live: String = "polite")
    /// Scroll every scrolling ancestor so the node is visible.
    case scrollIntoView(id: String)
    /// Round 2: bring item `index` (DATA order) of List / Table `id` into
    /// view; `align` = `start | center | end | nearest` (nil = nearest).
    case scrollToIndex(id: String, index: Int, align: String?)
}
