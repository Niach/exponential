import SwiftUI
import Observation
import ExponentialUICore
import ExponentialUIPrimitives

/// Options of a surface model.
public struct SurfaceOptions: Sendable {
    public var catalogId: String
    /// A built-in theme id (`exponential` default); nil = geometry mode.
    public var theme: ThemeHandle?
    /// The colour mode when `settings` is nil (a fixed light / dark).
    public var mode: Mode
    /// Round frames to whole points (off: fractional, like the web).
    public var rounding: Bool
    /// Locale, strings, `system` mode, density, contrast, font scale,
    /// insets, pointer, motion, today. nil = the defaults with `mode` above.
    public var settings: SurfaceSettings?

    public init(catalogId: String = coreCatalogId(), theme: ThemeHandle? = ThemeHandle.builtin(defaultThemeId()), mode: Mode = .dark, rounding: Bool = false, settings: SurfaceSettings? = nil) {
        self.catalogId = catalogId
        self.theme = theme
        self.mode = mode
        self.rounding = rounding
        self.settings = settings
    }

    /// The settings in force: `settings`, else the defaults pinned to `mode`.
    var resolvedSettings: SurfaceSettings {
        settings ?? SurfaceSettings(mode: mode == .dark ? .dark : .light)
    }
}

/// ONE surface: owns the core `Surface`, runs the layout passes with the
/// painter's measurer, keeps the node slots (patched from each pass's node
/// delta), the per-pass frames, visuals, scroll offsets, layers and toasts,
/// and holds every bit of interaction state (pressed / hover / focus, the
/// keyboard, text fields, timers). The views observe it;
/// `ExponentialSurface` is its SwiftUI face. Control VALUES live in the
/// core (bound ones in the data model, unbound ones in its local state):
/// the model fires `change` and reads the props back.
@MainActor
@Observable
public final class SurfaceModel {
    public let id: String
    @ObservationIgnored public let surface: Surface
    @ObservationIgnored public var host: HostPlugin
    @ObservationIgnored public let options: SurfaceOptions
    @ObservationIgnored let extensions: ExtensionRegistry

    /// The theme painters query (the core's effective theme under a
    /// density / contrast setting).
    public internal(set) var theme: ThemeHandle?
    /// The RESOLVED colour mode (`system` follows the platform).
    public internal(set) var mode: Mode
    /// The host settings (nil / `system` parts follow `platform`).
    public internal(set) var settings: SurfaceSettings
    /// What the platform reports (the SwiftUI environment).
    public internal(set) var platform = PlatformTraits()
    /// Every node SLOT (`nodes[i].index == i`; removed slots are hidden
    /// tombstones). Paint order is `order`, not the slot order.
    public private(set) var nodes: [NodeInfo] = []
    /// Children per slot in paint order (`nodes[i].children`).
    public private(set) var children: [[Int]] = []
    public private(set) var byId: [String: Int] = [:]
    public private(set) var structureVersion: UInt64 = 0
    /// The main tree's paint order (= accessibility order); `order.first` is the root.
    public private(set) var order: [Int] = []
    /// Frames in SURFACE coordinates by slot (layers included), UNSCROLLED.
    public private(set) var frames: [CGRect] = []
    public private(set) var styles: [PaintStyle] = []
    /// The text colour per node (own, else inherited, else `foreground`).
    public private(set) var inks: [Color] = []
    public private(set) var textStyles: [Int: TextStyle] = [:]
    public private(set) var layers: [LayerInfo] = []
    public private(set) var lists: [String: ListInfo] = [:]
    /// Scroll containers by slot: the core's offsets and content sizes.
    public private(set) var scrolls: [Int: ScrollInfo] = [:]
    /// Round 2: `position: sticky` nodes and pinned List section headers
    /// this pass, by slot: paint the node and its subtree moved by this.
    public private(set) var sticky: [Int: CGSize] = [:]
    /// The host's scroll of the whole surface (`setSurfaceScroll`).
    @ObservationIgnored public internal(set) var surfaceScroll: CGPoint = .zero
    /// The open toasts (the model runs their timers).
    public private(set) var toasts: [ToastInfo] = []
    /// `ltr` | `rtl` (the locale's, unless the author set `direction`).
    public private(set) var direction = "ltr"
    /// The active breakpoint (`sm`…`xl`; nil = base).
    public private(set) var breakpoint: String?
    public private(set) var surfaceSize: CGSize = .zero
    public private(set) var passCount = 0
    public private(set) var stats = PassStats()
    /// Tokens for the generic primitives under the surface's theme.
    public internal(set) var primitiveTokens = PrimitiveTokens.system

