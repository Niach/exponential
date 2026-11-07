// #VAPP-268: the runner (runner.json `$comment`, docs/spec/apps.md §13.3)
// over ONE Engine `AppInstance`: the bundle loads (or its failure paints in
// place of the app), the PAINTER installs `runnerPackage(pkg, formFactor)`,
// the scene's events drive lifecycle.json, every function call of the app
// goes to the instance (an `ask` paints `consent`, a refusal paints
// `toast`), and the runner's own events (`runner.navigate`,
// `runner.permissions`, `consent.toggle`, `consent.answer`) are handled
// here. No chrome: everything painted is a genUI surface the Engine built.
//
// THREADING: every Engine call runs on ONE serial worker queue (a
// capability blocks its caller while its picker is on screen, and refuses
// the main thread); the painter is only ever called through `deliver`
// (default: the main queue).
import Foundation
import ExponentialEngine

/// The surfaces a runner paints (runner.json `SURFACES`).
public enum RunnerSurface {
    public static let app = "app"
    public static let consent = "consent"
    public static let permissions = "permissions"
    public static let toast = "toast"
}

/// lifecycle.json `copy`: the toast of a link or tap that cannot open.
public enum RunnerCopy {
    public static let refusedLink = "Couldn't open the link"
}

/// What the platform's genUI host does for the runner (on the main queue).
public protocol RunnerPainter: AnyObject {
    /// Install the package the painter shows (the shell-wrapped one).
    func install(packageJson: String)
    /// Apply template `screen` to surface `app` with data `{params}`.
    func open(screen: String, paramsJson: String)
    /// Paint `surface` with these components (+ data), replacing what it showed.
    func paint(surface: String, componentsJson: String, dataJson: String?)
    func close(surface: String)
    /// The records changed: re-resolve every bound `records:` source.
    func recordsChanged()
}

/// The platform's scene events (lifecycle.json `EVENTS`).
public enum RunnerSceneEvent: String, Sendable {
    case foreground, background, terminate
}

/// An app's function call, answered.
public enum RunnerCallOutcome: Equatable, Sendable {
    /// The result JSON.
    case value(String)
    /// Refused (the toast is painted already).
    case refused(String)
}

public final class RunnerSession: @unchecked Sendable {
    /// What the Engine is told about this host (providers.json `host`).
    public struct Host: Sendable {
        public var platform: String
        public var formFactor: String
        public var locale: String
        public var zone: String?
        public init(platform: String = "ios", formFactor: String = "phone", locale: String = Locale.current.identifier, zone: String? = TimeZone.current.identifier) {
            self.platform = platform; self.formFactor = formFactor; self.locale = locale; self.zone = zone
        }
    }

    public let bundle: RunnerBundle?
    public private(set) var failure: RunnerFailureKind?
    public private(set) var instance: AppInstance?
    public weak var painter: RunnerPainter?

    let queue = DispatchQueue(label: "at.exponential.runner.engine")
    let deliver: (@escaping () -> Void) -> Void
    let dataRoot: URL?
    let vault: Vault
    let capabilities: [Capability]
    let host: Host
    var engine: Engine?
    /// VAPP-271: the app's files (the store the Engine writes) + its app id,
    /// read off the queue (a picker may block it) under `lock`.
    private var files: (app: String, blobs: RunnerBlobs)?
    var blobs: RunnerBlobs? { lock.withLock { files?.blobs } }
    var formFactor: RunnerFormFactor
    /// The open screen (template id) + its params JSON (queue-owned).
    var screen: (id: String, params: String)?
    var presented = 0
    private let lock = NSLock()
    private var pendingConsent: CheckedContinuation<ConsentAnswer, Never>?

    /// `dataRoot` = the platform app-data directory (the app's stores live in
    /// `<dataRoot>/<config id>`; nil = in memory); `vault` = the Keychain.
    public init(bundle: Result<RunnerBundle, RunnerFailureKind>, dataRoot: URL?, vault: Vault, capabilities: [Capability],
                host: Host = Host(), formFactor: RunnerFormFactor, painter: RunnerPainter? = nil,
                deliver: @escaping (@escaping () -> Void) -> Void = { work in DispatchQueue.main.async(execute: work) }) {
        switch bundle {
        case let .success(b): self.bundle = b
        case let .failure(kind): self.bundle = nil; failure = kind
        }
        self.dataRoot = dataRoot
        self.vault = vault
        self.capabilities = capabilities
        self.host = host
        self.formFactor = formFactor
        self.painter = painter
        self.deliver = deliver
    }

    /// Waits until every Engine call queued so far ran (tests).
    public func settle() { queue.sync {} }

    // MARK: launch

