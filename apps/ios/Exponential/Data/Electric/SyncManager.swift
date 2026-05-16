import Foundation
import GRDB
import os

private let logger = Logger(subsystem: "com.straehhuber.exponential", category: "SyncManager")

final class SyncManager: @unchecked Sendable {
    private let auth: AuthRepository
    let db: DatabaseManager
    private var shapeTasks: [Task<Void, Never>] = []
    private var observationTask: Task<Void, Never>?

    init(auth: AuthRepository, db: DatabaseManager) {
        self.auth = auth
        self.db = db
    }

    func start() {
        observationTask = Task { [weak self] in
            guard let self else { return }
            var previousUrl: String? = self.auth.instanceUrl
            var previousToken: String? = self.auth.token

            if previousUrl != nil && previousToken != nil {
                self.launchShapes()
            }

            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(500))
                let currentUrl = self.auth.instanceUrl
                let currentToken = self.auth.token

                if currentUrl != previousUrl || currentToken != previousToken {
                    logger.info("Auth state changed")
                    previousUrl = currentUrl
                    previousToken = currentToken
                    self.cancelShapes()
                    if currentUrl != nil && currentToken != nil {
                        self.launchShapes()
                    }
                }
            }
        }
    }

    func stop() {
        observationTask?.cancel()
        observationTask = nil
        cancelShapes()
    }

    func signOut() async {
        cancelShapes()
        do {
            try db.clearAllData()
        } catch {
            logger.error("Failed to clear data: \(error.localizedDescription)")
        }
    }

    /// One-shot fetch of all shapes — called from UI as a workaround
    func initialSync() async {
        guard let baseUrl = auth.instanceUrl, let token = auth.token else { return }
        logger.info("Starting initial sync")

        async let ws = fetchShape(baseUrl: baseUrl, token: token, path: "/api/shapes/workspaces", type: WorkspaceEntity.self)
        async let proj = fetchShape(baseUrl: baseUrl, token: token, path: "/api/shapes/projects", type: ProjectEntity.self)
        async let iss = fetchShape(baseUrl: baseUrl, token: token, path: "/api/shapes/issues", type: IssueEntity.self)
        async let lab = fetchShape(baseUrl: baseUrl, token: token, path: "/api/shapes/labels", type: LabelEntity.self)
        async let il = fetchShape(baseUrl: baseUrl, token: token, path: "/api/shapes/issue-labels", type: IssueLabelEntity.self)
        async let usr = fetchShape(baseUrl: baseUrl, token: token, path: "/api/shapes/users", type: UserEntity.self)
        async let wm = fetchShape(baseUrl: baseUrl, token: token, path: "/api/shapes/workspace-members", type: WorkspaceMemberEntity.self)
        async let wi = fetchShape(baseUrl: baseUrl, token: token, path: "/api/shapes/workspace-invites", type: WorkspaceInviteEntity.self)

        let results = await [ws, proj, iss, lab, il, usr, wm, wi]
        let names = ["ws", "proj", "issues", "labels", "il", "users", "wm", "wi"]
        for (name, count) in zip(names, results) {
            logger.info("  \(name): \(count)")
        }
    }

    private func fetchShape<T: Codable & FetchableRecord & PersistableRecord & Sendable>(
        baseUrl: String, token: String, path: String, type: T.Type
    ) async -> Int {
        guard let url = URL(string: "\(baseUrl)\(path)?offset=-1") else { return -1 }

        var request = URLRequest(url: url)
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Accept")

        do {
            let (data, response) = try await URLSession.shared.data(for: request)
            guard let http = response as? HTTPURLResponse, (200...299).contains(http.statusCode) else { return -2 }
            guard let array = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]] else { return -3 }

            var saved = 0
            for dict in array {
                guard let value = dict["value"] as? [String: Any] else { continue }
                let coerced = Self.coerceStringValues(value)
                guard let jsonData = try? JSONSerialization.data(withJSONObject: coerced) else { continue }

                if let entity = try? JSONDecoder().decode(T.self, from: jsonData) {
                    try await db.dbPool.write { db in try entity.save(db) }
                    saved += 1
                }
            }
            return saved
        } catch {
            logger.error("fetchShape \(path): \(error.localizedDescription)")
            return -4
        }
    }

    private static func coerceStringValues(_ dict: [String: Any]) -> [String: Any] {
        var result = [String: Any]()
        for (key, value) in dict {
            if let str = value as? String {
                if str == "true" { result[key] = true }
                else if str == "false" { result[key] = false }
                else if str.contains("."), let d = Double(str) { result[key] = d }
                else if let i = Int(str) { result[key] = i }
                else { result[key] = str }
            } else {
                result[key] = value
            }
        }
        return result
    }

    // MARK: - Live shapes (for real-time updates after initial sync)

    private func cancelShapes() {
        shapeTasks.forEach { $0.cancel() }
        shapeTasks.removeAll()
    }

    private func launchShapes() {
        logger.info("Launching live shape sync")
        // For now, live sync is handled by initialSync() + polling
        // TODO: re-enable ShapeClient-based live sync once GRDB async write issue is resolved
    }
}
