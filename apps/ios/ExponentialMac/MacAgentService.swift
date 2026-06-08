import AppKit
import ExpCore
import Foundation
import IOKit

/// Locally-stored desktop-device identity for one signed-in account. A device is
/// ACCOUNT-level — the server fans it out as an `agent` member of every workspace
/// the owner belongs to, so a single core watches assigned issues across them
/// all. `apiKey` is a long-lived `expk_` key used as the bearer for the
/// agent-core and the heartbeat (no OAuth refresh).
struct MacDeviceIdentity: Codable, Sendable {
    let instanceUrl: String
    let accountId: String
    let deviceId: String
    let apiKey: String
    let agentId: String
    let agentUserId: String
    let name: String
}

enum MacDeviceStore {
    static func dir() -> URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Exponential", isDirectory: true)
        try? FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
        return base
    }

    private static func path(_ accountId: String) -> URL {
        dir().appendingPathComponent("device-\(accountId).json")
    }

    static func save(_ id: MacDeviceIdentity) {
        guard let data = try? JSONEncoder().encode(id) else { return }
        let url = path(id.accountId)
        try? data.write(to: url)
        try? FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
    }

    static func load(accountId: String) -> MacDeviceIdentity? {
        guard let data = try? Data(contentsOf: path(accountId)) else { return nil }
        return try? JSONDecoder().decode(MacDeviceIdentity.self, from: data)
    }

    static func delete(accountId: String) { try? FileManager.default.removeItem(at: path(accountId)) }

    static func all() -> [MacDeviceIdentity] {
        guard let names = try? FileManager.default.contentsOfDirectory(atPath: dir().path) else { return [] }
        return names.filter { $0.hasPrefix("device-") && $0.hasSuffix(".json") }.compactMap { name in
            (try? Data(contentsOf: dir().appendingPathComponent(name))).flatMap {
                try? JSONDecoder().decode(MacDeviceIdentity.self, from: $0)
            }
        }
    }

    /// Stable per-machine id: the macOS hardware UUID (IOPlatformUUID), which
    /// survives re-launch / reinstall so the server treats this Mac as one
    /// device. Falls back to a persisted random UUID if IOKit is unavailable.
    static func hardwareDeviceId() -> String {
        let service = IOServiceGetMatchingService(
            kIOMainPortDefault, IOServiceMatching("IOPlatformExpertDevice"))
        if service != 0 {
            defer { IOObjectRelease(service) }
            if let cf = IORegistryEntryCreateCFProperty(
                service, "IOPlatformUUID" as CFString, kCFAllocatorDefault, 0
            )?.takeRetainedValue() as? String, !cf.isEmpty {
                return cf
            }
        }
        let fallback = dir().appendingPathComponent("device-id.txt")
        if let s = try? String(contentsOf: fallback, encoding: .utf8),
           !s.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return s.trimmingCharacters(in: .whitespacesAndNewlines)
        }
        let id = UUID().uuidString
        try? id.write(to: fallback, atomically: true, encoding: .utf8)
        return id
    }
}

@MainActor
@Observable
final class MacAgentService {
    private let auth: AuthRepository
    private let integrationsApi: IntegrationsApi
    private let terminalDock: MacTerminalDock
    // Keyed by accountId — a device is one registration per signed-in account.
    private(set) var registered: Set<String> = []
    private(set) var online: Set<String> = []
    var busy = false
    var lastError: String?

    private var heartbeats: [String: Task<Void, Never>] = [:]
    private var cores: [String: MacAgentCore] = [:]
    // Accounts whose core is mid-creation (the setup is async), so a racing
    // init/register can't create two cores for one account.
    private var startingCores: Set<String> = []

    init(auth: AuthRepository, integrationsApi: IntegrationsApi, terminalDock: MacTerminalDock) {
        self.auth = auth
        self.integrationsApi = integrationsApi
        self.terminalDock = terminalDock
        // Interactive agent runs mount into the shared bottom dock; headless runs
        // use the per-run window. The runner is a singleton, so point it here once.
        MacAgentTerminalRunner.shared.dock = terminalDock
        // Registration is kicked off by the auth gate (MacRootView) once an
        // account is actually signed in — at init time, right after a fresh
        // launch, there may be no account yet. See autoRegisterAll().
    }