    /// Loads the bundle into the instance and launches it; on any failure
    /// paints `runnerFailure(kind)` in surface `app`.
    public func start() {
        queue.async { [self] in
            guard failure == nil, let bundle else { return paintFailure(failure ?? .missing) }
            do {
                let dataDir = dataRoot.map { $0.appendingPathComponent(bundle.config.id, isDirectory: true) }
                if let dataDir { try? FileManager.default.createDirectory(at: dataDir, withIntermediateDirectories: true) }
                let blobs = RunnerBlobs(root: dataDir?.appendingPathComponent("blobs", isDirectory: true))
                let engine = try Engine(
                    host: EngineHost(kind: "exported", platform: host.platform, formFactor: host.formFactor, locale: host.locale,
                                     zone: host.zone, network: bundle.config.networkPolicy, dataDir: dataDir?.path),
                    identity: .local(deviceId: RunnerDevice.id(vault)),
                    providers: EngineProviders(vault: vault, window: RunnerWindow(self), blobs: blobs, capabilities: capabilities))
                let instance = try engine.load(fileJson: bundle.packageJson)
                self.engine = engine
                let app = instance.appId()
                lock.withLock { files = (app, blobs) }
                self.instance = instance
            } catch let EngineError.Load(kind, _) {
                return paintFailure(RunnerFailureKind(rawValue: kind) ?? .invalid)
            } catch {
                return paintFailure(.invalid)
            }
            guard let derived = derivedPackage() else { return paintFailure(.invalid) }
            deliver { [weak self] in self?.painter?.install(packageJson: derived) }
            let before = presented
            lifecycle(#"{"type":"launch"}"#)
            // A plain launch that opens nothing: the derived package's first screen.
            if presented == before, let first = Self.firstScreen(derived) { presented(screen: first, paramsJson: "{}") }
        }
    }

    func paintFailure(_ kind: RunnerFailureKind) {
        failure = kind
        let components = (try? runnerFailure(kind: kind.rawValue)) ?? "[]"
        deliver { [weak self] in self?.painter?.paint(surface: RunnerSurface.app, componentsJson: components, dataJson: nil) }
    }

    /// The shell-wrapped package for the current form factor (queue only).
    func derivedPackage() -> String? {
        guard let instance else { return nil }
        return try? runnerPackage(packageJson: instance.packageJson(), formFactor: formFactor)
    }

    static func firstScreen(_ packageJson: String) -> String? {
        ((RunnerJSON.object(packageJson)?["screens"] as? [[String: Any]])?.first?["template"]) as? String
    }

    /// The WindowHost's `present` (on the queue, inside an Engine call).
    func presented(screen id: String, paramsJson: String) {
        presented += 1
        screen = (id, paramsJson)
        deliver { [weak self] in self?.painter?.open(screen: id, paramsJson: paramsJson) }
    }

    /// One lifecycle.json event; a `refused` effect paints the toast (queue only).
    func lifecycle(_ eventJson: String) {
        guard let instance, let effects = try? instance.event(eventJson: eventJson) else { return }
        for effect in RunnerJSON.value(effects) as? [[String: Any]] ?? [] where effect["effect"] as? String == "refused" {
            toast(RunnerCopy.refusedLink)
        }
    }

    // MARK: the platform's events

    public func scene(_ event: RunnerSceneEvent) {
        queue.async { [self] in lifecycle(RunnerJSON.text(["type": event.rawValue])) }
    }

    /// A deep link `<config id>://<screen>?<params>`; false = not this app's scheme.
    @discardableResult
    public func open(url: URL) -> Bool {
        guard let config = bundle?.config, let event = RunnerLinks.event(url, id: config.id) else { return false }
        queue.async { [self] in lifecycle(event) }
        return true
    }

    /// A notification tap's JSON (`NotificationTaps.tapJson`), verbatim.
    public func notificationTap(_ tapJson: String) {
        queue.async { [self] in lifecycle(tapJson) }
    }

    /// The window crossed genUI's `md` breakpoint: re-install and re-apply.
    public func setFormFactor(_ next: RunnerFormFactor) {
        queue.async { [self] in
            guard next != formFactor else { return }
            formFactor = next
            guard let derived = derivedPackage() else { return }
            let open = screen
            deliver { [weak self] in
                self?.painter?.install(packageJson: derived)
                if let open { self?.painter?.open(screen: open.id, paramsJson: open.params) }
            }
        }
    }

    // MARK: the runner's own events

    /// `runner.navigate`, `runner.permissions`, `consent.toggle`,
    /// `consent.answer`: true when the runner handled `name`.
    @discardableResult
    public func handleEvent(name: String, context: [String: Any]) -> Bool {
        switch name {
        case "runner.navigate":
            guard let screen = context["screen"] as? String else { return true }
            queue.async { [self] in
                guard let instance else { return }
                let result = Result { try instance.call(name: "app_navigate", argsJson: RunnerJSON.text(["screen": screen]), newIds: []) }
                if case let .success(.refused(_, message)) = result { toast(message) }
            }
        case "runner.permissions":
            queue.async { [self] in paintPermissions() }
        case "consent.toggle":
            guard let scope = context["scope"] as? String else { return true }
            queue.async { [self] in
                guard let instance else { return }
                let granted = (RunnerJSON.object(instance.grantsJson())?[scope] as? String) == "granted"
                do { try instance.setGrant(scope: scope, value: granted ? nil : .granted) } catch { toast(error.localizedDescription) }
                paintPermissions()
            }
        case "consent.answer":
            let answer: ConsentAnswer
            switch context["answer"] as? String {
            case "allow": answer = .allow
            case "once": answer = .once
            case "deny": answer = .deny
            default: answer = .dismiss
            }
            answerConsent(answer)
        default:
            return false
        }
        return true
    }

    func paintPermissions() {
        guard let instance else { return }
        let components = instance.permissionsSheetJson()
        deliver { [weak self] in self?.painter?.paint(surface: RunnerSurface.permissions, componentsJson: components, dataJson: nil) }
    }

    func toast(_ message: String) {
        let components = runnerToast(message: message)
        deliver { [weak self] in self?.painter?.paint(surface: RunnerSurface.toast, componentsJson: components, dataJson: nil) }
    }

    func answerConsent(_ answer: ConsentAnswer) {
        let pending: CheckedContinuation<ConsentAnswer, Never>? = lock.withLock {
            defer { pendingConsent = nil }
            return pendingConsent
        }
        pending?.resume(returning: answer)
    }

    // MARK: the app's function calls

    /// Every function call of the app (`records.*`, `app.navigate`, the
    /// capabilities): the instance answers; an `ask` paints `consent` and
    /// waits for `consent.answer`; a refusal paints `toast`.
    public func call(name: String, argsJson: String) async -> RunnerCallOutcome {
        let ids = Self.newIds(name: name, argsJson: argsJson)
        var result = await engine { try $0.call(name: name, argsJson: argsJson, newIds: ids) }
        if case let .success(.ask(verdict, sheet)) = result {
            deliver { [weak self] in self?.painter?.paint(surface: RunnerSurface.consent, componentsJson: sheet.componentsJson, dataJson: sheet.dataJson) }
            let answer = await withCheckedContinuation { (c: CheckedContinuation<ConsentAnswer, Never>) in
                let earlier: CheckedContinuation<ConsentAnswer, Never>? = lock.withLock {
                    defer { pendingConsent = c }
                    return pendingConsent
                }
                earlier?.resume(returning: .dismiss)
            }
            deliver { [weak self] in self?.painter?.close(surface: RunnerSurface.consent) }
            result = await engine { try $0.answerConsent(name: name, argsJson: argsJson, verdictJson: verdict, answer: answer) }
        }
        switch result {
        case let .success(.value(json)):
            if name.hasPrefix("records.") { deliver { [weak self] in self?.painter?.recordsChanged() } }
            return .value(json)
        case let .success(.refused(_, message)):
            toast(message)
            return .refused(message)
        case .success(.ask):
            return .refused("dismiss")
        case let .failure(error):
            let message = Self.message(error)
            toast(message)
            return .refused(message)
        }
    }

    /// A `records:` source's value (JSON text; nil = not a records source).
    public func recordsValue(source: String, _ done: @escaping (String?) -> Void) {
        queue.async { [self] in
            let value = instance?.recordsValue(source: source)
            deliver { done(value) }
        }
    }

    /// VAPP-271: an `app-file:<fileId>` src's bytes (catalog/host.json
    /// media.appFile) from the running app's files; nil = missing or not
    /// running. genUI's `appFile` hook.
    public func appFile(_ fileId: String) async -> Data? {
        guard let files = lock.withLock({ files }) else { return nil }
        return await withCheckedContinuation { c in
            DispatchQueue.global(qos: .userInitiated).async {
                c.resume(returning: (try? files.blobs.get(app: files.app, id: fileId)) ?? nil)
            }
        }
    }

    func engine<T>(_ body: @escaping (AppInstance) throws -> T) async -> Result<T, Error> {
        await withCheckedContinuation { c in
            queue.async { [self] in
                guard let instance else { return c.resume(returning: .failure(EngineError.Invalid(reason: "the app is not running"))) }
                c.resume(returning: Result { try body(instance) })
            }
        }
    }

    static func message(_ error: Error) -> String {
        switch error {
        case let EngineError.Invalid(reason): return reason
        case let EngineError.Failure(_, reason): return reason
        case let EngineError.Load(kind, _): return kind
        default: return error.localizedDescription
        }
    }

    /// The uuids of the rows a call may create: one for an upsert, one per
    /// write of a batch.
    static func newIds(name: String, argsJson: String) -> [String] {
        let count: Int
        switch name {
        case "records_upsert": count = 1
        case "records_batch": count = (RunnerJSON.object(argsJson)?["writes"] as? [Any])?.count ?? 0
        default: count = 0
        }
        return (0..<count).map { _ in UUID().uuidString.lowercased() }
    }
}

/// The Engine's `WindowHost`: `present` opens the screen in surface `app`.
final class RunnerWindow: WindowHost, @unchecked Sendable {
    weak var session: RunnerSession?
    init(_ session: RunnerSession) { self.session = session }

    func present(app: String, screen: String, paramsJson: String, how: Presentation) {
        session?.presented(screen: screen, paramsJson: paramsJson)
    }
}