    // MARK: interaction (published: views observe these)

    /// The node holding keyboard focus (the model's view of it).
    public internal(set) var focusedId: String?
    /// The last focus move came from the keyboard (`focus-visible`: the ring).
    public internal(set) var keyboardFocus = false
    /// A request to move platform focus (`@FocusState` /
    /// `@AccessibilityFocusState` / a text field's first responder) to a
    /// node: a Form's first invalid field, a `focus` command, a dialog's
    /// initial focus or its return target, a keyboard move. `serial` grows
    /// with every request (the same id twice still fires `onChange`).
    public internal(set) var focusRequest: FocusRequest?
    /// The last `announce` (Form errors, CodeBlock `copied`, a command):
    /// also posted as an `AccessibilityNotification.Announcement`.
    public internal(set) var announcement: Announcement?
    /// A file picker the surface should present (`pickFiles`).
    public internal(set) var filePickRequest: FilePickRequest?
    /// A programmatic scroll a native scroll view should follow
    /// (scrollIntoView, keyboard scrolling), by container slot.
    public internal(set) var scrollJumps: [Int: ScrollJump] = [:]
    /// The Segmented item the arrows moved to (group id → item index).
    public internal(set) var groupFocus: [String: Int] = [:]
    /// The category / slice a Chart's tooltip shows (chart slot → index).
    public internal(set) var chartHover: [Int: Int] = [:]

    /// The host's width (the view's geometry), visible height and card bound.
    @ObservationIgnored public internal(set) var width: CGFloat = 0
    @ObservationIgnored public internal(set) var viewportHeight: CGFloat = 0
    @ObservationIgnored public internal(set) var maxHeight: CGFloat?
    @ObservationIgnored var nodesDirty = true
    @ObservationIgnored var visualsDirty = true
    @ObservationIgnored var layoutNeeded = true
    @ObservationIgnored var inPass = false
    @ObservationIgnored var passDepth = 0
    /// The offsets the native scroll views reported (by container slot).
    @ObservationIgnored var reportedOffsets: [Int: CGPoint] = [:]
    @ObservationIgnored var baseTheme: ThemeHandle?
    @ObservationIgnored var appliedSettings: FfiSettings?
    @ObservationIgnored var interaction: [String: InteractionState] = [:]
    @ObservationIgnored var pressedId: String?
    @ObservationIgnored var revisions: [String: Int] = [:]
    @ObservationIgnored var fields: [String: FieldState] = [:]
    @ObservationIgnored var layerReturn: [String: String] = [:]
    @ObservationIgnored var openLayerKeys: [String] = []
    @ObservationIgnored var justDismissed: String?
    @ObservationIgnored var hoverDelay: Task<Void, Never>?
    @ObservationIgnored var hoverCloseTimers: [String: Task<Void, Never>] = [:]
    @ObservationIgnored var toastTimers: [String: ToastTimer] = [:]
    @ObservationIgnored var toastHeld: [String: Set<String>] = [:]
    @ObservationIgnored var copyResets: [String: Task<Void, Never>] = [:]
    @ObservationIgnored var focusTrigger: String?
    @ObservationIgnored var pendingDay: (owner: String, iso: String)?
    @ObservationIgnored var focusSerial = 0
    @ObservationIgnored var measureGeneration: UInt64 = 0
    @ObservationIgnored var unknownReported: Set<String> = []
    @ObservationIgnored var rank: [Int] = []
    @ObservationIgnored var lastKinds: [String: String] = [:]
    /// The built-in string table (with the overrides it was built from) and
    /// the surface-locale formatters (`Model/Locale.swift`).
    @ObservationIgnored var stringTable: [String: String] = [:]
    @ObservationIgnored var stringsKey: String?
    @ObservationIgnored var numberFormatters: [String: NumberFormatter] = [:]
    @ObservationIgnored var dateFormatters: [String: DateFormatter] = [:]
    /// The surface Formatter the core formats through (`FoundationFormatter`).
    @ObservationIgnored public internal(set) var formatter: FoundationFormatter?
    /// Tests and geometry suites: lay out with the core's fixed fake
    /// measure (8 px per character, 20 px lines) instead of TextKit.
    @ObservationIgnored public var fixedMeasure = false
    /// Conformance: called with every node index a `NodeView` paints.
    @ObservationIgnored var paintProbe: ((Int) -> Void)?
    /// The text field that has focus.
    var focusedField: String?
    /// Slider drags in flight (track id → value).
    var drags: [String: Double] = [:]

