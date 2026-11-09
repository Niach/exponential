import SwiftUI
import ExponentialUICore

/// #EDE-65 lane M (`fixtures/embed.json`): an Embed paints a NESTED surface
/// in place, SANDBOXED. The pure half every painter shares (the twin of
/// `src/embed.ts` and the core's `eui_core::embed`): which `surface` value
/// is a card, the error box a bad one shows instead, and what an inner
/// event becomes when it reaches the host as the Embed's `action`.
public struct EmbedCard: Equatable, Sendable {
    /// The sound root as written (a nested node).
    public let root: JSONValue
    /// The card's OWN data model (never the host's).
    public let data: [String: JSONValue]

    public init(root: JSONValue, data: [String: JSONValue]) {
        self.root = root
        self.data = data
    }

    /// The Embed's one event: every inner `event` action reaches the host as it.
    public static let event = "action"

    /// What a bad `surface` paints instead (`fixtures/embed.json`
    /// `errorRoot`): an error Alert titled `$string.embedInvalid`; the host
    /// around it keeps painting.
    public static let errorRoot: JSONValue = .object([
        "id": .string("embed_error"),
        "component": .string("Alert"),
        "props": .object(["type": .string("error"), "title": .string("$string.embedInvalid")]),
    ])

    /// The Embed's resolved `surface` prop → its card, or nil when it is
    /// not one (not an object, no sound `root`, a non-object `data`, a
    /// `version` other than 1.0): the painter then shows `errorRoot`. A
    /// missing or null `data` is an empty model. An unknown component inside
    /// a sound root is NOT bad: the nested surface paints its Unknown
    /// placeholder.
    public static func card(_ surface: JSONValue?) -> EmbedCard? {
        guard let s = surface?.object else { return nil }
        if let v = s["version"], v != .string(a2uiVersion()) { return nil }
        guard let root = s["root"], soundNode(root, depth: 1) else { return nil }
        let data = s["data"] ?? .null
        if !data.isNull, data.object == nil { return nil }
        return EmbedCard(root: root, data: data.object ?? [:])
    }

    /// The card a painter mounts for `surface`: the card, else the error box.
    public static func mount(_ surface: JSONValue?) -> (card: EmbedCard, valid: Bool) {
        if let c = card(surface) { return (c, true) }
        return (EmbedCard(root: errorRoot, data: [:]), false)
    }

    /// An inner event → the Embed's `action` payload (merged over the
    /// author's context like any component payload): the inner name and
    /// context.
    public static func payload(name: String, context: JSONValue?) -> JSONValue {
        .object(["name": .string(name), "context": .object(context?.object ?? [:])])
    }

    /// The nested surface's id: the host's surface id and the Embed's
    /// (painted) component id, so two cards' inner ids never meet.
    public static func surfaceId(host: String, component: String) -> String {
        "\(host)/\(component)"
    }

    /// A structurally sound nested node: string non-empty `id` +
    /// `component`, `props` / `on` / `style` objects, `children` an array of
    /// nodes, `slots` an object of nodes, recursively (64 deep at most).
    static func soundNode(_ n: JSONValue, depth: Int) -> Bool {
        guard let o = n.object, depth <= 64 else { return false }
        guard let id = o["id"]?.string, !id.isEmpty, let component = o["component"]?.string, !component.isEmpty else { return false }
        for key in ["props", "on", "style"] {
            if let v = o[key], v.object == nil { return false }
        }
        if let c = o["children"] {
            guard let kids = c.array, kids.allSatisfy({ soundNode($0, depth: depth + 1) }) else { return false }
        }
        if let s = o["slots"] {
            guard let slots = s.object, slots.values.allSatisfy({ soundNode($0, depth: depth + 1) }) else { return false }
        }
        return true
    }
}

/// One mounted card: the nested surface of an Embed node and the `surface`
/// value it was mounted from (a changed value remounts fresh).
@MainActor
final class EmbedMount {
    let key: String
    let model: SurfaceModel
    let valid: Bool
    /// The nested surface's height the host measured last.
    var measuredHeight: CGFloat = 0

