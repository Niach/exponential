import SwiftUI
import Observation
import ExponentialUICore
import ExponentialUIPrimitives

/// How overlays present.
public enum OverlayPresentation: Sendable {
    /// Dialog / Drawer as sheets, Popover as a popover, DropdownMenu as a
    /// menu, Tooltip painted (iOS default).
    case native
    /// Everything painted inside the surface at the core's frames with a
    /// scrim (snapshots, macOS embedders that own their windows).
    case painted
}

/// Options of a surface model.
public struct SurfaceOptions: Sendable {
    public var catalogId: String
    /// A built-in theme id (`exponential` default); nil = geometry mode.
    public var theme: ThemeHandle?
    public var mode: Mode
    public var overlays: OverlayPresentation
    /// Round frames to whole points (off: fractional, like the web).
    public var rounding: Bool

    public init(catalogId: String = coreCatalogId(), theme: ThemeHandle? = ThemeHandle.builtin(defaultThemeId()), mode: Mode = .dark, overlays: OverlayPresentation = .native, rounding: Bool = false) {
        self.catalogId = catalogId
        self.theme = theme
        self.mode = mode
        self.overlays = overlays
        self.rounding = rounding
    }
}

/// ONE surface: owns the core `Surface`, runs the layout passes with the
/// painter's measurer, caches the per-structure node data and the
/// per-pass frames and visuals, and holds every bit of interaction state
/// (mirrors, pressed/hover/focus, text fields, open layers). The views
/// observe it; `ExponentialSurface` is its SwiftUI face.
@MainActor
@Observable
public final class SurfaceModel {
    public let id: String
    @ObservationIgnored public let surface: Surface
    @ObservationIgnored public var host: HostPlugin
    @ObservationIgnored public let options: SurfaceOptions
    @ObservationIgnored let extensions: ExtensionRegistry

    public private(set) var theme: ThemeHandle?
    public private(set) var mode: Mode
    /// Every layout node, pre-order (= paint order = accessibility order).
    public private(set) var nodes: [NodeInfo] = []
    public private(set) var children: [[Int]] = []
    public private(set) var byId: [String: Int] = [:]
    public private(set) var structureVersion: UInt64 = 0
    /// Frames in SURFACE coordinates by node index (layers included).
    public private(set) var frames: [CGRect] = []
    public private(set) var styles: [PaintStyle] = []
    /// The text colour per node (own, else inherited, else `foreground`).
    public private(set) var inks: [Color] = []
    public private(set) var textStyles: [Int: TextStyle] = [:]
    public private(set) var layers: [LayerInfo] = []
    public private(set) var lists: [String: ListInfo] = [:]
    public private(set) var surfaceSize: CGSize = .zero
    public private(set) var passCount = 0
    public private(set) var stats = PassStats()
    /// Tokens for the generic primitives under the surface's theme.
    public private(set) var primitiveTokens = PrimitiveTokens.system

    /// The host's width (the view's geometry), visible height and card bound.
    @ObservationIgnored public internal(set) var width: CGFloat = 0
    @ObservationIgnored public internal(set) var viewportHeight: CGFloat = 0
    @ObservationIgnored public internal(set) var maxHeight: CGFloat?
    @ObservationIgnored var nodesDirty = true
    @ObservationIgnored var layoutNeeded = true
    @ObservationIgnored var mirrors: [String: Mirror] = [:]
    @ObservationIgnored var interaction: [String: InteractionState] = [:]
    @ObservationIgnored var pressedId: String?
    @ObservationIgnored var revisions: [String: Int] = [:]
    @ObservationIgnored var fields: [String: FieldState] = [:]
    @ObservationIgnored var layerReturn: [String: String] = [:]
    @ObservationIgnored var justDismissed: String?
    @ObservationIgnored var tooltipTask: Task<Void, Never>?
    @ObservationIgnored var measureGeneration: UInt64 = 0
    @ObservationIgnored var unknownReported: Set<String> = []
    /// Tests and geometry suites: lay out with the core's fixed fake
    /// measure (8 px per character, 20 px lines) instead of TextKit.
    @ObservationIgnored public var fixedMeasure = false
    /// The open Select / DatePicker popup (the field node id).
    var popup: String?
    /// The field ids in the order they got focus (the focused one last).
    var focusedField: String?
    /// Slider drags in flight (track id → value).
    var drags: [String: Double] = [:]
    /// Mirrored controls bumped (views re-read their part visuals).
    var mirrorGeneration = 0