    // MARK: - Interactive sessions (desktop "AI" / "Approve & continue here")

    /// Can this account run an interactive agent session right now? (a device
    /// core is created + registered). Gates the AI / approve-continue / cancel UI.
    func canRunInteractive(accountId: String) -> Bool {
        cores[accountId] != nil && registered.contains(accountId)
    }

    /// Start an interactive plan session for an issue (the "AI" button). The core
    /// emits a `run_request` with interactive:true that mounts in the dock.
    func requestInteractive(accountId: String, issueId: String) {
        cores[accountId]?.requestInteractive(issueId: issueId)
    }

    /// Resume an interactive session after the plan was approved. "Approve &
    /// continue here".
    func approveInteractive(accountId: String, issueId: String) {
        cores[accountId]?.approveInteractive(issueId: issueId)
    }

    /// Cancel the run in flight for an issue (the "Cancel" button).
    func cancelIssue(accountId: String, issueId: String) {
        cores[accountId]?.cancelIssue(issueId: issueId)
    }

    static var defaultDeviceName: String { ProcessInfo.processInfo.hostName }

    func isRegistered(accountId: String) -> Bool { registered.contains(accountId) }
    func isOnline(accountId: String) -> Bool { online.contains(accountId) }
    func identity(accountId: String) -> MacDeviceIdentity? { MacDeviceStore.load(accountId: accountId) }

    // MARK: - Register (auto, on launch) / unregister

    /// Register this Mac as a device for every signed-in account, then start its
    /// core + heartbeat. Where registration fails (e.g. offline) but a device
    /// identity is already stored, fall back to it so the agent keeps running.
    func autoRegisterAll() async {
        for account in auth.accounts {
            await register(accountId: account.id)
        }
        for id in MacDeviceStore.all() where cores[id.accountId] == nil {
            registered.insert(id.accountId)
            startHeartbeat(id)
            startAgent(id)
        }
    }

    /// One human-session-authorized call registers this device account-wide: the
    /// server mints a fresh expk_ key, fans the device into every workspace the
    /// owner belongs to, and returns the key. Idempotent per machine.
    func register(accountId: String, name: String? = nil) async {
        guard let account = auth.accounts.first(where: { $0.id == accountId }),
              let token = account.token else {
            return // not signed in — nothing to do
        }
        let base = account.instanceUrl
        let deviceId = MacDeviceStore.hardwareDeviceId()
        let deviceName = name ?? Self.defaultDeviceName
        do {
            let res = try await trpc(
                base: base, path: "agent.register",
                input: ["deviceId": deviceId, "name": deviceName], bearer: token)
            guard let res,
                  let apiKey = res["apiKey"] as? String,
                  let agent = res["agent"] as? [String: Any],
                  let agentId = agent["id"] as? String,
                  let agentUserId = agent["userId"] as? String else {
                lastError = "Device registration failed"; return
            }
            let identity = MacDeviceIdentity(
                instanceUrl: base,
                accountId: accountId,
                deviceId: deviceId,
                apiKey: apiKey,
                agentId: agentId,
                agentUserId: agentUserId,
                name: (agent["name"] as? String) ?? deviceName
            )
            MacDeviceStore.save(identity)
            registered.insert(accountId)
            lastError = nil
            // Restart core + heartbeat with the freshly-rotated key.
            stopCore(accountId)
            startHeartbeat(identity)
            startAgent(identity)
        } catch let AgentError.http(code, _) where code == 401 {
            // Stale/expired human session — the device key can't be minted. Tell
            // the user plainly instead of surfacing a raw 401 body.
            lastError = "Your session expired — sign out and back in to register this Mac."
        } catch {
            lastError = error.localizedDescription
        }
    }

    func unregister(accountId: String) async {
        guard let id = MacDeviceStore.load(accountId: accountId) else { return }
        busy = true
        lastError = nil
        defer { busy = false }
        // Best-effort server revoke; always remove locally so this Mac stops
        // pinging under a dead credential even if the key was already revoked.
        do {
            _ = try await trpc(base: id.instanceUrl, path: "agent.uninstallSelf", input: nil, bearer: id.apiKey)
        } catch {
            lastError = "Removed locally; the server uninstall failed: \(error.localizedDescription)"
        }
        forgetLocal(accountId: accountId)
    }

