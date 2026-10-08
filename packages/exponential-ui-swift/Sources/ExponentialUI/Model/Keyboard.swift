import SwiftUI
import ExponentialUICore

/// Keyboard focus and the `catalog/a11y.json` keys (gpui `view/events.rs`
/// `handle_key` and friends): Tab order = PAINT order with roving tab stops
/// (Tabs, Radio) and the top overlay's focus trap; arrows in roving widgets
/// (wrap, Home/End), menus with submenus, listbox type-ahead, the calendar
/// grid (±day/week, PageUp/Down month, Shift = year, Home/End by the
/// locale's week), sliders, carousel dots, charts, scroll containers;
/// Escape; Shift+F10; Enter / Space. Horizontal arrows mirror under RTL.
///
/// The model's focus is the source of truth: `focusedId`, the core's
/// `focus` / `focus-visible` states (the ring = keyboard focus only), and
/// `focusRequest` for the views to move platform focus (`@FocusState`,
/// `@AccessibilityFocusState`, a text field's first responder).
extension SurfaceModel {
    // MARK: - focus

    /// The Tab order in force: the top interactive layer's (the focus trap),
    /// else the main tree's.
    public func focusOrder() -> [Int] {
        if let top = layers.last(where: \.isInteractive) { return focusOrder(in: top) }
        return Self.focusOrder(nodes: nodes, paintOrder: order, layer: 0)
    }

    func focusOrder(in layer: LayerInfo) -> [Int] {
        Self.focusOrder(nodes: nodes, paintOrder: layer.order, layer: layer.layer)
    }

    /// The Tab order of one layer (0 = the main tree) from the PAINT order:
    /// focusable nodes, ONE roving stop per Tabs (the selected tab) and per
    /// Radio (the checked dot, else the first).
    static func focusOrder(nodes: [NodeInfo], paintOrder: [Int], layer: Int) -> [Int] {
        var out: [Int] = []
        var roving: [String: Int] = [:]
        for i in paintOrder {
            guard let n = nodes[safe: i], n.layer == layer, n.isFocusable else { continue }
            var group: String?
            switch (n.ownerComponent, n.part) {
            case ("Tabs", "tab"), ("Radio", "dot"): group = n.owner
            default: group = nil
            }
            guard let g = group else {
                out.append(i)
                continue
            }
            let active = n.has("selected") || n.has("checked")
            if let pos = roving[g] {
                if active { out[pos] = i }
            } else {
                roving[g] = out.count
                out.append(i)
            }
        }
        return out
    }

    /// The next (previous) entry of `order` after `current` (wrapping);
    /// nothing focused = the first (last).
    static func nextFocus(_ order: [Int], current: Int?, backwards: Bool) -> Int? {
        guard !order.isEmpty else { return nil }
        let n = order.count
        if let c = current, let p = order.firstIndex(of: c) {
            return order[backwards ? (p + n - 1) % n : (p + 1) % n]
        }
        return backwards ? order[n - 1] : order[0]
    }

    /// The focused slot.
    public var focusedIndex: Int? { focusedId.flatMap { byId[$0] } }

    /// Move focus to `id` and ask the views to follow (`focusRequest`).
    public func requestFocus(_ id: String, keyboard: Bool) {
        guard let i = byId[id] else { return }
        setFocus(id, keyboard: keyboard)
        focusSerial += 1
        focusRequest = FocusRequest(id: id, index: i, keyboard: keyboard, serial: focusSerial)
    }

    /// The platform moved focus (a `@FocusState` change, a text field began
    /// / ended editing, a tap on a focusable view).
    public func focusMoved(id: String, focused: Bool) {
        if focused {
            setFocus(id, keyboard: keyboardFocus)
        } else if focusedId == id {
            setFocus(nil, keyboard: keyboardFocus)
        }
    }