    public init(id: String, options: SurfaceOptions = SurfaceOptions(), host: HostPlugin? = nil) throws {
        self.id = id
        self.options = options
        self.host = host ?? NoHost()
        self.theme = options.theme
        self.baseTheme = options.theme
        self.mode = options.mode
        self.settings = options.resolvedSettings
        self.extensions = ExtensionRegistry.shared
        self.surface = try Surface.withTheme(surfaceId: id, catalogId: options.catalogId, theme: options.theme?.theme, mode: options.mode.rawValue)
        surface.setRounding(on: options.rounding)
        // A registered extension the core refuses fails the surface (the
        // host learns why), never a surface silently missing its components.
        for json in extensions.definitions {
            try surface.registerExtension(extensionJson: json)
        }
        primitiveTokens = theme?.primitiveTokens(mode: mode) ?? .system
        applySettings()
    }

    // MARK: - Content

    /// One A2UI server→client message (`updateComponents`, `updateDataModel`…).
    @discardableResult
    public func apply(_ message: JSONValue) throws -> FfiApplyOutcome {
        let out = try surface.apply(messageJson: message.json)
        invalidate(structure: out.structureChanged)
        return out
    }

    @discardableResult
    public func apply(json: String) throws -> FfiApplyOutcome {
        let out = try surface.apply(messageJson: json)
        invalidate(structure: out.structureChanged)
        return out
    }

    /// The nested authoring form (fixtures, MCP templates).
    @discardableResult
    public func setNested(json: String) throws -> FfiApplyOutcome {
        let out = try surface.setNested(nestedJson: json)
        invalidate(structure: true)
        return out
    }

    /// A flat component list.
    @discardableResult
    public func setComponents(json: String) throws -> FfiApplyOutcome {
        let out = try surface.setComponents(componentsJson: json)
        invalidate(structure: true)
        return out
    }

    /// Write at a JSON pointer (nil removes).
    public func setData(path: String, value: JSONValue?) {
        try? surface.setData(path: path, valueJson: value?.json)
        invalidate(structure: false)
    }

    public var data: JSONValue { JSONValue.parse(surface.dataJson()) }
    public var issues: [JSONValue] { JSONValue.parse(surface.issuesJson()).array ?? [] }

    public func setTheme(_ theme: ThemeHandle) {
        baseTheme = theme
        surface.setTheme(theme: theme.theme)
        measureGeneration += 1
        visualsDirty = true
        refreshPaintTheme()
        invalidate(structure: true)
    }

    /// A fixed light / dark mode (`setModeSetting(.system)` follows the platform).
    public func setMode(_ mode: Mode) {
        var s = settings
        s.mode = mode == .dark ? .dark : .light
        setSettings(s)
    }

    /// Register an extension on THIS surface (the global registry covers
    /// new models; see `ExponentialUI.register`).
    public func register(extension json: String, painters: [String: ExtensionPainter]) throws {
        try surface.registerExtension(extensionJson: json)
        for (kind, painter) in painters { extensions.register(kind: kind, painter: painter) }
        measureGeneration += 1
        invalidate(structure: true)
    }

    /// Something changed in the core: relayout (a pass follows at once when
    /// the width is known). `structure` = the host replaced content.
    func invalidate(structure: Bool) {
        if structure { visualsDirty = true }
        layoutNeeded = true
        if width > 0 { pass() }
    }

    // MARK: - Viewport

