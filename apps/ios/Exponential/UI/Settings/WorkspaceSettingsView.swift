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
    @State private var allWorkspaces: [WorkspaceEntity] = []
    @State private var observationTask: Task<Void, Never>?
    @State private var showDeleteWorkspace = false
    @State private var deletingWorkspace = false
    @State private var deleteProjectTarget: ProjectEntity?
    @State private var deletingProject = false
    @State private var dangerError: String?

    var body: some View {
        ZStack {
            AppBackground()

            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    // Projects section
                    WorkspaceProjectsSection(
                        projects: projects.filter { $0.archivedAt == nil },
                        accountId: accountId,
                        workspaceId: workspaceId,
                        isOwner: isOwner,
                        projectsApi: deps.projectsApi,
                        repositoriesApi: deps.repositoriesApi,
                        onDelete: { project in deleteProjectTarget = project }
                    )

                    // Repositories registry (server-only, read over tRPC —
                    // masterplan §6). A pure registry with "used by" chips;
                    // both the GitHub connect (App install / grant capture)
                    // and the grant-model "Reconnect GitHub" hop run in-app
                    // (EXP-45), web parity with repositories-section.tsx.
                    WorkspaceRepositoriesSection(
                        accountId: accountId,
                        workspace: workspace,
                        isOwner: isOwner,
                        repositoriesApi: deps.repositoriesApi,
                        integrationsApi: deps.integrationsApi,
                        instanceBaseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
                        protectedRepositoryIds: protectedRepositoryIds
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
                        invitesApi: deps.workspaceInvitesApi,
                        isOwner: isOwner,
                        instanceBaseURL: deps.auth.instanceBaseURL(forAccountId: accountId)
                    )

                    // Labels section
                    WorkspaceLabelsSection(
                        accountId: accountId,
                        workspaceId: workspaceId,
                        labels: labels,
                        labelsApi: deps.labelsApi
                    )

                    // Delete workspace — owner-only (hidden for non-owners, full
                    // web parity), and never for the shared feedback workspace
                    // (the server rejects deleting it anyway).
                    if let workspace, workspace.slug != "feedback", isOwner {
                        VStack(alignment: .leading, spacing: 12) {
                            Text("Danger Zone")
                                .font(.subheadline.weight(.semibold))
                                .foregroundStyle(.red.opacity(0.8))

                            Button {
                                showDeleteWorkspace = true
                            } label: {
                                HStack {
                                    Image(systemName: "trash")
                                    Text("Delete Team")
                                }
                                .font(.subheadline.weight(.medium))
                                .foregroundStyle(.red.opacity(isOnlyWorkspace ? 0.4 : 1))
                                .frame(maxWidth: .infinity)
                                .padding(.vertical, 10)
                                // Full-capsule hit target — .plain hit-tests only opaque pixels.
                                .contentShape(Rectangle())
                            }
                            .glassButton()
                            .buttonStyle(.plain)
                            .disabled(isOnlyWorkspace)

                            if isOnlyWorkspace {
                                Text("This is your only team, so it can't be deleted.")
                                    .font(.caption)
                                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            }

                            if let dangerError {
                                Text(dangerError)
                                    .font(.caption)
                                    .foregroundStyle(.red.opacity(0.8))
                            }
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
        .alert("Delete Team", isPresented: $showDeleteWorkspace) {
            Button("Cancel", role: .cancel) {}
            Button("Delete", role: .destructive) {
                Task { await deleteWorkspace() }
            }
            .disabled(deletingWorkspace)
        } message: {
            Text("This will permanently delete \(workspace?.name ?? "this team") and all its projects, issues, and data. This cannot be undone.")
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
            Text("Move \(deleteProjectTarget?.name ?? "this project") and all its issues, comments and attachments to trash? You can restore it from team settings for 48 hours; after that it is permanently deleted.")
        }
    }

    /// Repository management is owner-only (the server enforces workspace-owner
    /// on the `repositories` router mutations); everyone else reads the registry.
    private var isOwner: Bool {
        guard let me = deps.auth.userId else { return false }
        return members.contains { $0.userId == me && $0.role == DomainContract.workspaceRoleOwner }
    }

    /// Repos backing a protected project — their remove affordance is hidden.
    /// Derived from the already-observed workspace projects (no extra query).
    private var protectedRepositoryIds: Set<String> {
        Set(projects.filter { $0.isProtected }.compactMap { $0.repositoryId })
    }

    /// The GRDB workspaces table mirrors the membership-scoped Electric shape,
    /// so "synced workspaces minus feedback" == "my personal workspaces".
    /// Deleting the last one is server-refused (EXP-82); empty-while-loading
    /// biases the affordance to disabled, the safe default.
    private var isOnlyWorkspace: Bool {
        allWorkspaces.filter { $0.slug != "feedback" }.count <= 1
    }

    private func deleteWorkspace() async {
        deletingWorkspace = true
        defer { deletingWorkspace = false }
        do {
            try await deps.workspacesApi.delete(accountId: accountId, workspaceId: workspaceId)
            // Deleting the LAST workspace is server-refused (EXP-82), so a
            // successful delete always leaves a surviving membership —
            // ensureDefault is idempotent and resolves it as the new
            // landing spot without creating anything.
            _ = try? await deps.workspacesApi.ensureDefault(accountId: accountId)
            // Membership changed, so every shape's server-derived where clause
            // rotated — relaunch the pipeline so all 15 shapes re-scope
            // immediately instead of waiting out the in-flight live long-polls
            // (up to ~60s of "deleted workspace still there / new personal
            // workspace missing").
            await deps.syncManager.restartPipeline(accountId: accountId)
            await MainActor.run {
                // Pop the whole stack to root — parent views (server detail /
                // settings) may still target the deleted workspace.
                NotificationCenter.default.post(name: .workspaceDeleted, object: nil)
            }
        } catch {
            dangerError = error.trpcUserMessage
        }
    }

    private func deleteProject(_ project: ProjectEntity) async {
        deletingProject = true
        defer { deletingProject = false; deleteProjectTarget = nil }
        do {
            try await deps.workspacesApi.deleteProject(accountId: accountId, projectId: project.id)
        } catch {
            dangerError = error.trpcUserMessage
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
            Task {
                let obs = ValueObservation.tracking { db in try WorkspaceEntity.fetchAll(db) }
                for try await items in obs.values(in: pool) {
                    await MainActor.run { allWorkspaces = items }
                }
            }
        }
    }
}
