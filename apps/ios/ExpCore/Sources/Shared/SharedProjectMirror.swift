import Foundation

/// A project entry mirrored from the app's local DB into the app-group
/// container so the Share Extension can populate its project picker without
/// opening the (per-account, non-shared) GRDB database.
public struct MirroredProject: Codable, Sendable, Identifiable {
    public let accountId: String
    public let accountName: String
    public let workspaceId: String
    public let workspaceName: String
    public let projectId: String
    public let projectName: String
    public let prefix: String

    public var id: String { accountId + ":" + projectId }

    public init(
        accountId: String,
        accountName: String,
        workspaceId: String,
        workspaceName: String,
        projectId: String,
        projectName: String,
        prefix: String
    ) {
        self.accountId = accountId
        self.accountName = accountName
        self.workspaceId = workspaceId
        self.workspaceName = workspaceName
        self.projectId = projectId
        self.projectName = projectName
        self.prefix = prefix
    }
}

/// The most recently opened/created project, used as the picker's default.
public struct LastUsedProject: Codable, Sendable {
    public let accountId: String
    public let projectId: String

    public init(accountId: String, projectId: String) {
        self.accountId = accountId
        self.projectId = projectId
    }
}

/// Reads/writes the project picker mirror in the shared app-group defaults. The
/// app writes (whenever projects sync / a project is opened); the extension
/// reads. All keys live under [SharedAppGroup.suiteName].
public enum SharedProjectMirror {
    private static let projectsKey = "picker_projects_v1"
    private static let lastUsedKey = "picker_last_used_project_v1"

    // MARK: - App writes

    public static func write(projects: [MirroredProject]) {
        guard let defaults = SharedAppGroup.defaults,
              let data = try? JSONEncoder().encode(projects)
        else { return }
        defaults.set(data, forKey: projectsKey)
    }

    public static func writeLastUsed(accountId: String, projectId: String) {
        guard !accountId.isEmpty, !projectId.isEmpty,
              let defaults = SharedAppGroup.defaults,
              let data = try? JSONEncoder().encode(LastUsedProject(accountId: accountId, projectId: projectId))
        else { return }
        defaults.set(data, forKey: lastUsedKey)
    }

    // MARK: - Extension reads

    public static func readProjects() -> [MirroredProject] {
        guard let defaults = SharedAppGroup.defaults,
              let data = defaults.data(forKey: projectsKey),
              let list = try? JSONDecoder().decode([MirroredProject].self, from: data)
        else { return [] }
        return list
    }

    public static func readLastUsed() -> LastUsedProject? {
        guard let defaults = SharedAppGroup.defaults,
              let data = defaults.data(forKey: lastUsedKey),
              let value = try? JSONDecoder().decode(LastUsedProject.self, from: data)
        else { return nil }
        return value
    }
}
