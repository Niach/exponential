import Foundation

@Observable
public final class WorkspaceState {
    public var workspaces: [WorkspaceEntity] = []
    public var projects: [ProjectEntity] = []
    public var activeWorkspaceId: String?

    public init() {}

    public var activeWorkspace: WorkspaceEntity? {
        workspaces.first { $0.id == activeWorkspaceId } ?? workspaces.first
    }

    public var filteredProjects: [ProjectEntity] {
        guard let wsId = activeWorkspace?.id else { return [] }
        return projects
            .filter { $0.workspaceId == wsId && $0.archivedAt == nil }
            .sorted { ($0.sortOrder ?? 0) < ($1.sortOrder ?? 0) }
    }
}
