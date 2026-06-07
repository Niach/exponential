import AppKit
import ExpCore
import Foundation

/// Locally-stored desktop-agent identity for one workspace (mirrors the Linux
/// `identity_store` agent-{workspaceId}.json). `apiKey` holds the OAuth access
/// token (a valid bearer for every agent call); the refresh fields rotate it.
struct MacAgentIdentity: Codable, Sendable {
    let instanceUrl: String
    let apiKey: String
    var refreshToken: String?
    var tokenEndpoint: String?
    var oauthClientId: String?
    let agentId: String
    let agentUserId: String
    let agentName: String
    let workspaceId: String
    let workspaceSlug: String
    let workspaceName: String
}

enum MacAgentStore {
    static func dir() -> URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Exponential", isDirectory: true)
        try? FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
        return base
    }

    private static func path(_ workspaceId: String) -> URL {
        dir().appendingPathComponent("agent-\(workspaceId).json")
    }

    static func save(_ id: MacAgentIdentity) {
        guard let data = try? JSONEncoder().encode(id) else { return }
        let url = path(id.workspaceId)
        try? data.write(to: url)
        try? FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
    }

    static func load(workspaceId: String) -> MacAgentIdentity? {
        guard let data = try? Data(contentsOf: path(workspaceId)) else { return nil }
        return try? JSONDecoder().decode(MacAgentIdentity.self, from: data)
    }

    static func delete(workspaceId: String) { try? FileManager.default.removeItem(at: path(workspaceId)) }

    static func all() -> [MacAgentIdentity] {
        guard let names = try? FileManager.default.contentsOfDirectory(atPath: dir().path) else { return [] }
        return names.filter { $0.hasPrefix("agent-") && $0.hasSuffix(".json") }.compactMap { name in
            (try? Data(contentsOf: dir().appendingPathComponent(name))).flatMap {
                try? JSONDecoder().decode(MacAgentIdentity.self, from: $0)
            }
        }
    }
}

@MainActor
@Observable
final class MacAgentService {
    private let auth: AuthRepository
    private let integrationsApi: IntegrationsApi
    private let terminalDock: MacTerminalDock
    private(set) var registered: Set<String> = []
    private(set) var online: Set<String> = []
    var busy = false
    var lastError: String?

    private var heartbeats: [String: Task<Void, Never>] = [:]
    private var cores: [String: MacAgentCore] = [:]
    // Workspaces whose core is mid-creation (the GitHub-token fetch is async), so
    // a racing init/register can't create two cores for one workspace.
    private var startingCores: Set<String> = []

    init(auth: AuthRepository, integrationsApi: IntegrationsApi, terminalDock: MacTerminalDock) {
        self.auth = auth
        self.integrationsApi = integrationsApi
        self.terminalDock = terminalDock
        // Interactive agent runs mount into the shared bottom dock; headless runs
        // use the per-run window. The runner is a singleton, so point it here once.
        MacAgentTerminalRunner.shared.dock = terminalDock
        for id in MacAgentStore.all() {
            registered.insert(id.workspaceId)
            startHeartbeat(id)
            startAgent(id)
        }
    }

    // MARK: - Interactive sessions (desktop "AI" / "Approve & continue here")

    /// Can this workspace run an interactive agent session right now? (core
    /// created + registered). Gates the AI / approve-continue / cancel buttons.
    func canRunInteractive(workspaceId: String) -> Bool {
        cores[workspaceId] != nil && registered.contains(workspaceId)
    }

    /// Start an interactive plan session for an issue (the "AI" button). The core
    /// emits a `run_request` with interactive:true that mounts in the dock.
    func requestInteractive(workspaceId: String, issueId: String) {
        cores[workspaceId]?.requestInteractive(issueId: issueId)
    }

    /// Resume an interactive session after the plan was approved (the human
    /// already approved via agentPlan.approvePlan). "Approve & continue here".
    func approveInteractive(workspaceId: String, issueId: String) {
        cores[workspaceId]?.approveInteractive(issueId: issueId)
    }

    /// Cancel the run in flight for an issue (the "Cancel" button).
    func cancelIssue(workspaceId: String, issueId: String) {
        cores[workspaceId]?.cancelIssue(issueId: issueId)
    }

    static var defaultAgentName: String { "\(ProcessInfo.processInfo.hostName)" }

    func isRegistered(_ workspaceId: String) -> Bool { registered.contains(workspaceId) }
    func isOnline(_ workspaceId: String) -> Bool { online.contains(workspaceId) }
    func identity(_ workspaceId: String) -> MacAgentIdentity? { MacAgentStore.load(workspaceId: workspaceId) }

    // MARK: - Register / unregister