    /// The host's width (from the view's geometry) and visible height
    /// (dialog centring, windowed lists). `maxHeight` bounds a card.
    public func setViewport(width: CGFloat, height: CGFloat, maxHeight: CGFloat? = nil) {
        let w = max(0, width.rounded(.down))
        if w == self.width, height == viewportHeight, maxHeight == self.maxHeight, !layoutNeeded { return }
        self.width = w
        viewportHeight = height
        self.maxHeight = maxHeight
        layoutNeeded = true
        if w > 0 { pass() }
    }

    // MARK: - The pass

    /// Run one layout pass (the core, the measurer, the caches), then the
    /// bookkeeping that follows a pass: layers (focus in / back), toasts
    /// (timers), field echoes, and the events the core raised on its own.
    public func pass() {
        guard width > 0, !inPass else { return }
        inPass = true
        let t0 = DispatchTime.now().uptimeNanoseconds
        _ = surface.setViewport(width: Float(width), height: Float(viewportHeight), maxHeight: maxHeight.map { Float($0) })
        var changed = Set<Int>()
        var calls = 0
        var layoutNs: UInt64 = 0
        var tries = 0
        var out: FfiLayout
        while true {
            let kinds = lastKinds
            if fixedMeasure, let fixed = try? surface.layoutFixed(sizesJson: nil, wrap: true) {
                out = fixed
            } else {
                let measurer = SurfaceMeasurer(theme: theme, mode: mode, extensions: extensions, kinds: kinds, generation: measureGeneration)
                out = surface.layout(measurer: measurer)
                calls += measurer.calls
            }
            layoutNs += out.layoutNs
            for i in out.visualChanges { changed.insert(Int(i)) }
            for i in out.delta.added { changed.insert(Int(i)) }
            for i in out.delta.changed { changed.insert(Int(i)) }
            if nodes.isEmpty || nodesDirty || out.delta.renumbered {
                readNodes()
                visualsDirty = true
            } else {
                patchNodes(out.delta)
            }
            tries += 1
            // An extension leaf measured before its kind was known: measure
            // it again now that the slots know it.
            let unknown = fixedMeasure ? [] : nodes.filter { n in !n.removed && n.extensionKind != nil && kinds[n.id] == nil }.map(\.index)
            if unknown.isEmpty || tries >= 3 { break }
            for i in unknown { _ = surface.markDirty(index: UInt32(i)) }
        }
        order = out.frames.map { Int($0.index) }
        var f = [CGRect](repeating: .zero, count: nodes.count)
        for fr in out.frames where Int(fr.index) < f.count { f[Int(fr.index)] = CGRect(fr) }
        for l in out.layers {
            for fr in l.frames where Int(fr.index) < f.count { f[Int(fr.index)] = CGRect(fr) }
        }
        frames = f
        var r = [Int](repeating: Int.max, count: nodes.count)
        var k = 0
        for i in order where i < r.count { r[i] = k; k += 1 }
        for l in out.layers { for fr in l.frames where Int(fr.index) < r.count { r[Int(fr.index)] = k; k += 1 } }
        rank = r
        let newLayers = out.layers.map(LayerInfo.init)
        var ls: [String: ListInfo] = [:]
        for l in out.lists { ls[l.id] = ListInfo(l) }
        if ls != lists { lists = ls }
        var sc: [Int: ScrollInfo] = [:]
        for s in out.scrolls { sc[Int(s.index)] = ScrollInfo(s) }
        if sc != scrolls { scrolls = sc }
        var pins: [Int: CGSize] = [:]
        for p in out.sticky { pins[Int(p.index)] = CGSize(width: CGFloat(p.dx), height: CGFloat(p.dy)) }
        if pins != sticky { sticky = pins }
        publishScrollJumps()
        if out.direction != direction { direction = out.direction }
        if out.breakpoint != breakpoint { breakpoint = out.breakpoint }
        readVisuals(visualsDirty || styles.count != nodes.count ? nil : changed)
        visualsDirty = false
        let size = CGSize(width: CGFloat(out.surfaceWidth), height: CGFloat(out.surfaceHeight))
        if size != surfaceSize { surfaceSize = size }
        stats = PassStats(layoutNs: layoutNs, wallNs: DispatchTime.now().uptimeNanoseconds - t0, upcalls: Int(out.upcalls), measureRounds: Int(out.measureRounds), measureCalls: calls, nodes: nodes.lazy.filter { !$0.removed }.count, restyled: Int(out.restyled), builtNodes: Int(out.builtNodes), rebuilt: out.rebuilt)
        layoutNeeded = false
        passCount += 1
        let newToasts = out.toasts.map(ToastInfo.init)
        if newToasts != toasts { toasts = newToasts }
        if newLayers != layers {
            let before = layers
            layers = newLayers
            layersChanged(from: before)
        } else {
            layersSettled()
        }
        syncToasts()
        reportUnknowns()
        echoFields()
        inPass = false
        // Bookkeeping above may have changed states (focus into a layer):
        // the core raised events or needs another pass. Bounded.
        passDepth += 1
        defer { passDepth -= 1 }
        guard passDepth < 6 else { return }
        let late = surface.takeEvents()
        if !late.isEmpty {
            dispatch(late)
        } else if layoutNeeded {
            pass()
        }
    }

