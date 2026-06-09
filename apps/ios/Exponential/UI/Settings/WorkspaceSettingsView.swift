import ExpUI
import ExpCore
import SwiftUI
import GRDB

struct WorkspaceSettingsView: View {
    let workspaceId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss
    @State private var workspace: WorkspaceEntity?
    @State private var members: [WorkspaceMemberEntity] = []
    @State private var invites: [WorkspaceInviteEntity] = []
    @State private var labels: [LabelEntity] = []
    @State private var projects: [ProjectEntity] = []
    @State private var users: [UserEntity] = []
    @State private var observationTask: Task<Void, Never>?
    @State private var showDeleteWorkspace = false
    @State private var deletingWorkspace = false
    @State private var deleteProjectTarget: ProjectEntity?
    @State private var deletingProject = false

    var body: some View {
        ZStack {
            AppBackground()

            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    // General — public toggle + write policy
                    WorkspaceGeneralSection(
                        accountId: accountId,
                        workspace: workspace,
                        workspacesApi: deps.workspacesApi
                    )

                    // Projects section
                    WorkspaceProjectsSection(
                        projects: projects.filter { $0.archivedAt == nil },
                        accountId: accountId,
                        projectsApi: deps.projectsApi,
                        integrationsApi: deps.integrationsApi,
                        installBaseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
                        onDelete: { project in deleteProjectTarget = project }
                    )

                    // Members section (includes invite controls)
                    WorkspaceMembersSection(
                        accountId: accountId,
                        members: members,
                        users: users,
                        currentUserId: deps.auth.userId,
                        membersApi: deps.workspaceMembersApi,
                        workspaceId: workspaceId,
                        invites: invites.filter { $0.acceptedAt == nil },
                        invitesApi: deps.workspaceInvitesApi
                    )

                    // Labels section
                    WorkspaceLabelsSection(
                        accountId: accountId,
                        workspaceId: workspaceId,
                        labels: labels,
                        labelsApi: deps.labelsApi
                    )

                    // Delete workspace (only for non-public workspaces)
                    if let workspace, !workspace.isPublic {
                        VStack(alignment: .leading, spacing: 12) {
                            Text("Danger Zone")
                                .font(.subheadline.weight(.semibold))
                                .foregroundStyle(.red.opacity(0.8))

                            Button {
                                showDeleteWorkspace = true
                            } label: {
                                HStack {
                                    Image(systemName: "trash")
                                    Text("Delete Workspace")
                                }
                                .font(.subheadline.weight(.medium))
                                .foregroundStyle(.red)
                                .frame(maxWidth: .infinity)
                                .padding(.vertical, 10)
                            }
                            .glassButton()
                            .buttonStyle(.plain)
                        }
                    }
                }
                .padding(16)
            }
        }
        .navigationTitle("Settings")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .onAppear { startObserving() }
        .onDisappear { observationTask?.cancel() }
        .alert("Delete Workspace", isPresented: $showDeleteWorkspace) {
            Button("Cancel", role: .cancel) {}
            Button("Delete", role: .destructive) {
                Task { await deleteWorkspace() }
            }
            .disabled(deletingWorkspace)
        } message: {
            Text("This will permanently delete \(workspace?.name ?? "this workspace") and all its projects, issues, and data. This cannot be undone.")
        }
        .alert("Delete Project", isPresented: Binding(
            get: { deleteProjectTarget != nil },
            set: { if !$0 { deleteProjectTarget = nil } }
        )) {
            Button("Cancel", role: .cancel) { deleteProjectTarget = nil }
            Button("Delete", role: .destructive) {
                if let project = deleteProjectTarget {
                    Task { await deleteProject(project) }
                }
            }
            .disabled(deletingProject)
        } message: {
            Text("This will permanently delete \(deleteProjectTarget?.name ?? "this project") and all its issues. This cannot be undone.")
        }
    }

    private func deleteWorkspace() async {
        deletingWorkspace = true
        defer { deletingWorkspace = false }
        do {
            try await deps.workspacesApi.delete(accountId: accountId, workspaceId: workspaceId)
            await MainActor.run { dismiss() }
        } catch {
            // Deletion failed — stay on the page
        }
    }

    private func deleteProject(_ project: ProjectEntity) async {
        deletingProject = true
        defer { deletingProject = false; deleteProjectTarget = nil }
        do {
            try await deps.workspacesApi.deleteProject(accountId: accountId, projectId: project.id)
        } catch {
            // Deletion failed — Electric will reconcile
        }
    }

    private func startObserving() {
        observationTask = Task {
            guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
            Task {
                let obs = ValueObservation.tracking { db in
                    try WorkspaceEntity.fetchOne(db, key: workspaceId)
                }
                for try await item in obs.values(in: pool) {
                    await MainActor.run { workspace = item }
                }
            }
            Task {
                let obs = ValueObservation.tracking { db in
                    try WorkspaceMemberEntity.filter(Column("workspace_id") == workspaceId).fetchAll(db)
                }
                for try await items in obs.values(in: pool) {
                    await MainActor.run { members = items }
                }
            }
            Task {
                let obs = ValueObservation.tracking { db in
                    try WorkspaceInviteEntity.filter(Column("workspace_id") == workspaceId).fetchAll(db)
                }
                for try await items in obs.values(in: pool) {
                    await MainActor.run { invites = items }
                }
            }
            Task {
                let obs = ValueObservation.tracking { db in
                    try LabelEntity.filter(Column("workspace_id") == workspaceId).fetchAll(db)
                }
                for try await items in obs.values(in: pool) {
                    await MainActor.run { labels = items }
                }
            }
            Task {
                let obs = ValueObservation.tracking { db in
                    try ProjectEntity.filter(Column("workspace_id") == workspaceId).fetchAll(db)
                }
                for try await items in obs.values(in: pool) {
                    await MainActor.run { projects = items }
                }
            }
            Task {
                let obs = ValueObservation.tracking { db in try UserEntity.fetchAll(db) }
                for try await items in obs.values(in: pool) {
                    await MainActor.run { users = items }
                }
            }
        }
    }
}
