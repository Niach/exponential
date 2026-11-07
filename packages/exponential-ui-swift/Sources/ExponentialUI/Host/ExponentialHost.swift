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
    /// VAPP-194: the member a package action runs as (its `me`).
    public var me: String?
    /// VAPP-194: the clock and the team zone a package action reads (nil =
    /// the wall clock, UTC).
    public var now: (() -> Date)?
    public var zone: HostZone?
    /// VAPP-194: a package action that landed nothing (a false `require`'s
    /// message, a refused write or host call): the host's failed-action toast.
    public var onActionFailed: ((ActionFailure) -> Void)?
    /// VAPP-218 (U1): open one of the surface's package screens
    /// (`app_navigate`, checked by `Screens.target`); the host's shell opens
    /// it. Nil = no shell, so a navigate is refused. True when it navigated.
    public var navigate: ((NavigateTarget) -> Bool)?
    /// #VAPP-252: `SurfaceOptions.liftsLayers` of every surface.
    public var liftsLayers: Bool
    /// Round 9 §6.5: the transport's far end reads renderer → agent messages:
    /// a surface an untrusted `createSurface` made has an agent.
    public var agent: Bool

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
        onOp: ((JSONValue) -> Void)? = nil,
        me: String? = nil,
        now: (() -> Date)? = nil,
        zone: HostZone? = nil,
        onActionFailed: ((ActionFailure) -> Void)? = nil,
        navigate: ((NavigateTarget) -> Bool)? = nil,
        liftsLayers: Bool = false,
        agent: Bool = false
    ) {
        self.liftsLayers = liftsLayers
        self.agent = agent
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
        self.me = me
        self.now = now
        self.zone = zone
        self.onActionFailed = onActionFailed
        self.navigate = navigate
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

    @ObservationIgnored fileprivate var options: HostOptions
    @ObservationIgnored private var functions: [String: HostFunction]
    @ObservationIgnored private var sources: [String: SourceResolver]
    @ObservationIgnored private var extensions: [HostExtension]
    @ObservationIgnored private var packages: [String: JSONValue] = [:]
    /// Per surface, one entry per bound path (VAPP-122: a re-bind of a path
    /// cancels the earlier subscription at that path).
    @ObservationIgnored private var subscriptions: [String: [(path: String, cancel: () -> Void)]] = [:]
    @ObservationIgnored private var sendDataModel: Set<String> = []
    /// `surfaceId\0componentId\0message` of the paint errors already sent.
    @ObservationIgnored private var paintErrors: Set<String> = []
    @ObservationIgnored private lazy var bridge = HostBridge(host: self)
    /// Round 9 §6.5: the routed calls (the core's `CallHub`).
    @ObservationIgnored private let calls: HostCalls
    /// The press waiting for each action call to the agent.
    @ObservationIgnored private var agentWaiters: [String: CheckedContinuation<JSONValue, Never>] = [:]
    @ObservationIgnored private var flushingCalls = false
    /// The one timer, armed at `calls.nextDeadlineMs()`.
    @ObservationIgnored private var deadlineTimer: Task<Void, Never>?
    @ObservationIgnored private var armedDeadline: Double?

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
        self.calls = HostCalls(agent: options.agent)
        calls.setRegistered(names: Array(options.functions.keys))
        try? calls.setPolicyJson(policyJson: PolicyJSON.encode(options.policy.functions))
        for pkg in options.packages { installPackage(pkg) }
    }

    /// False for a local-only host (packages, in-memory feeds).
    public var hasTransport: Bool { options.transport != nil }

    // MARK: - negotiation + registration

    public var supportedCatalogIds: [String] { router.supportedCatalogIds() }

    /// A2UI `rendererCapabilities` (`{"1.0": {supportedCatalogIds}}`).
    public var rendererCapabilities: JSONValue { JSONValue.parse((try? rendererCapabilitiesJson(extensionIds: extensions.map(\.id), inlineCatalogsJson: nil)) ?? "{}") }

    public var extensionIds: [String] { extensions.map(\.id) }

    public func registerExtension(_ ext: HostExtension) {
        guard !extensions.contains(where: { $0.id == ext.id }) else { return }
        extensions.append(ext)
        router.registerExtension(id: ext.id)
        for model in surfaces.values { try? model.register(extension: ext.json, painters: ext.painters) }
    }

    public func registerFunction(_ name: String, _ fn: @escaping HostFunction) {
        functions[name] = fn
        calls.setRegistered(names: Array(functions.keys))
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
        let ops: [JSONValue] = surfaceIds.map { .object(["op": .string("delete"), "surfaceId": .string($0)]) }
        ops.forEach(perform)
        observe(JSONValue.array(ops).json)
    }

    /// One agent message (JSON text); returns the ops performed. Round 9 §2:
    /// a transport is untrusted; only host code passes `trusted: true`
    /// (`applyTemplate`, `bindDataModel`).
    @discardableResult
    public func receive(_ messageJSON: String, trusted: Bool = false) -> [JSONValue] {
        // VAPP-206: `${today}`, `${now}`, `${month}` at this bind, in the team zone.
        let nowMs = options.now.map { $0().timeIntervalSince1970 * 1000 }
        let routed = HostParams.adding(to: messageJSON, nowMs: nowMs, zone: options.zone)
        let opsJson = router.routeJson(messageJson: routed, trusted: trusted)
        let ops = JSONValue.parse(opsJson).array ?? []
        for op in ops { perform(op) }
        observe(opsJson)
        return ops
    }

    @discardableResult
    public func receive(_ message: JSONValue, trusted: Bool = false) -> [JSONValue] {
        receive(message.json, trusted: trusted)
    }

    /// Round 9 §1.4: a fresh id to re-open a surface under: mint it, apply
    /// the template onto it, repaint with the new id, then `deleteSurface` the
    /// old one, so the old surface paints until the new one exists.
    public func mintSurfaceId(_ base: String) -> String {
        router.mintSurfaceId(base: base)
    }

    /// A client message (JSON text) out over the transport.
    public func send(_ messageJSON: String) {
        if let t = options.transport {
            let metadata = dataModelMetadata(messageJSON)
            Task {
                if let metadata {
                    try? await t.send(messageJSON, metadata: metadata)
                } else {
                    try? await t.send(messageJSON)
                }
            }
        }
        options.onSend?(messageJSON)
    }

    /// Round 9 §1.3: the metadata beside a message about a `sendDataModel` surface.
    private func dataModelMetadata(_ messageJSON: String) -> String? {
        guard !sendDataModel.isEmpty else { return nil }
        let message = JSONValue.parse(messageJSON)
        guard let id = ["action", "callAgentFunction", "error"].lazy.compactMap({ message[$0]?["surfaceId"]?.string }).first,
              sendDataModel.contains(id), let model = surfaces[id] else { return nil }
        return (try? calls.dataModelMetadataJson(messageJson: messageJSON, dataJson: model.surface.dataJson())) ?? nil
    }

    // MARK: - routed calls (round 9 §6.5–§6.8, §18.3)

    /// Every router answer, AFTER its ops ran: the hub follows the surfaces and
    /// settles the `respond` ops, then the one flush.
    private func observe(_ opsJson: String) {
        answerPresses((try? calls.observeJson(opsJson: opsJson, router: router)) ?? "[]")
        flushCalls()
    }

    /// `[SettledCall]` JSON: the presses waiting for those calls go on.
    private func answerPresses(_ settledJson: String) {
        for call in JSONValue.parse(settledJson).array ?? [] {
            if let id = call["functionCallId"]?.string, let waiter = agentWaiters.removeValue(forKey: id) {
                waiter.resume(returning: call)
            }
        }
    }

    /// What the hub queued: messages out, reactive effects run, surfaces whose
    /// routed value settled bound again, until all three are empty; then the
    /// timer. A flush a rebind's layout asks for is this loop's next round.
    private func flushCalls() {
        guard !flushingCalls else { return }
        flushingCalls = true
        defer { flushingCalls = false }
        while true {
            let outbox = JSONValue.parse(calls.takeOutboxJson()).array ?? []
            let effects = JSONValue.parse(calls.takeEffectsJson()).array ?? []
            let dirty = calls.takeDirty()
            if outbox.isEmpty, effects.isEmpty, dirty.isEmpty { break }
            for message in outbox { send(message.json) }
            for effect in effects { runEffect(effect) }
            for id in dirty { surfaces[id]?.rebind() }
        }
        armDeadline()
    }

    private func runEffect(_ effect: JSONValue) {
        guard let ticket = effect["ticket"]?.number.map({ UInt64($0) }) else { return }
        let call = SurfaceFunctionCall(surfaceId: effect["surfaceId"]?.string ?? "", componentId: "", name: effect["name"]?.string ?? "", args: effect["args"]?.object ?? [:])
        switch effect["effect"]?.string {
        case "ask":
            Task { @MainActor in
                let allowed = await options.policy.onFunctionCall?(call) ?? false
                calls.consent(ticket: ticket, allowed: allowed)
                flushCalls()
            }
        case "runLocal":
            Task { @MainActor in
                var value: String?
                var failure: String?
                do {
                    let answer = try await functions[call.name]?(call.args, call)
                    value = answer.map { (($0 as? JSONValue) ?? JSONValue(any: $0)).json }
                } catch {
                    failure = error.localizedDescription
                }
                try? calls.settleLocal(ticket: ticket, valueJson: value, error: failure)
                flushCalls()
            }
        default:
            break
        }
    }

    /// The hub's clock is the wall clock; a press always settles here (the
    /// task holds the host until its deadline, 30 s at most).
    private func armDeadline() {
        let next = calls.nextDeadlineMs()
        guard next != armedDeadline else { return }
        deadlineTimer?.cancel()
        deadlineTimer = nil
        armedDeadline = next
        guard let next else { return }
        let wait = max(0, next - Date().timeIntervalSince1970 * 1000)
        deadlineTimer = Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(wait + 1))
            guard !Task.isCancelled else { return }
            deadlineTimer = nil
            armedDeadline = nil
            answerPresses(calls.expireJson())
            flushCalls()
        }
    }

    /// Round 9 §5.2, §6.8: a layout may leave issues, failures or routed
    /// calls: to the hub, then out. The core reports no cheap "anything
    /// pending?" for failures, and this is as cheap as that read.
    func afterLayout(_ surfaceId: String) {
        guard let model = surfaces[surfaceId] else { return }
        calls.flushSurface(surfaceId: surfaceId, surface: model.surface)
        flushCalls()
    }

    /// An action call to the agent: the policy, the consent, one new
    /// `callAgentFunction`, its answer or the timeout (§6.5: never shared).
    private func callAgent(_ call: FunctionCallInfo) async -> FunctionOutcome {
        let args = JSONValue.object(call.args).json
        func press(_ consent: Bool?) -> JSONValue {
            JSONValue.parse((try? calls.pressJson(surfaceId: call.surfaceId, name: call.name, catalogId: nil, argsJson: args, consent: consent)) ?? #"{"route":"deny"}"#)
        }
        var route = press(nil)
        if route["route"]?.string == "ask" {
            let allowed = await options.policy.onFunctionCall?(call) ?? false
            route = press(allowed)
        }
        switch route["route"]?.string {
        case "agent":
            guard let id = route["functionCallId"]?.string else { break }
            let answer = await withCheckedContinuation { (waiter: CheckedContinuation<JSONValue, Never>) in
                agentWaiters[id] = waiter
                flushCalls()
            }
            return FunctionOutcome(decision: .allow, result: answer["value"], error: answer["error"]?.string)
        case "notFound":
            flushCalls()
            return FunctionOutcome(decision: .notFound)
        case "local":
            flushCalls()
            return await runLocal(call)
        default:
            break
        }
        flushCalls()
        return FunctionOutcome(decision: .deny)
    }

    /// `run`; §6.8: a local function that threw is an EXECUTION_ERROR.
    private func runLocal(_ call: FunctionCallInfo) async -> FunctionOutcome {
        let outcome = await run(call)
        if let error = outcome.error {
            calls.localFailed(surfaceId: call.surfaceId, message: error)
            flushCalls()
        }
        return outcome
    }

    private func perform(_ op: JSONValue) {
        options.onOp?(op)
        let surfaceId = op["surfaceId"]?.string ?? ""
        switch op["op"]?.string {
        case "create":
            unbind(surfaceId)
            forgetPaintErrors(surfaceId)
            // Round 9 §8: the server's `at_exponential_theme` wins; an unusable
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
                    send(errorMessageJson(code: "VALIDATION_FAILED", surfaceId: surfaceId, message: "createSurface.metadata.extensions.at_exponential_theme is unusable: \(refused.joined(separator: "; "))", path: "/createSurface/metadata/extensions/at_exponential_theme"))
                }
            }
            var o = SurfaceOptions(catalogId: op["catalogId"]?.string ?? coreCatalogId(), theme: surfaceTheme, mode: mode, settings: settings)
            o.rounding = false
            o.liftsLayers = options.liftsLayers
            guard let model = try? SurfaceModel(id: surfaceId, options: o, host: bridge) else { return }
            for e in extensions { try? model.register(extension: e.json, painters: e.painters) }
            if op["sendDataModel"]?.bool == true { sendDataModel.insert(surfaceId) } else { sendDataModel.remove(surfaceId) }
            surfaces[surfaceId] = model
            calls.attachSurface(surfaceId: surfaceId, surface: model.surface)
            if !surfaceIds.contains(surfaceId) { surfaceIds.append(surfaceId) }
        case "components":
            forgetPaintErrors(surfaceId, components: (op["components"]?.array ?? []).compactMap { $0["id"]?.string })
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
            let earlier = subscriptions[surfaceId] ?? []
            for sub in earlier where sub.path == path { sub.cancel() }
            var kept = earlier.filter { $0.path != path }
            let cancel = resolver(source) { [weak self] value in
                self?.surfaces[surfaceId]?.setData(path: path, value: value)
            }
            if let cancel { kept.append((path: path, cancel: cancel)) }
            subscriptions[surfaceId] = kept
        case "delete":
            unbind(surfaceId)
            forgetPaintErrors(surfaceId)
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
        for sub in subscriptions[surfaceId] ?? [] { sub.cancel() }
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

    /// An action `functionCall`: `openUrl` through the URL policy, `bind`
    /// through the router (VAPP-122), `actions_<name>` = the surface's
    /// package action (VAPP-194), anything else through the gate, the
    /// consent hook and the registry.
    @discardableResult
    public func callFunction(_ call: FunctionCallInfo) async -> FunctionOutcome {
        if let builtIn = runBuiltIn(call) { return builtIn }
        if call.name == Screens.navigateFunction { return navigate(call) }
        if let action = Actions.actionName(call: call.name) { return await runPackageAction(call, action: action) }
        // Round 9 §6.5: nothing local: the hub knows which surfaces have an
        // agent, and a built-in answers `local`.
        if options.agent, functions[call.name] == nil { return await callAgent(call) }
        let verdict = await gate(call)
        if verdict.decision != .allow { return verdict }
        return await runLocal(call)
    }

    /// The policy gate and the consent hook: `.allow`, or the refusal (sent
    /// as UNKNOWN_FUNCTION / INVALID_FUNCTION_CALL).
    private func gate(_ call: FunctionCallInfo) async -> FunctionOutcome {
        // VAPP-218: app_navigate is always allowed (checked when it runs).
        if call.name == Screens.navigateFunction { return FunctionOutcome(decision: .allow) }
        var decision = decide(surfaceId: call.surfaceId, name: call.name)
        if decision == .notFound {
            send(errorMessageJson(code: "UNKNOWN_FUNCTION", surfaceId: call.surfaceId, message: "no function \(call.name)", path: nil))
            return FunctionOutcome(decision: decision)
        }
        if decision == .ask {
            let ok = await options.policy.onFunctionCall?(call) ?? false
            if !ok { decision = .deny }
        }
        if decision == .deny {
            send(errorMessageJson(code: "INVALID_FUNCTION_CALL", surfaceId: call.surfaceId, message: "\(call.name) was not allowed", path: nil))
            return FunctionOutcome(decision: .deny)
        }
        return FunctionOutcome(decision: .allow)
    }

    /// VAPP-205: `openUrl` and `bind`, for a press and an action's `call`
    /// step alike. Nil for any other name.
    private func runBuiltIn(_ call: FunctionCallInfo) -> FunctionOutcome? {
        if call.name == "openUrl" {
            return FunctionOutcome(decision: openURL(call.args["url"]?.string ?? "") ? .allow : .deny)
        }
        if call.name == "bind" {
            return FunctionOutcome(decision: bind(surfaceId: call.surfaceId, args: call.args) ? .allow : .deny)
        }
        return nil
    }

    private func run(_ call: FunctionCallInfo) async -> FunctionOutcome {
        if let builtIn = runBuiltIn(call) { return builtIn }
        if call.name == Screens.navigateFunction { return navigate(call) }
        guard let fn = functions[call.name] else { return FunctionOutcome(decision: .allow) }
        do {
            return FunctionOutcome(decision: .allow, result: try await fn(call.args, call))
        } catch {
            return FunctionOutcome(decision: .allow, error: error.localizedDescription)
        }
    }

    /// VAPP-194: `actions_<name>` = the surface's package action, all or
    /// nothing: the plan over one snapshot (the args = params, the surface
    /// data, `me`, the clock), every host call's gate and consent, then the
    /// record writes as ONE `records_batch`, then the sets, then the calls.
    /// A failure before the commit lands nothing: EXECUTION_ERROR +
    /// `onActionFailed` (the toast), the outcome's `error`. VAPP-271: a
    /// result call (`call` + `result`) is gated and run before the commit
    /// and the action re-runs over the same snapshot with its answer.
    private func runPackageAction(_ call: FunctionCallInfo, action: String) async -> FunctionOutcome {
        func failed(_ message: String) -> FunctionOutcome {
            send(errorMessageJson(code: "EXECUTION_ERROR", surfaceId: call.surfaceId, message: message, path: nil))
            options.onActionFailed?(ActionFailure(surfaceId: call.surfaceId, action: action, message: message))
            return FunctionOutcome(decision: .allow, error: message)
        }
        guard let model = surfaces[call.surfaceId], let pkgId = router.packageIdOf(surfaceId: call.surfaceId), let pkg = packages[pkgId] else {
            return failed("no action \(action)")
        }
        // VAPP-271: a `call` step with a `result` runs when the plan reaches
        // it (its gate first, before the commit; a refusal or a failure =
        // the action's failure); the action runs again over the SAME
        // snapshot with its answer as `results.<name>`.
        let data = model.data
        let nowMs = options.now.map { $0().timeIntervalSince1970 * 1000 }
        var results: [String: JSONValue] = [:]
        func runAction() -> Actions.Outcome {
            Actions.run(pkg, action: action, params: .object(call.args), data: data, me: options.me, nowMs: nowMs, zone: options.zone, results: results)
        }
        var outcome = runAction()
        while case let .pending(pending) = outcome {
            let c = SurfaceFunctionCall(surfaceId: call.surfaceId, componentId: call.componentId, name: pending.name, args: pending.args)
            let verdict = await gate(c)
            if verdict.decision != .allow { return failed(verdict.error ?? "\(c.name) was not allowed") }
            let answer = await run(c)
            if let error = answer.error { return failed(error) }
            if answer.decision != .allow { return failed("\(c.name) was not allowed") }
            results[pending.result] = answer.result.map { ($0 as? JSONValue) ?? JSONValue(any: $0) } ?? .null
            outcome = runAction()
        }
        let plan: Actions.Plan
        switch outcome {
        case let .error(message): return failed(message)
        case let .plan(p): plan = p
        case .pending: return failed("no action \(action)")
        }
        let calls = plan.calls.map { SurfaceFunctionCall(surfaceId: call.surfaceId, componentId: call.componentId, name: $0.name, args: $0.args) }
        for c in calls {
            let verdict = await gate(c)
            if verdict.decision != .allow { return failed(verdict.error ?? "\(c.name) was not allowed") }
        }
        var committed: Any? = ["ids": [String]()]
        let writes = plan.writes
        if !writes.isEmpty {
            guard let fn = functions[Actions.batchFunction] else { return failed("this host keeps no records") }
            let args: Props = ["writes": .array(writes)]
            do {
                committed = try await fn(args, SurfaceFunctionCall(surfaceId: call.surfaceId, componentId: call.componentId, name: Actions.batchFunction, args: args))
            } catch {
                return failed(error.localizedDescription)
            }
        }
        for s in plan.sets { model.assignData(path: s.path, value: s.value) }
        for c in calls {
            let outcome = await run(c)
            if let error = outcome.error ?? (outcome.decision == .allow ? nil : "\(c.name) was not allowed") {
                return FunctionOutcome(decision: .allow, result: committed, error: error)
            }
        }
        return FunctionOutcome(decision: .allow, result: committed)
    }

    /// VAPP-218: `app.navigate {screen, params?}` → the host's shell, when
    /// the surface's package has that screen. A refusal = INVALID_FUNCTION_CALL.
    @discardableResult
    public func navigate(_ call: FunctionCallInfo) -> FunctionOutcome {
        func refuse(_ message: String) -> FunctionOutcome {
            send(errorMessageJson(code: "INVALID_FUNCTION_CALL", surfaceId: call.surfaceId, message: message, path: nil))
            return FunctionOutcome(decision: .deny, error: message)
        }
        guard let pkgId = router.packageIdOf(surfaceId: call.surfaceId), let pkg = packages[pkgId] else {
            return refuse("navigate: the surface has no package")
        }
        switch Screens.target(pkg, args: call.args) {
        case let .failure(refusal): return refuse(refusal.message)
        case let .success(target):
            guard let open = options.navigate else { return refuse("navigate: this host has no screens") }
            let ok = open(NavigateTarget(surfaceId: call.surfaceId, packageId: pkgId, screen: target.screen, params: target.params))
            return ok ? FunctionOutcome(decision: .allow) : refuse("navigate: \(target.screen) was not opened")
        }
    }

    /// VAPP-122: the `bind` action: re-follow `path` from `source` with
    /// `params` appended (`BindingSource.sourceWithParams`), through the
    /// router like a server's bindDataModel (the surface's package
    /// allowlist applies; a refusal is the usual SOURCE_DENIED /
    /// VALIDATION_FAILED error). True when the bind op ran.
    @discardableResult
    public func bind(surfaceId: String, args: Props) -> Bool {
        let path = args["path"]?.string ?? ""
        let source = args["source"]?.string ?? ""
        guard !source.isEmpty, surfaces[surfaceId] != nil else { return false }
        let params: JSONValue? = args["params"]?.object.map { .object($0) }
        let message: JSONValue = .object([
            "version": .string(Self.a2uiVersion),
            "bindDataModel": .object([
                "surfaceId": .string(surfaceId),
                "path": .string(path),
                "source": .string(BindingSource.sourceWithParams(source, params: params)),
            ]),
        ])
        return receive(message, trusted: true).contains { $0["op"]?.string == "bind" }
    }

    /// The A2UI protocol version the host speaks (`catalog` `A2UI_VERSION`).
    public static let a2uiVersion = ExponentialUICore.a2uiVersion()

    /// The URL policy every href passes (relative urls against the urls'
    /// or the media `baseUrl`).
    public var urlPolicy: UrlPolicy {
        var policy = options.policy.urls ?? UrlPolicy()
        if policy.baseUrl == nil { policy.baseUrl = options.policy.media?.baseUrl }
        return policy
    }

    /// The absolute href a link may navigate to, or nil (paint it as text).
    public func linkHref(_ url: String) -> String? {
        safeHref(url, policy: urlPolicy)
    }

    /// openUrl / Link through the URL policy. True when it opened.
    @discardableResult
    public func openURL(_ url: String) -> Bool {
        guard let abs = linkHref(url), let u = URL(string: abs) else { return false }
        if let open = options.policy.openUrl { open(u) } else { systemOpen(u) }
        return true
    }

    /// The image loader's request: the absolute url + the media rules'
    /// headers; nil when the media policy (schemes, hosts) denies it.
    public func mediaRequest(_ src: String) -> URLRequest? {
        policyMediaRequest(src, options: options.policy.media)
    }

    /// `onPaintError` (`catalog/host.json` `paint`): a component's painter
    /// failed. Sent ONCE per surface + component (whatever the message) as
    /// an A2UI `RENDER_FAILED` error at `/components/<componentId>`, until
    /// an `updateComponents` names that component again.
    public func paintError(_ error: SurfacePaintError) {
        let key = "\(error.surfaceId)\u{0}\(error.componentId)"
        guard !paintErrors.contains(key) else { return }
        paintErrors.insert(key)
        send(errorMessageJson(code: PaintContract.errorCode, surfaceId: error.surfaceId, message: error.message, path: "/components/\(error.componentId)"))
    }

    /// Forget the surface's reported components (`components` = only those).
    private func forgetPaintErrors(_ surfaceId: String, components: [String]? = nil) {
        if let components {
            for c in components { paintErrors.remove("\(surfaceId)\u{0}\(c)") }
            return
        }
        let prefix = "\(surfaceId)\u{0}"
        paintErrors = paintErrors.filter { !$0.hasPrefix(prefix) }
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

    func afterLayout(surfaceId: String) { host?.afterLayout(surfaceId) }

    func onFunctionCall(_ call: SurfaceFunctionCall) {
        guard let host else { return }
        Task { @MainActor in
            _ = await host.callFunction(call)
            self.base?.onFunctionCall(call)
        }
    }

    func openUrl(_ url: String) { host?.openURL(url) }

    func textLinks(_ text: String) -> [TextLinkSegment]? {
        host?.options.policy.textLinks?(text) ?? base?.textLinks(text)
    }

    func imageSize(_ src: String) -> CGSize? {
        host?.options.policy.imageSize?(src) ?? base?.imageSize(src)
    }

    var onImagePress: (@MainActor (String) -> Void)? { host?.options.policy.onImagePress ?? base?.onImagePress }

    var markdownOptions: MarkdownOptions { host?.options.policy.markdownOptions ?? base?.markdownOptions ?? MarkdownOptions() }

    func attachment(_ href: String) -> MarkdownAttachmentInfo? {
        host?.options.policy.attachment?(href) ?? base?.attachment(href)
    }

    var urlPolicy: UrlPolicy? { host?.urlPolicy }

    var mediaOptions: MediaOptions? { host?.options.policy.media }

    func onPaintError(_ error: SurfacePaintError) {
        host?.paintError(error)
        base?.onPaintError(error)
    }

    func resolveUrl(_ src: String) -> String { base?.resolveUrl(src) ?? src }

    func mediaRequest(_ src: String) -> URLRequest? { host?.mediaRequest(resolveUrl(src)) }

    var appFile: AppFileLoader? { host?.options.policy.appFile ?? base?.appFile }

    func onUnknown(component: String, catalogId: String?, id: String) { base?.onUnknown(component: component, catalogId: catalogId, id: id) }

    func fontFamily(_ name: String) -> String? { base?.fontFamily(name) }

    func markdown(_ text: String, width: CGFloat) -> AnyView? { base?.markdown(text, width: width) }

    func onUpload(_ event: SurfaceUploadEvent) { base?.onUpload(event) }

    func pickFiles(_ request: FilePickRequest) -> Bool { base?.pickFiles(request) ?? false }

    func announce(text: String, live: String) { base?.announce(text: text, live: live) }

    func copy(_ text: String) -> Bool { base?.copy(text) ?? false }
}
