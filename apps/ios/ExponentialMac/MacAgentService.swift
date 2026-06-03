import AppKit
import ExpCore
import Foundation

/// Locally-stored desktop-agent identity for one workspace (mirrors the Linux
/// `identity_store` agent-{workspaceId}.json). Holds the agent's `expk_` key.
struct MacAgentIdentity: Codable, Sendable {
    let instanceUrl: String
    let apiKey: String
    let agentId: String
    let agentUserId: String
    let agentName: String
    let workspaceId: String
    let workspaceSlug: String
    let workspaceName: String
    var githubClientId: String?
    var githubLogin: String?
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

    /// Machine-global GitHub token (mirrors Linux github.json), consumed by the
    /// Rust agent-core config once it's linked (A5 loop).
    static func saveGithubToken(_ token: String, login: String) {
        let json: [String: Any] = ["token": token, "login": login]
        guard let data = try? JSONSerialization.data(withJSONObject: json) else { return }
        let url = dir().appendingPathComponent("github.json")
        try? data.write(to: url)
        try? FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
    }

    static func githubToken() -> String? {
        let url = dir().appendingPathComponent("github.json")
        guard let data = try? Data(contentsOf: url),
              let o = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return nil }
        return o["token"] as? String
    }
}

@MainActor
@Observable
final class MacAgentService {
    private let auth: AuthRepository
    private(set) var registered: Set<String> = []
    private(set) var online: Set<String> = []
    var busy = false
    var lastError: String?
    var githubPrompt: GithubPrompt?

    struct GithubPrompt: Equatable { let userCode: String; let uri: String }

    private var heartbeats: [String: Task<Void, Never>] = [:]
    private var cores: [String: MacAgentCore] = [:]

