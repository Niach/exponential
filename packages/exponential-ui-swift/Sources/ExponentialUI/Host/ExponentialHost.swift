import SwiftUI
import Observation
import ExponentialUICore

/// What an `ExponentialHost` is built from (VAPP-91, `catalog/host.json`).
public struct HostOptions {
    /// Messages in, client messages out (nil = a local-only host fed through
    /// `receive`).
    public var transport: Transport?
    /// Host functions by name (`functionCall` actions outside the catalog's
    /// built-ins), gated by `policy.functions`.
    public var functions: [String: HostFunction]
    /// Binding source resolvers by scheme (`bindDataModel`).
    public var sources: [String: SourceResolver]
    public var extensions: [HostExtension]
    /// Declarative packages (JSON) installed at start (`applyTemplate`).
    public var packages: [String]
    public var policy: HostPolicy
    /// The theme and mode every surface the host creates starts with.
    public var theme: ThemeHandle?
    public var mode: Mode
    /// Locale, strings, `system` mode, density, contrast, font scale… for
    /// every surface (nil = the defaults with `mode`).
    public var settings: SurfaceSettings?
    /// Icons, fonts, markdown, input observers (actions, functions, urls and
    /// media route through the host first).
    public var plugin: HostPlugin?
    /// Every client message that leaves (after the transport got it).
    public var onSend: ((String) -> Void)?
    /// Every op the host performs (tests, logging).
    public var onOp: ((JSONValue) -> Void)?

    public init(
        transport: Transport? = nil,
        functions: [String: HostFunction] = [:],
        sources: [String: SourceResolver] = [:],
        extensions: [HostExtension] = [],
        packages: [String] = [],
        policy: HostPolicy = HostPolicy(),
        theme: ThemeHandle? = ThemeHandle.builtin(defaultThemeId()),
        mode: Mode = .light,
        settings: SurfaceSettings? = nil,
        plugin: HostPlugin? = nil,
        onSend: ((String) -> Void)? = nil,
        onOp: ((JSONValue) -> Void)? = nil
    ) {
        self.transport = transport
        self.functions = functions
        self.sources = sources
        self.extensions = extensions
        self.packages = packages
        self.policy = policy
        self.theme = theme
        self.mode = mode
        self.settings = settings
        self.plugin = plugin
        self.onSend = onSend
        self.onOp = onOp
    }
}

/// The platform host runtime (the Swift mirror of the TS `ExponentialHost`):
/// owns the transport, the core's `HostRouter`, one `SurfaceModel` per
/// surface, the source subscriptions, the function registry and the policy
/// hooks. The router (Rust) decides; this performs its ops. Paint a surface
/// with `HostSurface(host:surfaceId:)`.
@MainActor
@Observable
public final class ExponentialHost {
    @ObservationIgnored public let router: HostRouter
    /// The live surfaces by id.
    public private(set) var surfaces: [String: SurfaceModel] = [:]
    /// Their ids in creation order.
    public private(set) var surfaceIds: [String] = []
    public private(set) var status: TransportStatus = .closed
    public private(set) var statusDetail: String?
    /// The last UNSUPPORTED_CATALOG id seen (the catalog-update banner).
    public private(set) var unsupportedCatalog: String?
    public private(set) var theme: ThemeHandle?
    public private(set) var mode: Mode
    public private(set) var settings: SurfaceSettings?

    @ObservationIgnored private var options: HostOptions
    @ObservationIgnored private var functions: [String: HostFunction]
    @ObservationIgnored private var sources: [String: SourceResolver]
    @ObservationIgnored private var extensions: [HostExtension]
    @ObservationIgnored private var packages: [String: JSONValue] = [:]
    @ObservationIgnored private var subscriptions: [String: [() -> Void]] = [:]
    @ObservationIgnored private var sendDataModel: Set<String> = []
    @ObservationIgnored private lazy var bridge = HostBridge(host: self)

    public init(_ options: HostOptions = HostOptions()) {
        self.options = options
        self.functions = options.functions
        var sources: [String: SourceResolver] = [:]
        for (k, v) in options.sources { sources[k.lowercased()] = v }
        self.sources = sources
        var exts: [HostExtension] = []
        for e in options.extensions where !exts.contains(where: { $0.id == e.id }) { exts.append(e) }
        self.extensions = exts
        self.theme = options.theme
        self.mode = options.mode
        self.settings = options.settings
        self.router = HostRouter(extensionIds: exts.map(\.id))
        for pkg in options.packages { installPackage(pkg) }
    }