    init(key: String, model: SurfaceModel, valid: Bool) {
        self.key = key
        self.model = model
        self.valid = valid
    }
}

/// The nested surface's host: the SAME icons, fonts, media and url policy,
/// markdown hooks, announcements and clipboard as the host's; every inner
/// action fires the Embed's `action` on the host; inner HOST function
/// calls are denied (never forwarded, the action after one stops); inner
/// input edits and uploads stay inner.
@MainActor
final class EmbedHost: HostPlugin {
    weak var owner: SurfaceModel?
    let embedId: String

    init(owner: SurfaceModel, embedId: String) {
        self.owner = owner
        self.embedId = embedId
    }

    private var base: HostPlugin? { owner?.host }

    func icon(_ name: String, size: CGFloat) -> AnyView? { base?.icon(name, size: size) }

    func afterLayout(surfaceId: String) { owner?.embedLaidOut(embedId) }

    func onAction(_ event: SurfaceActionEvent) { owner?.embedAction(embedId, event) }

    // Denied: never forwarded; with an event after it, the event stops too.
    func onFunctionCall(_ call: SurfaceFunctionCall) {}
    func onFunctionCallThen(_ call: SurfaceFunctionCall, then: @escaping @MainActor () -> Void) {}

    func openUrl(_ url: String) { base?.openUrl(url) }

    func textLinks(_ text: String) -> [TextLinkSegment]? { base?.textLinks(text) }

    func imageSize(_ src: String) -> CGSize? { base?.imageSize(src) }

    var onImagePress: (@MainActor (String) -> Void)? { base?.onImagePress }

    func attachment(_ href: String) -> MarkdownAttachmentInfo? { base?.attachment(href) }

    var markdownOptions: MarkdownOptions { base?.markdownOptions ?? MarkdownOptions() }

    var urlPolicy: UrlPolicy? { base?.urlPolicy }

    var mediaOptions: MediaOptions? { base?.mediaOptions }

    func resolveUrl(_ src: String) -> String { base?.resolveUrl(src) ?? src }

    func mediaRequest(_ src: String) -> URLRequest? { base?.mediaRequest(src) }

    var appFile: AppFileLoader? { base?.appFile }

    func onPaintError(_ error: SurfacePaintError) { base?.onPaintError(error) }

    func onUnknown(component: String, catalogId: String?, id: String) { base?.onUnknown(component: component, catalogId: catalogId, id: id) }

    func fontFamily(_ name: String) -> String? { base?.fontFamily(name) }

    func markdown(_ text: String, width: CGFloat) -> AnyView? { base?.markdown(text, width: width) }

    func announce(text: String, live: String) { base?.announce(text: text, live: live) }

    func copy(_ text: String) -> Bool { base?.copy(text) ?? false }
}

extension SurfaceModel {
    /// The nested surface of the Embed node `nodeId` for its resolved
    /// `props` (mounted on first ask; a changed `surface` value remounts
    /// fresh). Its own data model and ids (surface id `<host>/<embed id>`),
    /// the host's theme, mode, settings (strings, locale, density) and
    /// platform; a bad surface mounts the error box.
    public func embedModel(id nodeId: String, props: Props) -> SurfaceModel? {
        let surface = props["surface"] ?? .null
        let key = surface.json
        if let m = embeds[nodeId], m.key == key {
            syncEmbed(m.model)
            return m.model
        }
        let mount = EmbedCard.mount(surface)
        let options = SurfaceOptions(catalogId: self.options.catalogId, theme: baseTheme, mode: mode, rounding: self.options.rounding, settings: settings)
        guard let nested = try? SurfaceModel(id: EmbedCard.surfaceId(host: id, component: nodeId), options: options, host: EmbedHost(owner: self, embedId: nodeId)) else { return nil }
        nested.fixedMeasure = fixedMeasure
        nested.memoizeMeasures = memoizeMeasures
        nested.setPlatform(platform)
        var valid = mount.valid
        if (try? nested.setNested(json: mount.card.root.json)) != nil {
            if valid { nested.setData(path: "", value: .object(mount.card.data)) }
        } else {
            // A sound root the core still refuses (a malformed template…).
            valid = false
            _ = try? nested.setNested(json: EmbedCard.errorRoot.json)
        }
        embeds[nodeId] = EmbedMount(key: key, model: nested, valid: valid)
        return nested
    }