    public init(id: String, options: SurfaceOptions = SurfaceOptions(), host: HostPlugin? = nil) throws {
        self.id = id
        self.options = options
        self.host = host ?? NoHost()
        self.theme = options.theme
        self.mode = options.mode
        self.extensions = ExtensionRegistry.shared
        self.surface = try Surface.withTheme(surfaceId: id, catalogId: options.catalogId, theme: options.theme?.theme, mode: options.mode.rawValue)
        surface.setRounding(on: options.rounding)
        for json in extensions.definitions {
            try? surface.registerExtension(extensionJson: json)
        }
        primitiveTokens = theme?.primitiveTokens(mode: mode) ?? .system
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
        invalidate(structure: true)
    }

    public var data: JSONValue { JSONValue.parse(surface.dataJson()) }
    public var issues: [JSONValue] { JSONValue.parse(surface.issuesJson()).array ?? [] }

    public func setTheme(_ theme: ThemeHandle) {
        self.theme = theme
        surface.setTheme(theme: theme.theme)
        primitiveTokens = theme.primitiveTokens(mode: mode)
        measureGeneration += 1
        invalidate(structure: true)
    }

    public func setMode(_ mode: Mode) {
        self.mode = mode
        try? surface.setMode(mode: mode.rawValue)
        primitiveTokens = theme?.primitiveTokens(mode: mode) ?? .system
        invalidate(structure: true)
    }

    /// Register an extension on THIS surface (the global registry covers
    /// new models; see `ExponentialUI.register`).
    public func register(extension json: String, painters: [String: ExtensionPainter]) throws {
        try surface.registerExtension(extensionJson: json)
        for (kind, painter) in painters { extensions.register(kind: kind, painter: painter) }
        invalidate(structure: true)
    }