    /// The model's focus moved: `focus` (+ `focus-visible` from the
    /// keyboard) follows; a toast with focus pauses; keyboard focus inside a
    /// hover-opened trigger is that trigger's `focus-visible` (it opens).
    func setFocus(_ id: String?, keyboard: Bool) {
        let prev = focusedId
        if prev != id, let p = prev {
            setInteraction(p) {
                $0.focus = false
                $0.focusVisible = false
            }
            if let pi = byId[p], nodes[pi].component == "Chart" { chartHover[pi] = nil }
            if let toast = toastOwner(of: p) { toastHold(toast, "focus", false) }
        }
        if focusedId != id { focusedId = id }
        if keyboardFocus != keyboard { keyboardFocus = keyboard }
        if let id {
            setInteraction(id) {
                $0.focus = true
                $0.focusVisible = keyboard
            }
            if let toast = toastOwner(of: id) { toastHold(toast, "focus", true) }
        }
        let trigger = keyboard ? id.flatMap { byId[$0] }.flatMap { hoverTriggerAbove($0) } : nil
        if trigger != focusTrigger {
            if let old = focusTrigger, byId[old] != nil, old != id {
                setInteraction(old) { $0.focusVisible = false }
            }
            if let t = trigger, t != id {
                setInteraction(t) { $0.focusVisible = true }
            }
            focusTrigger = trigger
        }
    }

    /// The nearest node at or above `index` that opens an overlay on hover.
    private func hoverTriggerAbove(_ index: Int) -> String? {
        var cur: Int? = index
        var guardCount = 0
        while let i = cur, let n = node(i), guardCount < 4096 {
            if let t = n.triggerFor, opensOnHover(t) { return n.id }
            cur = n.parent
            guardCount += 1
        }
        return nil
    }

    /// Move focus to the next (previous) focusable node, from the keyboard.
    @discardableResult
    public func focusNext(backwards: Bool = false) -> Int? {
        guard let next = Self.nextFocus(focusOrder(), current: focusedIndex, backwards: backwards), let n = node(next) else { return nil }
        requestFocus(n.id, keyboard: true)
        return next
    }

    /// Clear focus (a press on nothing, the surface lost focus).
    public func blur() {
        setFocus(nil, keyboard: false)
    }

    private func focusKey(_ index: Int) {
        if let n = node(index) { requestFocus(n.id, keyboard: true) }
    }

    // MARK: - keys

    /// What a key means along a horizontal axis: `+1` forward, `-1` back
    /// (Left / Right swap in RTL); Up / Down when `vertical` too.
    public static func arrowStep(_ key: String, rtl: Bool, vertical: Bool) -> Int? {
        let (fwd, back) = rtl ? ("left", "right") : ("right", "left")
        switch key {
        case fwd: return 1
        case back: return -1
        case "down" where vertical: return 1
        case "up" where vertical: return -1
        default: return nil
        }
    }