    /// The mounted card of an Embed node: valid, or the error box.
    public func embedValid(id nodeId: String) -> Bool? { embeds[nodeId]?.valid }

    /// The host's theme and settings follow into a mounted card (its
    /// platform traits come from its own `ExponentialSurface`).
    private func syncEmbed(_ nested: SurfaceModel) {
        if let t = baseTheme, nested.baseTheme !== t { nested.setTheme(t) }
        if nested.settings != settings { nested.setSettings(settings) }
    }

    /// The measurer's question for an Embed leaf: its nested surface laid
    /// out at the content width (`nil` = the host's width: a card fills its
    /// line; `0` = min-content, nothing), as tall as its content.
    func embedSize(id nodeId: String, props: Props, wrap: CGFloat?) -> CGSize {
        if let w = wrap, w <= 0 { return .zero }
        let w = wrap ?? width
        guard w > 0, let nested = embedModel(id: nodeId, props: props), let mount = embeds[nodeId] else { return .zero }
        nested.setViewport(width: w, height: viewportHeight)
        let h = nested.surfaceSize.height
        mount.measuredHeight = h
        return CGSize(width: w, height: h)
    }

    /// The fixed measure's Embed answers (`layoutFixed` sizes): each card
    /// laid out at the width its Embed got in `out` (none yet = the host's).
    func fixedEmbedSizes(_ out: FfiLayout) -> [String: CGSize] {
        var widths: [Int: CGFloat] = [:]
        for fr in out.frames { widths[Int(fr.index)] = CGRect(fr).width }
        for l in out.layers { for fr in l.frames { widths[Int(fr.index)] = CGRect(fr).width } }
        var sizes: [String: CGSize] = [:]
        for n in nodes where !n.removed && n.component == "Embed" && n.part == nil {
            let w = widths[n.index].flatMap { $0 > 0 ? $0 : nil }
            sizes[n.id] = embedSize(id: n.id, props: n.props, wrap: w)
        }
        return sizes
    }

    /// `{id: {w, h}}` for `layoutFixed` (nil = none).
    static func fixedSizesJson(_ sizes: [String: CGSize]) -> String? {
        guard !sizes.isEmpty else { return nil }
        return JSONValue.object(sizes.mapValues { .object(["w": .number(Double($0.width)), "h": .number(Double($0.height))]) }).json
    }

    /// A card laid out on its own (an inner `set`, its platform): a new
    /// height measures its Embed again.
    func embedLaidOut(_ nodeId: String) {
        guard !inPass, let mount = embeds[nodeId], let i = byId[nodeId] else { return }
        let h = mount.model.surfaceSize.height
        guard abs(h - mount.measuredHeight) > 0.5 else { return }
        mount.measuredHeight = h
        remeasure(i)
    }

    /// An inner `event` action → the Embed's `action` with `{name, context}`
    /// (the core merges it over the author's context; no `on.action` =
    /// nothing reaches the host).
    func embedAction(_ nodeId: String, _ event: SurfaceActionEvent) {
        fire(id: nodeId, EmbedCard.event, payload: EmbedCard.payload(name: event.name, context: event.context))
    }

    /// Cards whose Embed left the surface unmount.
    func pruneEmbeds() {
        guard !embeds.isEmpty else { return }
        for key in Array(embeds.keys) {
            if let i = byId[key], nodes[i].component == "Embed" { continue }
            embeds[key] = nil
        }
    }
}