    /// The core moved a scroll offset the native scroll view did not report
    /// (scrollIntoView, the keyboard, clamping): the view should follow.
    private func publishScrollJumps() {
        for (i, s) in scrolls {
            let r = reportedOffsets[i] ?? .zero
            if abs(r.x - s.offset.x) > 0.5 || abs(r.y - s.offset.y) > 0.5 {
                reportedOffsets[i] = s.offset
                focusSerial += 1
                scrollJumps[i] = ScrollJump(offset: s.offset, serial: focusSerial)
            }
        }
    }

    /// Every slot from the core (first pass, a compaction, a host edit).
    private func readNodes() {
        let raw = surface.nodes()
        var list: [NodeInfo] = []
        list.reserveCapacity(raw.count)
        for n in raw { list.append(NodeInfo(n)) }
        // `nodes()[i].index == i`; guard a sparse answer anyway.
        list.sort { $0.index < $1.index }
        nodes = list
        rebuildIndex()
        nodesDirty = false
    }

    /// Apply a pass's node delta: tombstone `removed`, refetch `added` +
    /// `changed` (growing the slot list once).
    private func patchNodes(_ delta: FfiDelta) {
        if delta.added.isEmpty && delta.removed.isEmpty && delta.changed.isEmpty {
            if structureVersion != surface.structureVersion() { structureVersion = surface.structureVersion() }
            return
        }
        var list = nodes
        var fetch = Set(delta.added.map(Int.init))
        fetch.formUnion(delta.changed.map(Int.init))
        let removed = Set(delta.removed.map(Int.init)).subtracting(fetch)
        if !removed.isEmpty {
            let gone = surface.nodesAt(indices: removed.sorted().map(UInt32.init))
            for n in gone where Int(n.index) < list.count { list[Int(n.index)] = NodeInfo(n) }
        }
        if !fetch.isEmpty {
            let fresh = surface.nodesAt(indices: fetch.sorted().map(UInt32.init))
            if let maxIndex = fresh.map({ Int($0.index) }).max(), maxIndex >= list.count {
                // New slots past the end: tombstones until fetched (rare gap).
                let missing = (list.count...maxIndex).map(UInt32.init)
                let pad = surface.nodesAt(indices: missing)
                var byIndex: [Int: NodeInfo] = [:]
                for n in pad { byIndex[Int(n.index)] = NodeInfo(n) }
                for i in list.count...maxIndex {
                    guard let n = byIndex[i] else { readNodes(); return }
                    list.append(n)
                }
            }
            for n in fresh where Int(n.index) < list.count { list[Int(n.index)] = NodeInfo(n) }
        }
        nodes = list
        rebuildIndex()
    }

    private func rebuildIndex() {
        var ids: [String: Int] = [:]
        var kinds: [String: String] = [:]
        ids.reserveCapacity(nodes.count)
        for n in nodes where !n.removed {
            ids[n.id] = n.index
            if let k = n.extensionKind { kinds[n.id] = k }
        }
        byId = ids
        lastKinds = kinds
        children = nodes.map(\.children)
        structureVersion = surface.structureVersion()
        pruneFields()
        pruneInteraction()
    }