    /// One key (gpui names: `tab escape enter space up down left right home
    /// end pageup pagedown backspace delete f10`; `typed` = the character a
    /// letter key types) on the focused node. `true` = handled (stop it).
    @discardableResult
    public func handleKey(key: String, shift: Bool = false, typed: String? = nil) -> Bool {
        let focused = focusedIndex.flatMap { node($0) }
        switch key {
        case "tab":
            focusNext(backwards: shift)
            return true
        case "escape":
            if let n = focused, n.component == "Composer", n.props.flag("busy") {
                fire(n.index, "stop")
                return true
            }
            if let n = focused, n.component == "Chart", chartHover[n.index] != nil {
                setChartHover(n.index, nil)
                return true
            }
            return escape()
        case "f10" where shift:
            guard let n = focused else { return false }
            contextMenu(n.index)
            return true
        default:
            break
        }
        guard let n = focused else { return false }
        if n.isTextField { return fieldKey(n.index, key: key, shift: shift) }
        let owner = n.ownerComponent ?? ""
        let rtl = isRTL
        let stepH = Self.arrowStep(key, rtl: rtl, vertical: false)
        let stepHV = Self.arrowStep(key, rtl: rtl, vertical: true)
        let vertical: Int? = key == "down" ? 1 : (key == "up" ? -1 : nil)
        switch (n.component, owner, n.part) {
        case ("ToggleGroup", _, _):
            return toggleGroupKey(n, key: key, step: stepHV)
        case ("Chart", _, nil) where !n.isSparkline:
            let count = Self.chartPointCount(n.props)
            guard count > 0 else { return false }
            let next: Int
            switch key {
            case "home": next = 0
            case "end": next = count - 1
            default:
                guard let d = stepH else { return false }
                if let a = chartHover[n.index] { next = ((a + d) % count + count) % count } else { next = d > 0 ? 0 : count - 1 }
            }
            setChartHover(n.index, next)
            return true
        case ("Slider", _, "track"):
            let lo = n.props.num("min") ?? 0, hi = n.props.num("max") ?? 100, step = n.props.num("step") ?? 1
            let current = sliderValue(n.index)
            let value: Double
            switch key {
            case "home": value = lo
            case "end": value = hi
            case "up": value = Self.snap(current + step, min: lo, max: hi, step: step)
            case "down": value = Self.snap(current - step, min: lo, max: hi, step: step)
            case "pageup": value = Self.snap(current + 10 * step, min: lo, max: hi, step: step)
            case "pagedown": value = Self.snap(current - 10 * step, min: lo, max: hi, step: step)
            default:
                guard let d = stepH else { return false }
                value = Self.snap(current + Double(d) * step, min: lo, max: hi, step: step)
            }
            fire(n.index, "change", payload: .object(["value": .number(value)]))
            return true
        case ("Box", "Carousel", "indicator"):
            let count = Int(n.props.num("count") ?? 0)
            let page = Int(n.props.num("page") ?? 0)
            guard count > 0 else { return false }
            let next: Int
            switch key {
            case "home": next = 0
            case "end": next = count - 1
            default:
                guard let d = stepH else { return false }
                next = ((page + d) % count + count) % count
            }
            carouselPage(n.index, page: next)
            return true
        case ("Text", "Tabs", "tab"):
            if let t = Self.rovingTarget(rovingSet(n), current: n.index, key: key, step: stepH) {
                focusKey(t)
                press(t)
                return true
            }
        case ("Radio", "Radio", "dot"):
            if let t = Self.rovingTarget(rovingSet(n), current: n.index, key: key, step: stepHV) {
                focusKey(t)
                press(t)
                return true
            }
        case ("Text", "Accordion", "trigger"):
            if let t = Self.rovingTarget(rovingSet(n), current: n.index, key: key, step: vertical) {
                focusKey(t)
                return true
            }
        case ("Box", "DropdownMenu", "item"), ("Box", "ContextMenu", "item"):
            let kind = n.props.str("kind").isEmpty ? "item" : n.props.str("kind")
            let (openKey, closeKey) = rtl ? ("left", "right") : ("right", "left")
            if key == openKey && kind == "submenu" {
                press(n.index)
                return true
            }
            if key == closeKey && n.layer > 1, let l = layers.first(where: { $0.layer == n.layer }) {
                // Back to the parent menu: close this submenu.
                fire(l.root, "dismiss")
                return true
            }
            let set = rovingSet(n)
            if let t = Self.rovingTarget(set, current: n.index, key: key, step: vertical) {
                focusKey(t)
                return true
            }
            if let t = typed, t.count == 1, !t.trimmingCharacters(in: .whitespaces).isEmpty {
                return typeahead(set, current: n.index, typed: t)
            }
        case ("Text", "Select", "item"), ("Text", "TimePicker", "item"):
            let set = rovingSet(n)
            if let t = Self.rovingTarget(set, current: n.index, key: key, step: vertical) {
                focusKey(t)
                return true
            }
            if let t = typed, t.count == 1, !t.trimmingCharacters(in: .whitespaces).isEmpty {
                return typeahead(set, current: n.index, typed: t)
            }
        case ("Text", "DatePicker", "day"), ("Text", "DateRangePicker", "day"):
            return calendarKey(n, key: key, shift: shift, step: stepH)
        default:
            break
        }
        // Openers: ArrowDown (Up) on a picker / menu trigger opens it.
        if (key == "down" || key == "up") && (n.triggerFor != nil || n.part == "trigger") && owner != "Accordion" && owner != "Tooltip" {
            if !n.open { press(n.index) }
            return true
        }
        // A focused scroll container scrolls.
        if let s = scrolls[n.index] {
            let f = frame(n.index)
            let maxY = max(0, s.contentSize.height - f.height)
            var y: CGFloat?
            switch key {
            case "down": y = s.offset.y + 40
            case "up": y = s.offset.y - 40
            case "pagedown", "space": y = s.offset.y + f.height * 0.9
            case "pageup": y = s.offset.y - f.height * 0.9
            case "home": y = 0
            case "end": y = maxY
            default: y = nil
            }
            if let y {
                scrollTo(id: n.id, offset: CGPoint(x: s.offset.x, y: min(max(0, y), maxY)))
                return true
            }
        }
        if key == "enter" || key == "space" {
            if key == "enter" && owner == "Table" && n.part == "row" {
                fire(n.index, "press")
                return true
            }
            press(n.index)
            return true
        }
        return false
    }

