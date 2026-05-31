import Foundation

/// A project entry mirrored from the app's local DB into the app-group
/// container so the Share Extension can populate its project picker without
/// opening the (per-account, non-shared) GRDB database.
struct MirroredProject: Codable, Sendable, Identifiable {
    let accountId: String
    let accountName: String
    let workspaceId: String
    let workspaceName: String
    let projectId: String
    let projectName: String
    let prefix: String

    var id: String { accountId + ":" + projectId }
}

/// The most recently opened/created project, used as the picker's default.
struct LastUsedProject: Codable, Sendable {
    let accountId: String
    let projectId: String
}

/// Reads/writes the project picker mirror in the shared app-group defaults. The
/// app writes (whenever projects sync / a project is opened); the extension
/// reads. All keys live under [SharedAppGroup.suiteName].
enum SharedProjectMirror {
    private static let projectsKey = "picker_projects_v1"
    private static let lastUsedKey = "picker_last_used_project_v1"

    // MARK: - App writes

    static func write(projects: [MirroredProject]) {
        guard let defaults = SharedAppGroup.defaults,
              let data = try? JSONEncoder().encode(projects)
        else { return }
        defaults.set(data, forKey: projectsKey)
    }

    static func writeLastUsed(accountId: String, projectId: String) {
        guard !accountId.isEmpty, !projectId.isEmpty,
              let defaults = SharedAppGroup.defaults,
              let data = try? JSONEncoder().encode(LastUsedProject(accountId: accountId, projectId: projectId))
        else { return }
        defaults.set(data, forKey: lastUsedKey)
    }

    // MARK: - Extension reads

    static func readProjects() -> [MirroredProject] {
        guard let defaults = SharedAppGroup.defaults,
              let data = defaults.data(forKey: projectsKey),
              let list = try? JSONDecoder().decode([MirroredProject].self, from: data)
        else { return [] }
        return list
    }

    static func readLastUsed() -> LastUsedProject? {
        guard let defaults = SharedAppGroup.defaults,
              let data = defaults.data(forKey: lastUsedKey),
              let value = try? JSONDecoder().decode(LastUsedProject.self, from: data)
        else { return nil }
        return value
    }
}
