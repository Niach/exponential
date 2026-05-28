import Foundation

@Observable
final class WorkspaceState {
    var workspaces: [WorkspaceEntity] = []
    var projects: [ProjectEntity] = []
    var activeWorkspaceId: String?

    var activeWorkspace: WorkspaceEntity? {
        workspaces.first { $0.id == activeWorkspaceId } ?? workspaces.first
    }

    var filteredProjects: [ProjectEntity] {
        guard let wsId = activeWorkspace?.id else { return [] }
        return projects
            .filter { $0.workspaceId == wsId && $0.archivedAt == nil }
            .sorted { ($0.sortOrder ?? 0) < ($1.sortOrder ?? 0) }
    }
}