    /// Keys inside a host text field that the field does not own:
    /// NumberField steps (Up/Down, Shift ×10, PageUp/Down ×10), ChipInput's
    /// Backspace in an empty field (removes the last chip). Text field
    /// views call this from their key handling; `true` = handled.
    @discardableResult
    public func fieldKey(_ index: Int, key: String, shift: Bool = false) -> Bool {
        guard let n = node(index) else { return false }
        switch (n.ownerComponent ?? "", key) {
        case ("NumberField", "up"), ("NumberField", "down"), ("NumberField", "pageup"), ("NumberField", "pagedown"):
            guard let ownerId = n.owner else { return false }
            let times = key.hasPrefix("page") || shift ? 10 : 1
            let part = key == "up" || key == "pageup" ? "increment" : "decrement"
            guard let button = byId["\(ownerId).\(part)"] else { return false }
            flushPending(index)
            for _ in 0..<times { fire(button, "press") }
            return true
        case ("ChipInput", "backspace"):
            guard fieldText(index).isEmpty, let ownerId = n.owner else { return false }
            if let remove = liveParts(ownerId, "remove").last {
                fire(remove.index, "press")
                return true
            }
            return false
        default:
            return false
        }
    }

    /// The siblings a roving widget moves between: same owner + part +
    /// component, in paint order, enabled.
    func rovingSet(_ n: NodeInfo) -> [Int] {
        let paint = n.layer == 0 ? order : (layers.first { $0.order.contains(n.index) }?.order ?? [])
        return paint.filter { i in
            guard let m = node(i) else { return false }
            return m.owner == n.owner && m.part == n.part && m.component == n.component && !m.hidden && m.isFocusable && !isDisabled(i)
        }
    }

    /// Move within a roving set by `step` (wrapping); Home / End = the ends.
    static func rovingTarget(_ set: [Int], current: Int, key: String, step: Int?) -> Int? {
        guard !set.isEmpty else { return nil }
        if key == "home" { return set.first }
        if key == "end" { return set.last }
        guard let step else { return nil }
        let pos = set.firstIndex(of: current) ?? 0
        let len = set.count
        return set[((pos + step) % len + len) % len]
    }

    /// Type-ahead in a listbox / menu: the next entry starting with `typed`.
    private func typeahead(_ set: [Int], current: Int, typed: String) -> Bool {
        let lower = typed.lowercased()
        let pos = set.firstIndex(of: current) ?? 0
        func text(_ i: Int) -> String {
            guard let n = node(i) else { return "" }
            if let t = n.props["text"]?.string { return t }
            for c in n.children { if let t = node(c)?.props["text"]?.string { return t } }
            return ""
        }
        guard !set.isEmpty else { return false }
        for k in 1...set.count {
            let i = set[(pos + k) % set.count]
            if text(i).lowercased().hasPrefix(lower) {
                focusKey(i)
                return true
            }
        }
        return false
    }

    /// Calendar grid keys: ±1 day / ±1 week, PageUp/PageDown month (Shift:
    /// year), Home/End the row (the locale's week).
    private func calendarKey(_ n: NodeInfo, key: String, shift: Bool, step: Int?) -> Bool {
        guard let (y, m, d) = CivilDate.parse(n.props.str("date")) else { return false }
        let z = CivilDate.days(y, m, d)
        let owner = n.owner ?? ""
        var target: Int?
        switch key {
        case "enter", "space":
            // a11y.json DatePicker: "Enter, Space: choose".
            press(n.index)
            return true
        case "up": target = z - 7
        case "down": target = z + 7
        case "home", "end":
            let row = n.parent.flatMap { node($0) }?.children ?? []
            if let i = key == "home" ? row.first : row.last {
                focusKey(i)
                return true
            }
        case "pageup", "pagedown":
            let months = (shift ? 12 : 1) * (key == "pageup" ? -1 : 1)
            let (ny, nm) = CivilDate.addMonths(y, m, months)
            target = CivilDate.days(ny, nm, min(d, CivilDate.daysInMonth(ny, nm)))
        default:
            target = step.map { z + $0 }
        }
        guard let t = target else { return false }
        let (ty, tm, td) = CivilDate.civil(t)
        let iso = CivilDate.iso(ty, tm, td)
        if let i = dayNode(owner: owner, iso: iso) {
            focusKey(i)
            return true
        }
        // Another month: page the calendar, focus once it is built.
        let forward = t > z
        if let button = byId["\(owner).\(forward ? "next" : "previous")"] {
            let months = max(1, abs((ty - y) * 12 + tm - m))
            for _ in 0..<months { fire(button, "press") }
            // Each press laid out at once: the target month is built now
            // (else a later pass focuses it, `layersSettled`).
            if let i = dayNode(owner: owner, iso: iso) { focusKey(i) } else { pendingDay = (owner, iso) }
            return true
        }
        return false
    }