    init(auth: AuthRepository) {
        self.auth = auth
        for id in MacAgentStore.all() {
            registered.insert(id.workspaceId)
            startHeartbeat(id)
            startAgent(id)
        }
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
            // 1. companion.create (human session) → setupToken
            let created = try await trpc(base: base, path: "companion.create",
                                         input: ["workspaceId": workspaceId, "name": name], bearer: token)
            guard let setupToken = created?["setupToken"] as? String else {
                lastError = "Server did not return a setup token"; return
            }
            // 2. companion.claimSetup (public) → agent credentials
            let claim = try await trpc(base: base, path: "companion.claimSetup",
                                       input: ["setupToken": setupToken], bearer: nil)
            guard let claim,
                  let apiKey = claim["apiKey"] as? String,
                  let agent = claim["agent"] as? [String: Any],
                  let agentId = agent["id"] as? String,
                  let agentUserId = agent["userId"] as? String,
                  let ws = claim["workspace"] as? [String: Any],
                  let wsId = ws["id"] as? String else {
                lastError = "Claim setup failed"; return
            }
            let identity = MacAgentIdentity(
                instanceUrl: base,
                apiKey: apiKey,
                agentId: agentId,
                agentUserId: agentUserId,
                agentName: (agent["name"] as? String) ?? name,
                workspaceId: wsId,
                workspaceSlug: (ws["slug"] as? String) ?? "",
                workspaceName: (ws["name"] as? String) ?? "",
                githubClientId: (claim["oauth"] as? [String: Any])?["githubClientId"] as? String,
                githubLogin: nil
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
        defer { busy = false }
        _ = try? await trpc(base: id.instanceUrl, path: "companion.uninstallSelf", input: nil, bearer: id.apiKey)
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
        guard cores[id.workspaceId] == nil else { return }
        let dir = MacAgentStore.dir().path
        for sub in ["repos", "worktrees"] {
            try? FileManager.default.createDirectory(atPath: "\(dir)/\(sub)", withIntermediateDirectories: true)
        }
        let config: [String: Any] = [
            "baseUrl": id.instanceUrl,
            "apiKey": id.apiKey,
            "botUserId": id.agentUserId,
            "githubToken": MacAgentStore.githubToken() ?? "",
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

    // MARK: - Heartbeat (30s, under the agent's expk_ key)

    private func startHeartbeat(_ id: MacAgentIdentity) {
        heartbeats[id.workspaceId]?.cancel()
        let base = id.instanceUrl, key = id.apiKey, wid = id.workspaceId
        heartbeats[wid] = Task { [weak self] in
            while !Task.isCancelled {
                do {
                    _ = try await self?.trpc(base: base, path: "companion.heartbeat", input: nil, bearer: key)
                    self?.online.insert(wid)
                } catch {
                    self?.online.remove(wid)
                }
                try? await Task.sleep(for: .seconds(30))
            }
        }
    }

    private func stopHeartbeat(_ workspaceId: String) {
        heartbeats[workspaceId]?.cancel()
        heartbeats[workspaceId] = nil
    }

    // MARK: - GitHub device flow

    func connectGitHub(workspaceId: String) async {
        guard var id = MacAgentStore.load(workspaceId: workspaceId), let clientId = id.githubClientId else {
            lastError = "This workspace has no GitHub client configured"; return
        }
        busy = true
        lastError = nil
        defer { busy = false; githubPrompt = nil }
        do {
            let dc = try await githubRequestDeviceCode(clientId: clientId)
            githubPrompt = GithubPrompt(userCode: dc.userCode, uri: dc.verificationUri)
            if let url = URL(string: dc.verificationUri) { NSWorkspace.shared.open(url) }
            let token = try await githubPoll(clientId: clientId, deviceCode: dc.deviceCode,
                                             interval: dc.interval, expiresIn: dc.expiresIn)
            let login = (try? await githubFetchLogin(token: token)) ?? ""
            MacAgentStore.saveGithubToken(token, login: login)
            _ = try? await trpc(base: id.instanceUrl, path: "companion.reportGithubIdentity",
                                input: ["login": login, "repos": []], bearer: id.apiKey)
            id.githubLogin = login
            MacAgentStore.save(id)
        } catch {
            lastError = error.localizedDescription
        }
    }

    private struct DeviceCode { let deviceCode: String; let userCode: String; let verificationUri: String; let interval: Int; let expiresIn: Int }

    private func githubRequestDeviceCode(clientId: String) async throws -> DeviceCode {
        var req = URLRequest(url: URL(string: "https://github.com/login/device/code")!)
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Accept")
        req.setValue("application/x-www-form-urlencoded", forHTTPHeaderField: "Content-Type")
        req.httpBody = "client_id=\(clientId)&scope=repo%20read:user".data(using: .utf8)
        let (data, _) = try await URLSession.shared.data(for: req)
        guard let o = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let device = o["device_code"] as? String else { throw AgentError.github("Bad device-code response") }
        return DeviceCode(
            deviceCode: device,
            userCode: (o["user_code"] as? String) ?? "",
            verificationUri: (o["verification_uri"] as? String) ?? "https://github.com/login/device",
            interval: (o["interval"] as? Int) ?? 5,
            expiresIn: (o["expires_in"] as? Int) ?? 900
        )
    }

    private func githubPoll(clientId: String, deviceCode: String, interval: Int, expiresIn: Int) async throws -> String {
        var wait = interval
        let deadline = Date().addingTimeInterval(TimeInterval(expiresIn))
        let body = "client_id=\(clientId)&device_code=\(deviceCode)&grant_type=urn:ietf:params:oauth:grant-type:device_code"
        while Date() < deadline {
            try? await Task.sleep(for: .seconds(Double(wait)))
            var req = URLRequest(url: URL(string: "https://github.com/login/oauth/access_token")!)
            req.httpMethod = "POST"
            req.setValue("application/json", forHTTPHeaderField: "Accept")
            req.setValue("application/x-www-form-urlencoded", forHTTPHeaderField: "Content-Type")
            req.httpBody = body.data(using: .utf8)
            guard let (data, _) = try? await URLSession.shared.data(for: req),
                  let o = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { continue }
            if let token = o["access_token"] as? String { return token }
            switch o["error"] as? String {
            case "slow_down": wait += 5
            case "expired_token": throw AgentError.github("Device code expired")
            case "access_denied": throw AgentError.github("Authorization denied")
            default: break
            }
        }
        throw AgentError.github("Timed out waiting for GitHub authorization")
    }

    private func githubFetchLogin(token: String) async throws -> String {
        var req = URLRequest(url: URL(string: "https://api.github.com/user")!)
        req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        req.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        let (data, _) = try await URLSession.shared.data(for: req)
        return ((try? JSONSerialization.jsonObject(with: data) as? [String: Any])?["login"] as? String) ?? ""
    }

    // MARK: - Raw tRPC (bare-input POST, custom bearer)

    private enum AgentError: LocalizedError {
        case badUrl, http(Int, String), github(String)
        var errorDescription: String? {
            switch self {
            case .badUrl: "Invalid URL"
            case let .http(code, msg): "HTTP \(code): \(msg)"
            case let .github(m): m
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