    /// False for a local-only host (packages, in-memory feeds).
    public var hasTransport: Bool { options.transport != nil }

    // MARK: - negotiation + registration

    public var supportedCatalogIds: [String] { router.supportedCatalogIds() }

    /// A2UI `a2uiClientCapabilities` (`{"v0.9": {supportedCatalogIds}}`).
    public var clientCapabilities: JSONValue { JSONValue.parse(clientCapabilitiesJson(extensionIds: extensions.map(\.id))) }

    public var extensionIds: [String] { extensions.map(\.id) }

    public func registerExtension(_ ext: HostExtension) {
        guard !extensions.contains(where: { $0.id == ext.id }) else { return }
        extensions.append(ext)
        router.registerExtension(id: ext.id)
        for model in surfaces.values { try? model.register(extension: ext.json, painters: ext.painters) }
    }

    public func registerFunction(_ name: String, _ fn: @escaping HostFunction) {
        functions[name] = fn
    }

    public func registerSource(_ scheme: String, _ resolver: @escaping SourceResolver) {
        sources[scheme.lowercased()] = resolver
    }

    /// Install a declarative package (JSON); returns its issues
    /// (`[{path, message}]`, installed only when empty).
    @discardableResult
    public func installPackage(_ json: String) -> [JSONValue] {
        let issues = JSONValue.parse((try? router.installPackage(packageJson: json)) ?? #"[{"path":"","message":"not JSON"}]"#).array ?? []
        let pkg = JSONValue.parse(json)
        if issues.isEmpty, let id = pkg["id"]?.string { packages[id] = pkg }
        return issues
    }

    public func surface(_ id: String) -> SurfaceModel? { surfaces[id] }

    /// The theme every surface paints with (live).
    public func setTheme(_ theme: ThemeHandle) {
        self.theme = theme
        for m in surfaces.values { m.setTheme(theme) }
    }

    public func setMode(_ mode: Mode) {
        self.mode = mode
        for m in surfaces.values { m.setMode(mode) }
    }

    /// The settings every surface uses (live; nil = the defaults with `mode`).
    public func setSettings(_ settings: SurfaceSettings?) {
        self.settings = settings
        for m in surfaces.values { m.setSettings(settings ?? SurfaceSettings(mode: mode == .dark ? .dark : .light)) }
    }

    // MARK: - transport

    public func connect() {
        guard let t = options.transport else { return }
        t.start(receive: { [weak self] m in self?.receive(m) }, status: { [weak self] status, detail in
            self?.status = status
            self?.statusDetail = detail
        })
    }

    public func close() {
        options.transport?.close()
        for id in surfaceIds { perform(.object(["op": .string("delete"), "surfaceId": .string(id)])) }
    }

    /// One server message (JSON text); returns the ops performed.
    @discardableResult
    public func receive(_ messageJSON: String) -> [JSONValue] {
        let ops = JSONValue.parse(router.route(messageJson: messageJSON)).array ?? []
        for op in ops { perform(op) }
        return ops
    }

    @discardableResult
    public func receive(_ message: JSONValue) -> [JSONValue] {
        receive(message.json)
    }

    /// A client message (JSON text) out over the transport.
    public func send(_ messageJSON: String) {
        if let t = options.transport {
            Task { try? await t.send(messageJSON) }
        }
        options.onSend?(messageJSON)
    }

    private func perform(_ op: JSONValue) {
        options.onOp?(op)
        let surfaceId = op["surfaceId"]?.string ?? ""
        switch op["op"]?.string {
        case "create":
            unbind(surfaceId)
            // Round 4: the server's `createSurface.theme` wins; an unusable
            // one never fails the host: the surface paints with the host's
            // theme and the server hears VALIDATION_FAILED (the TS / gpui rule).
            var surfaceTheme = theme
            if let t = op["theme"], t != .null {
                var refused: [String] = []
                if let named = t.string {
                    if let builtin = ThemeHandle.builtin(named) { surfaceTheme = builtin } else { refused = ["unknown built-in theme \"\(named)\""] }
                } else {
                    let loaded = ThemeHandle.loadOrDefault(json: t.json) { issues in refused = issues.map { "\($0.path): \($0.message)" } }
                    if refused.isEmpty { surfaceTheme = loaded }
                }
                if !refused.isEmpty {
                    send(errorMessageJson(code: "VALIDATION_FAILED", surfaceId: surfaceId, message: "createSurface.theme is unusable: \(refused.joined(separator: "; "))", path: "/createSurface/theme"))
                }
            }
            var o = SurfaceOptions(catalogId: op["catalogId"]?.string ?? coreCatalogId(), theme: surfaceTheme, mode: mode, settings: settings)
            o.rounding = false
            guard let model = try? SurfaceModel(id: surfaceId, options: o, host: bridge) else { return }
            for e in extensions { try? model.register(extension: e.json, painters: e.painters) }
            if op["sendDataModel"]?.bool == true { sendDataModel.insert(surfaceId) } else { sendDataModel.remove(surfaceId) }
            surfaces[surfaceId] = model
            if !surfaceIds.contains(surfaceId) { surfaceIds.append(surfaceId) }
        case "components":
            guard let model = surfaces[surfaceId] else { return }
            _ = try? model.setComponents(json: (op["components"] ?? .array([])).json)
        case "data":
            surfaces[surfaceId]?.setData(path: op["path"]?.string ?? "", value: op["value"])
        case "bind":
            let path = op["path"]?.string ?? ""
            guard surfaces[surfaceId] != nil, let source = BindingSource.parse(op["source"]?.string ?? "") else { return }
            guard let resolver = sources[source.scheme] else {
                send(errorMessageJson(code: "VALIDATION_FAILED", surfaceId: surfaceId, message: "no resolver for the source scheme \(source.scheme)", path: path.isEmpty ? "/" : path))
                return
            }
            let cancel = resolver(source) { [weak self] value in
                self?.surfaces[surfaceId]?.setData(path: path, value: value)
            }
            if let cancel { subscriptions[surfaceId, default: []].append(cancel) }
        case "delete":
            unbind(surfaceId)
            surfaces[surfaceId] = nil
            surfaceIds.removeAll { $0 == surfaceId }
            sendDataModel.remove(surfaceId)
        case "send":
            let message = op["message"] ?? .null
            if let err = message["error"], err["code"]?.string == "UNSUPPORTED_CATALOG" {
                let text = err["message"]?.string ?? ""
                unsupportedCatalog = text.hasPrefix("catalog ") ? String(text.dropFirst(8).prefix { !$0.isWhitespace }) : text
            }
            send(message.json)
        default:
            break
        }
    }

    private func unbind(_ surfaceId: String) {
        for cancel in subscriptions[surfaceId] ?? [] { cancel() }
        subscriptions[surfaceId] = nil
    }

    // MARK: - interactions out

    /// A component's server event as the A2UI client action message, sent.
    @discardableResult
    public func action(surfaceId: String, componentId: String, name: String, context: JSONValue = .object([:]), payload: JSONValue? = nil, timestamp: Date = Date()) -> String {
        let ts = ISO8601DateFormatter.withFractionalSeconds.string(from: timestamp)
        let message = (try? actionMessageJson(surfaceId: surfaceId, componentId: componentId, name: name, contextJson: context.json, payloadJson: payload.flatMap { $0.isNull ? nil : $0.json }, timestamp: ts)) ?? "{}"
        send(message)
        return message
    }

    /// The policy decision for a call, before any consent hook (a template
    /// surface's package narrows it).
    public func decide(surfaceId: String, name: String) -> FunctionDecision {
        let registered = functions[name] != nil
        let hostPolicy = PolicyJSON.encode(options.policy.functions)
        var decision = (try? decideFunction(policyJson: hostPolicy, name: name, registered: registered)) ?? "deny"
        if let pkgId = router.packageIdOf(surfaceId: surfaceId), let pkg = packages[pkgId] {
            let fns = pkg["functions"].map(\.json)
            if let policy = try? packagePolicyJson(functionsJson: fns), let narrowed = try? decideFunction(policyJson: policy, name: name, registered: registered) {
                decision = (try? combineDecisions(a: decision, b: narrowed)) ?? "deny"
            }
        }
        return FunctionDecision(rawValue: decision) ?? .deny
    }

    /// An action `functionCall`: `openUrl` through the URL policy, anything
    /// else through the gate, the consent hook and the registry.
    @discardableResult
    public func callFunction(_ call: FunctionCallInfo) async -> FunctionOutcome {
        if call.name == "openUrl" {
            return FunctionOutcome(decision: openURL(call.args["url"]?.string ?? "") ? .allow : .deny)
        }
        var decision = decide(surfaceId: call.surfaceId, name: call.name)
        if decision == .notFound {
            send(errorMessageJson(code: "FUNCTION_NOT_FOUND", surfaceId: call.surfaceId, message: "no function \(call.name)", path: nil))
            return FunctionOutcome(decision: decision)
        }
        if decision == .ask {
            let ok = await options.policy.onFunctionCall?(call) ?? false
            if !ok { decision = .deny }
        }
        if decision == .deny {
            send(errorMessageJson(code: "FUNCTION_DENIED", surfaceId: call.surfaceId, message: "\(call.name) was not allowed", path: nil))
            return FunctionOutcome(decision: .deny)
        }
        guard let fn = functions[call.name] else { return FunctionOutcome(decision: .allow) }
        do {
            return FunctionOutcome(decision: .allow, result: try await fn(call.args, call))
        } catch {
            return FunctionOutcome(decision: .allow, error: error.localizedDescription)
        }
    }

    /// openUrl / Link through the URL policy. True when it opened.
    @discardableResult
    public func openURL(_ url: String) -> Bool {
        var policy = options.policy.urls ?? UrlPolicy()
        if policy.baseUrl == nil { policy.baseUrl = options.policy.media?.baseUrl }
        guard let json = try? decideUrlJson(policyJson: PolicyJSON.encode(policy), url: url) else { return false }
        let d = JSONValue.parse(json)
        guard d["allowed"]?.bool == true, let abs = d["url"]?.string, let u = URL(string: abs) else { return false }
        if let open = options.policy.openUrl {
            open(u)
        } else {
            #if canImport(UIKit)
            UIApplication.shared.open(u)
            #elseif canImport(AppKit)
            NSWorkspace.shared.open(u)
            #endif
        }
        return true
    }

    /// The image loader's request: the absolute url + the media rules'
    /// headers (nil when the url does not resolve).
    public func mediaRequest(_ src: String) -> URLRequest? {
        let options = PolicyJSON.encode(self.options.policy.media ?? MediaOptions()) ?? "{}"
        guard let json = try? mediaRequestJson(url: src, optionsJson: options), let r = JSONValue.parse(json).object, let u = r["url"]?.string.flatMap(URL.init(string:)) else { return nil }
        var request = URLRequest(url: u)
        for (k, v) in r["headers"]?.object ?? [:] { request.setValue(v.displayText, forHTTPHeaderField: k) }
        return request
    }

    /// The plugin a surface model talks to (tests).
    public var plugin: HostPlugin { bridge }

    var basePlugin: HostPlugin? { options.plugin }
}

extension ISO8601DateFormatter {
    static let withFractionalSeconds: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f
    }()
}