    /// The day cell of calendar `owner` showing `iso` (not an outside day).
    func dayNode(owner: String, iso: String) -> Int? {
        nodes.first { !$0.removed && $0.owner == owner && $0.part == "day" && $0.props.str("date") == iso && !$0.props.flag("outside") }?.index
    }

    /// ToggleGroup keys: arrows move the roving item, Space / Enter toggle it.
    private func toggleGroupKey(_ n: NodeInfo, key: String, step: Int?) -> Bool {
        let items = n.props.list("items")
        guard !items.isEmpty else { return false }
        let len = items.count
        let current = toggleGroupFocusIndex(n.index)
        let next: Int
        switch key {
        case "home": next = 0
        case "end": next = len - 1
        case "enter", "space":
            if let v = items[safe: current]?["value"] { toggleGroupSelect(n.index, value: v) }
            groupFocus[n.id] = current
            return true
        default:
            guard let s = step else { return false }
            next = ((current + s) % len + len) % len
        }
        groupFocus[n.id] = next
        return true
    }

    /// The points a Chart's keyboard tooltip steps through (pie / donut:
    /// the first series' slices; else the categories or the longest series).
    static func chartPointCount(_ props: Props) -> Int {
        let series = props.list("series")
        let longest = series.map { $0["values"]?.array?.count ?? 0 }.max() ?? 0
        switch props.str("kind") {
        case "pie", "donut": return series.first?["values"]?.array?.count ?? 0
        default: return max(props.list("categories").count, longest)
        }
    }
}

/// Proleptic Gregorian day arithmetic (Howard Hinnant's civil algorithms),
/// what the calendar keys page with.
enum CivilDate {
    static func parse(_ iso: String) -> (Int, Int, Int)? {
        let p = iso.prefix(10).split(separator: "-")
        guard p.count == 3, let y = Int(p[0]), let m = Int(p[1]), let d = Int(p[2]), (1...12).contains(m), (1...31).contains(d) else { return nil }
        return (y, m, d)
    }

    static func days(_ y0: Int, _ m: Int, _ d: Int) -> Int {
        let y = m <= 2 ? y0 - 1 : y0
        let era = (y >= 0 ? y : y - 399) / 400
        let yoe = y - era * 400
        let mp = (m + 9) % 12
        let doy = (153 * mp + 2) / 5 + d - 1
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy
        return era * 146_097 + doe - 719_468
    }

    static func civil(_ z0: Int) -> (Int, Int, Int) {
        let z = z0 + 719_468
        let era = (z >= 0 ? z : z - 146_096) / 146_097
        let doe = z - era * 146_097
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365
        let y = yoe + era * 400
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100)
        let mp = (5 * doy + 2) / 153
        let d = doy - (153 * mp + 2) / 5 + 1
        let m = mp < 10 ? mp + 3 : mp - 9
        return (m <= 2 ? y + 1 : y, m, d)
    }

    static func addMonths(_ y: Int, _ m: Int, _ delta: Int) -> (Int, Int) {
        let total = y * 12 + (m - 1) + delta
        let ny = Int((Double(total) / 12).rounded(.down))
        return (ny, total - ny * 12 + 1)
    }

    static func daysInMonth(_ y: Int, _ m: Int) -> Int {
        let (ny, nm) = addMonths(y, m, 1)
        return days(ny, nm, 1) - days(y, m, 1)
    }

    static func iso(_ y: Int, _ m: Int, _ d: Int) -> String {
        String(format: "%04d-%02d-%02d", y, m, d)
    }
}