    /// The resolved visuals and text styles: every slot (`nil`) or the
    /// changed ones; then the inherited text colours.
    private func readVisuals(_ changed: Set<Int>?) {
        var st = styles
        var ts = textStyles
        if let changed {
            for i in changed where i < st.count {
                st[i] = surface.visual(index: UInt32(i)).map(PaintStyle.init) ?? PaintStyle()
                if nodes[i].isLeaf, let t = surface.textStyle(index: UInt32(i)) { ts[i] = TextStyle(t, defaultFamily: theme?.sansFamily) } else { ts[i] = nil }
            }
        } else {
            st = surface.visuals().map(PaintStyle.init)
            if st.count < nodes.count { st += [PaintStyle](repeating: PaintStyle(), count: nodes.count - st.count) }
            ts = [:]
            // The core resolves a leaf's text style DURING the pass (the
            // recipe's font): read after it, never from pre-pass nodes.
            for n in nodes where n.isLeaf && !n.removed {
                if let t = surface.textStyle(index: UInt32(n.index)) { ts[n.index] = TextStyle(t, defaultFamily: theme?.sansFamily) }
            }
        }
        if changed == nil || changed?.isEmpty == false || inks.count != nodes.count {
            styles = st
            if ts != textStyles { textStyles = ts }
            computeInks()
        }
    }

    /// Text colour inheritance in TREE order (slots are not pre-order).
    private func computeInks() {
        let fallback = theme?.ink(mode: mode) ?? (mode == .dark ? Color.white : Color.black)
        var ink = [Color](repeating: fallback, count: nodes.count)
        var roots: [Int] = order.first.map { [$0] } ?? []
        roots += layers.map(\.root)
        // Layer roots are found by the frames; a parentless slot is a root too.
        var stack: [(Int, Color)] = roots.map { ($0, fallback) }
        var seen = Set<Int>()
        while let (i, inherited) = stack.popLast() {
            guard i < nodes.count, seen.insert(i).inserted else { continue }
            let own = styles[safe: i]?.color ?? inherited
            ink[i] = own
            for c in nodes[i].children { stack.append((c, own)) }
        }
        for n in nodes where !seen.contains(n.index) && !n.removed {
            // Not reachable from a painted root this pass (a closed layer's
            // nodes): own colour, else the parent's as computed.
            if let c = styles[safe: n.index]?.color { ink[n.index] = c } else if let p = n.parent, p < ink.count { ink[n.index] = ink[p] }
        }
        inks = ink
    }

    private func reportUnknowns() {
        for n in nodes where !n.removed && n.component == "Unknown" && !unknownReported.contains(n.id) {
            unknownReported.insert(n.id)
            host.onUnknown(component: n.props.str("component").isEmpty ? n.component : n.props.str("component"), catalogId: n.catalogId ?? n.props.str("catalogId"), id: n.id)
        }
    }

    private func pruneInteraction() {
        for id in Array(interaction.keys) where byId[id] == nil && !id.hasSuffix(".layer") {
            interaction[id] = nil
        }
        if let f = focusedId, byId[f] == nil { focusedId = nil }
    }

    // MARK: - Lookups

    public func index(of id: String) -> Int? { byId[id] }

    /// A live node (tombstones answer nil).
    public func node(_ index: Int) -> NodeInfo? {
        guard index >= 0 && index < nodes.count, !nodes[index].removed else { return nil }
        return nodes[index]
    }

    public func node(id: String) -> NodeInfo? { byId[id].flatMap(node) }

    /// The main tree's root slot.
    public var rootIndex: Int? { order.first }

    public func frame(_ index: Int) -> CGRect { index >= 0 && index < frames.count ? frames[index] : .zero }

    public func style(_ index: Int) -> PaintStyle { index >= 0 && index < styles.count ? styles[index] : PaintStyle() }

    public func ink(_ index: Int) -> Color { index >= 0 && index < inks.count ? inks[index] : (theme?.ink(mode: mode) ?? .primary) }

    public func textStyle(_ index: Int) -> TextStyle { textStyles[index] ?? .body }

    /// A node's position in paint (= reading) order: the main tree, then
    /// each layer in stacking order.
    public func paintRank(_ index: Int) -> Int { index >= 0 && index < rank.count ? rank[index] : Int.max }