    func register(accountId: String, workspaceId: String, name: String) async {
        guard let account = auth.accounts.first(where: { $0.id == accountId }),
              let token = account.token else {
            lastError = "Not signed in"; return
        }
        let base = account.instanceUrl
        busy = true
        lastError = nil
        defer { busy = false }
        do {
            // One human-session-authorized call → the agent sub-identity + a
            // refreshable OAuth credential (no setup token, no public claim).
            let res = try await trpc(base: base, path: "agent.register",
                                     input: ["workspaceId": workspaceId, "name": name], bearer: token)
            guard let res,
                  let cred = res["credential"] as? [String: Any],
                  let accessToken = cred["accessToken"] as? String,
                  let agent = res["agent"] as? [String: Any],
                  let agentId = agent["id"] as? String,
                  let agentUserId = agent["userId"] as? String,
                  let ws = res["workspace"] as? [String: Any],
                  let wsId = ws["id"] as? String else {
                lastError = "Registration failed"; return
            }
            let identity = MacAgentIdentity(
                instanceUrl: base,
                apiKey: accessToken,
                refreshToken: cred["refreshToken"] as? String,
                tokenEndpoint: cred["tokenEndpoint"] as? String,
                oauthClientId: cred["clientId"] as? String,
                agentId: agentId,
                agentUserId: agentUserId,
                agentName: (agent["name"] as? String) ?? name,
                workspaceId: wsId,
                workspaceSlug: (ws["slug"] as? String) ?? "",
                workspaceName: (ws["name"] as? String) ?? ""
            )
            MacAgentStore.save(identity)
            registered.insert(wsId)
            startHeartbeat(identity)
            startAgent(identity)
        } catch {
            lastError = error.localizedDescription
        }
    }

    func unregister(workspaceId: String) async {
        guard let id = MacAgentStore.load(workspaceId: workspaceId) else { return }
        busy = true
        lastError = nil
        defer { busy = false }
        // Best-effort server uninstall; always remove locally (the user wants the
        // agent gone on this Mac even if the token was already revoked server-side).
        do {
            _ = try await trpc(base: id.instanceUrl, path: "agent.uninstallSelf", input: nil, bearer: id.apiKey)
        } catch {
            lastError = "Removed locally; the server uninstall failed: \(error.localizedDescription)"
        }
        forgetLocal(workspaceId: workspaceId)
    }

    /// Local-only teardown for a workspace's agent: stop the heartbeat, shut the
    /// core down, drop the stored identity. Used both by `unregister` (after the
    /// best-effort server uninstall) and after revoking an orphan agent the
    /// server already removed (so this Mac stops pinging under a dead credential).
    func forgetLocal(workspaceId: String) {
        stopHeartbeat(workspaceId)
        cores[workspaceId]?.shutdown()
        cores[workspaceId] = nil
        MacAgentStore.delete(workspaceId: workspaceId)
        registered.remove(workspaceId)
        online.remove(workspaceId)
    }

    // MARK: - Agent loop (Rust agent-core)

    /// Create + start the agent-core for this workspace (watches assigned issues,
    /// emits run_request → MacAgentCore runs the CLI). v1 runs only while the app
    /// is open. No-op if already running or the dylib failed to create the core.
    private func startAgent(_ id: MacAgentIdentity) {
        guard cores[id.workspaceId] == nil, startingCores.insert(id.workspaceId).inserted else { return }
        Task { @MainActor in
            defer { startingCores.remove(id.workspaceId) }
            // agent-core fetches a fresh per-repo GitHub App installation token from
            // the server (agent.repoToken) just before clone/push, so the host
            // no longer feeds a token.
            let dir = MacAgentStore.dir().path
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
                "dbPath": "\(dir)/agent-state-\(id.workspaceId).sqlite",
                "maxConcurrent": 2,
                "timeoutS": 30,
            ]
            guard let data = try? JSONSerialization.data(withJSONObject: config),
                  let json = String(data: data, encoding: .utf8),
                  let core = MacAgentCore(configJson: json) else {
                lastError = "Failed to start the agent core"
                return
            }
            cores[id.workspaceId] = core
        }
    }

    // MARK: - Heartbeat (30s, under the agent's expk_ key)

    private func startHeartbeat(_ id: MacAgentIdentity) {
        heartbeats[id.workspaceId]?.cancel()
        let base = id.instanceUrl, key = id.apiKey, wid = id.workspaceId
        heartbeats[wid] = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                // Self-terminate if the service is gone (it weakly captures self,
                // so the loop would otherwise spin forever as a no-op) — this also
                // means we don't need a deinit, which can't touch main-actor state.
                guard let self else { return }
                do {
                    _ = try await self.trpc(base: base, path: "agent.heartbeat", input: nil, bearer: key)
                    self.online.insert(wid)
                } catch {
                    self.online.remove(wid)
                }
                try? await Task.sleep(for: .seconds(30))
            }
        }
    }

    private func stopHeartbeat(_ workspaceId: String) {
        heartbeats[workspaceId]?.cancel()
        heartbeats[workspaceId] = nil
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