    /// Local-only teardown for an account's device: stop the heartbeat, shut the
    /// core down, drop the stored identity.
    func forgetLocal(accountId: String) {
        stopHeartbeat(accountId)
        stopCore(accountId)
        MacDeviceStore.delete(accountId: accountId)
        registered.remove(accountId)
        online.remove(accountId)
    }

    private func stopCore(_ accountId: String) {
        cores[accountId]?.shutdown()
        cores[accountId] = nil
    }

    // MARK: - Agent loop (Rust agent-core)

    /// Create + start the device's agent-core (watches assigned issues across all
    /// of the owner's workspaces via the account-wide assigned-issues shape,
    /// emits run_request → MacAgentCore runs the CLI). v1 runs only while the app
    /// is open. No-op if already running or the dylib failed to create the core.
    private func startAgent(_ id: MacDeviceIdentity) {
        guard cores[id.accountId] == nil, startingCores.insert(id.accountId).inserted else { return }
        Task { @MainActor in
            defer { startingCores.remove(id.accountId) }
            // agent-core fetches a fresh per-repo GitHub App installation token
            // from the server (agent.repoToken) just before clone/push.
            let dir = MacDeviceStore.dir().path
            for sub in ["repos", "worktrees"] {
                try? FileManager.default.createDirectory(atPath: "\(dir)/\(sub)", withIntermediateDirectories: true)
            }
            let config: [String: Any] = [
                "baseUrl": id.instanceUrl,
                "apiKey": id.apiKey,
                "botUserId": id.agentUserId,
                "githubToken": "",
                "reposRoot": "\(dir)/repos",
                "worktreesRoot": "\(dir)/worktrees",
                "branchPrefix": "agent",
                "driver": "claude",
                "dbPath": "\(dir)/agent-state-\(id.accountId).sqlite",
                "maxConcurrent": 2,
                "timeoutS": 30,
            ]
            guard let data = try? JSONSerialization.data(withJSONObject: config),
                  let json = String(data: data, encoding: .utf8),
                  let core = MacAgentCore(configJson: json) else {
                lastError = "Failed to start the agent core"
                return
            }
            cores[id.accountId] = core
        }
    }

    // MARK: - Heartbeat (30s, under the device's expk_ key)

    private func startHeartbeat(_ id: MacDeviceIdentity) {
        heartbeats[id.accountId]?.cancel()
        let base = id.instanceUrl, key = id.apiKey, aid = id.accountId
        heartbeats[aid] = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                // Self-terminate if the service is gone (it weakly captures self,
                // so the loop would otherwise spin forever as a no-op).
                guard let self else { return }
                do {
                    _ = try await self.trpc(base: base, path: "agent.heartbeat", input: nil, bearer: key)
                    self.online.insert(aid)
                } catch {
                    self.online.remove(aid)
                }
                try? await Task.sleep(for: .seconds(30))
            }
        }
    }

    private func stopHeartbeat(_ accountId: String) {
        heartbeats[accountId]?.cancel()
        heartbeats[accountId] = nil
    }

    // MARK: - Raw tRPC (bare-input POST, custom bearer)

    private enum AgentError: LocalizedError {
        case badUrl, http(Int, String)
        var errorDescription: String? {
            switch self {
            case .badUrl: "Invalid URL"
            case let .http(code, msg): "HTTP \(code): \(msg)"
            }
        }
    }

    @discardableResult
    private func trpc(base: String, path: String, input: [String: Any]?, bearer: String?) async throws -> [String: Any]? {
        guard let url = URL(string: "\(base)/api/trpc/\(path)") else { throw AgentError.badUrl }
        var req = URLRequest(url: url)
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.setValue("application/json", forHTTPHeaderField: "Accept")
        if let bearer { req.setValue("Bearer \(bearer)", forHTTPHeaderField: "Authorization") }
        if let input { req.httpBody = try JSONSerialization.data(withJSONObject: input) }
        let (data, resp) = try await URLSession.shared.data(for: req)
        let code = (resp as? HTTPURLResponse)?.statusCode ?? 0
        guard (200...299).contains(code) else {
            throw AgentError.http(code, String(data: data, encoding: .utf8) ?? "")
        }
        let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        return (obj?["result"] as? [String: Any])?["data"] as? [String: Any]
    }
}