    /// The `accessibilitySortPriority` that makes VoiceOver read in paint
    /// order (higher reads first).
    public func accessibilityPriority(_ index: Int) -> Double {
        let r = paintRank(index)
        return r == Int.max ? 0 : Double(rank.count - r)
    }

    /// The scroll container at a slot (nil = it does not scroll).
    public func scroll(_ index: Int) -> ScrollInfo? { scrolls[index] }

    /// The owner node of a synthetic part (else the node itself).
    public func owner(of index: Int) -> NodeInfo? {
        guard let n = node(index) else { return nil }
        if let o = n.owner, let oi = byId[o] { return nodes[oi] }
        return n
    }

    public func ownerProps(_ index: Int) -> Props { owner(of: index)?.props ?? [:] }

    /// A part's look under the surface's theme (empty in geometry mode).
    public func part(_ component: String, _ part: String, props: Props, states: [String] = []) -> PartStyle {
        theme?.part(component, part, props: props, states: states, mode: mode) ?? .empty
    }

    public func color(_ name: String) -> Color? { theme?.color(name, mode: mode) }

    public func spacing(_ name: String) -> CGFloat { theme?.spacing(name) ?? GeometrySpacing.value(name) }

    public func control(_ name: String, _ fallback: CGFloat) -> CGFloat { theme?.control(name, fallback) ?? fallback }

    /// The interaction states the painter set on a node (`hover`,
    /// `pressed`, `focus`, `focus-visible`, `dragover`).
    public func states(of id: String) -> [String] { interaction[id]?.states ?? [] }

    /// Does this node show the keyboard focus ring (`focus-visible`,
    /// contract §2: keyboard focus only, never a press)?
    public func focusVisible(_ id: String) -> Bool { interaction[id]?.focusVisible ?? false }

    /// Is a node (or its owner, label parts aside) disabled?
    func isDisabled(_ index: Int) -> Bool {
        guard let n = node(index) else { return true }
        if n.disabled || n.props.flag("loading") { return true }
        if n.part != "label", let o = owner(of: index), o.index != n.index, o.disabled || o.props.flag("loading") { return true }
        return false
    }

    /// The content height a scrolling container shows (its children's extent).
    public func contentHeight(_ index: Int) -> CGFloat {
        if let n = node(index), let l = lists[n.id], l.windowed, !l.horizontal { return l.contentHeight }
        if let s = scrolls[index] { return s.contentSize.height }
        let origin = frame(index).origin
        return (children[safe: index] ?? []).map { frame($0).maxY - origin.y }.max() ?? 0
    }

    /// The content width a scrolling container shows.
    public func contentWidth(_ index: Int) -> CGFloat {
        // A horizontal windowed list (round 2): the core's content extent is its width.
        if let n = node(index), let l = lists[n.id], l.windowed, l.horizontal { return l.contentHeight }
        if let s = scrolls[index] { return s.contentSize.width }
        let origin = frame(index).origin
        return (children[safe: index] ?? []).map { frame($0).maxX - origin.x }.max() ?? 0
    }
}

/// A request to move platform focus to a node.
public struct FocusRequest: Equatable, Sendable {
    public let id: String
    public let index: Int
    /// From the keyboard (show the ring) or programmatic (a dialog opening,
    /// a failed submit, a command).
    public let keyboard: Bool
    public let serial: Int
}

/// An `announce` for the platform's live region.
public struct Announcement: Equatable, Sendable {
    public let text: String
    /// `polite` | `assertive`.
    public let live: String
    public let serial: Int
}

/// A file picker request (`pickFiles`, a FileUpload drop zone pressed).
public struct FilePickRequest: Equatable, Sendable, Identifiable {
    public var id: String { componentId }
    public let surfaceId: String
    /// The FileUpload's id.
    public let componentId: String
    /// The `accept` prop (MIME types / extensions, comma-separated).
    public let accept: String?
    public let multiple: Bool
}

/// A programmatic scroll of one container (the core moved its offset).
public struct ScrollJump: Equatable, Sendable {
    public let offset: CGPoint
    public let serial: Int
}

extension Array {
    subscript(safe i: Int) -> Element? { i >= 0 && i < count ? self[i] : nil }
}