/// The renderer callbacks that route through the host: actions become A2UI
/// client messages, host functions pass the gate, urls the URL policy, media
/// the loader; the rest is the base plugin's.
@MainActor
final class HostBridge: HostPlugin {
    weak var host: ExponentialHost?

    init(host: ExponentialHost) {
        self.host = host
    }

    private var base: HostPlugin? { host?.basePlugin }

    func icon(_ name: String, size: CGFloat) -> AnyView? { base?.icon(name, size: size) }

    func onAction(_ event: SurfaceActionEvent) {
        host?.action(surfaceId: event.surfaceId, componentId: event.componentId, name: event.name, context: event.context, payload: event.payload)
        base?.onAction(event)
    }

    func onInput(_ event: SurfaceInputEvent) { base?.onInput(event) }

    func onFunctionCall(_ call: SurfaceFunctionCall) {
        guard let host else { return }
        Task { @MainActor in
            _ = await host.callFunction(call)
            self.base?.onFunctionCall(call)
        }
    }

    func openUrl(_ url: String) { host?.openURL(url) }

    func resolveUrl(_ src: String) -> String { base?.resolveUrl(src) ?? src }

    func mediaRequest(_ src: String) -> URLRequest? { host?.mediaRequest(resolveUrl(src)) }

    func onUnknown(component: String, catalogId: String?, id: String) { base?.onUnknown(component: component, catalogId: catalogId, id: id) }

    func fontFamily(_ name: String) -> String? { base?.fontFamily(name) }

    func markdown(_ text: String, width: CGFloat) -> AnyView? { base?.markdown(text, width: width) }

    func onUpload(_ event: SurfaceUploadEvent) { base?.onUpload(event) }

    func pickFiles(_ request: FilePickRequest) -> Bool { base?.pickFiles(request) ?? false }

    func announce(text: String, live: String) { base?.announce(text: text, live: live) }

    func copy(_ text: String) -> Bool { base?.copy(text) ?? false }
}
