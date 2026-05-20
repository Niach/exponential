import SwiftUI
import GRDB

struct WorkspaceSettingsView: View {
    let workspaceId: String

    @Environment(AppDependencies.self) private var deps
    @State private var workspace: WorkspaceEntity?
    @State private var members: [WorkspaceMemberEntity] = []
    @State private var invites: [WorkspaceInviteEntity] = []
    @State private var labels: [LabelEntity] = []
    @State private var users: [UserEntity] = []
    @State private var observationTask: Task<Void, Never>?

    var body: some View {
        ZStack {
            AppBackground()

            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    // General — public toggle + write policy
                    WorkspaceGeneralSection(
                        workspace: workspace,
                        workspacesApi: deps.workspacesApi
                    )

                    // Members section
                    WorkspaceMembersSection(
                        members: members,
                        users: users,
                        currentUserId: deps.auth.userId,
                        membersApi: deps.workspaceMembersApi
                    )

                    // Invites section
                    WorkspaceInvitesSection(
                        workspaceId: workspaceId,
                        invites: invites.filter { $0.acceptedAt == nil },
                        invitesApi: deps.workspaceInvitesApi
                    )

                    // Labels section
                    WorkspaceLabelsSection(
                        workspaceId: workspaceId,
                        labels: labels,
                        labelsApi: deps.labelsApi
                    )
                }
                .padding(16)
            }
        }
        .navigationTitle("Settings")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .onAppear { startObserving() }
        .onDisappear { observationTask?.cancel() }
    }

    private func startObserving() {
        observationTask = Task {
            Task {
                let obs = ValueObservation.tracking { db in
                    try WorkspaceEntity.fetchOne(db, key: workspaceId)
                }
                for try await item in obs.values(in: deps.db.dbPool) {
                    await MainActor.run { workspace = item }
                }
            }
            Task {
                let obs = ValueObservation.tracking { db in
                    try WorkspaceMemberEntity.filter(Column("workspace_id") == workspaceId).fetchAll(db)
                }
                for try await items in obs.values(in: deps.db.dbPool) {
                    await MainActor.run { members = items }
                }
            }
            Task {
                let obs = ValueObservation.tracking { db in
                    try WorkspaceInviteEntity.filter(Column("workspace_id") == workspaceId).fetchAll(db)
                }
                for try await items in obs.values(in: deps.db.dbPool) {
                    await MainActor.run { invites = items }
                }
            }
            Task {
                let obs = ValueObservation.tracking { db in
                    try LabelEntity.filter(Column("workspace_id") == workspaceId).fetchAll(db)
                }
                for try await items in obs.values(in: deps.db.dbPool) {
                    await MainActor.run { labels = items }
                }
            }
            Task {
                let obs = ValueObservation.tracking { db in try UserEntity.fetchAll(db) }
                for try await items in obs.values(in: deps.db.dbPool) {
                    await MainActor.run { users = items }
                }
            }
        }
    }
}