    func invalidate(structure: Bool) {
        if structure { nodesDirty = true }
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

    /// Run one layout pass (the core, the measurer, the caches).
    public func pass() {
        guard width > 0 else { return }
        let t0 = DispatchTime.now().uptimeNanoseconds
        _ = surface.setViewport(width: Float(width), height: Float(viewportHeight), maxHeight: maxHeight.map { Float($0) })
        if nodesDirty { readNodes() }
        let measurer = SurfaceMeasurer(theme: theme, mode: mode, extensions: extensions, kinds: extensionKinds, generation: measureGeneration)
        let out: FfiLayout
        if fixedMeasure, let fixed = try? surface.layoutFixed(sizesJson: nil, wrap: true) {
            out = fixed
        } else {
            out = surface.layout(measurer: measurer)
        }
        if out.structureVersion != structureVersion || nodesDirty { readNodes() }
        var f = [CGRect](repeating: .zero, count: nodes.count)
        for fr in out.frames where Int(fr.index) < f.count { f[Int(fr.index)] = CGRect(fr) }
        for l in out.layers {
            for fr in l.frames where Int(fr.index) < f.count { f[Int(fr.index)] = CGRect(fr) }
        }
        frames = f
        let newLayers = out.layers.map(LayerInfo.init)
        if newLayers != layers { layers = newLayers }
        var ls: [String: ListInfo] = [:]
        for l in out.lists { ls[l.id] = ListInfo(l) }
        if ls != lists { lists = ls }
        if passCount == 0 || !out.visualChanges.isEmpty || styles.count != nodes.count {
            readVisuals()
            // The core resolves a leaf's text style DURING the pass (the
            // recipe's font); read it after, never from the pre-pass nodes.
            readTextStyles()
        }
        let size = CGSize(width: CGFloat(out.surfaceWidth), height: CGFloat(out.surfaceHeight))
        if size != surfaceSize { surfaceSize = size }
        stats = PassStats(layoutNs: out.layoutNs, wallNs: DispatchTime.now().uptimeNanoseconds - t0, upcalls: Int(out.upcalls), measureRounds: Int(out.measureRounds), measureCalls: measurer.calls, nodes: nodes.count)
        layoutNeeded = false
        passCount += 1
        layersChanged()
        reportUnknowns()
        echoFields()
    }

    private var extensionKinds: [String: String] {
        var k: [String: String] = [:]
        for n in nodes { if let e = n.extensionKind { k[n.id] = e } }
        return k
    }

    private func readNodes() {
        let raw = surface.nodes()
        var list: [NodeInfo] = []
        list.reserveCapacity(raw.count)
        var ch = [[Int]](repeating: [], count: raw.count)
        var ids: [String: Int] = [:]
        for n in raw {
            let info = NodeInfo(n)
            if let p = info.parent, p < ch.count { ch[p].append(info.index) }
            ids[info.id] = info.index
            list.append(info)
        }
        nodes = list
        children = ch
        byId = ids
        structureVersion = surface.structureVersion()
        nodesDirty = false
        pruneFields()
    }

    private func readTextStyles() {
        var ts: [Int: TextStyle] = [:]
        for n in nodes where n.isLeaf {
            if let t = surface.textStyle(index: UInt32(n.index)) { ts[n.index] = TextStyle(t) }
        }
        if ts != textStyles { textStyles = ts }
    }

    private func readVisuals() {
        let raw = surface.visuals()
        var st = raw.map(PaintStyle.init)
        if st.count < nodes.count { st += [PaintStyle](repeating: PaintStyle(), count: nodes.count - st.count) }
        styles = st
        let fallback = theme?.ink(mode: mode) ?? (mode == .dark ? Color.white : Color.black)
        var ink = [Color](repeating: fallback, count: nodes.count)
        for n in nodes {
            if let c = st[n.index].color {
                ink[n.index] = c
            } else if let p = n.parent, p < n.index {
                ink[n.index] = ink[p]
            }
        }
        inks = ink
    }

    private func reportUnknowns() {
        for n in nodes where n.component == "Unknown" && !unknownReported.contains(n.id) {
            unknownReported.insert(n.id)
            host.onUnknown(component: n.props.str("component").isEmpty ? n.component : n.props.str("component"), catalogId: n.catalogId ?? n.props.str("catalogId"), id: n.id)
        }
    }

    // MARK: - Lookups

    public func index(of id: String) -> Int? { byId[id] }

    public func node(_ index: Int) -> NodeInfo? { index >= 0 && index < nodes.count ? nodes[index] : nil }

    public func frame(_ index: Int) -> CGRect { index >= 0 && index < frames.count ? frames[index] : .zero }

    public func style(_ index: Int) -> PaintStyle { index >= 0 && index < styles.count ? styles[index] : PaintStyle() }

    public func ink(_ index: Int) -> Color { index >= 0 && index < inks.count ? inks[index] : (theme?.ink(mode: mode) ?? .primary) }

    public func textStyle(_ index: Int) -> TextStyle { textStyles[index] ?? .body }

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

    public func states(of id: String) -> [String] { interaction[id]?.states ?? [] }

    /// Is a node (or its owner, label parts aside) disabled?
    func isDisabled(_ index: Int) -> Bool {
        guard let n = node(index) else { return true }
        if n.disabled { return true }
        if n.part != "label", let o = owner(of: index), o.index != n.index, o.disabled { return true }
        return false
    }

    /// The content height a scrolling container shows (its children's extent).
    public func contentHeight(_ index: Int) -> CGFloat {
        if let n = node(index), let l = lists[n.id], l.windowed { return l.contentHeight }
        let origin = frame(index).origin
        return (children[safe: index] ?? []).map { frame($0).maxY - origin.y }.max() ?? 0
    }
}

extension Array {
    subscript(safe i: Int) -> Element? { i >= 0 && i < count ? self[i] : nil }
}
