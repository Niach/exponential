import ExpUI
import ExpCore
import SwiftUI
import GRDB

struct TeamSettingsView: View {
    let teamId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss
    @State private var team: TeamEntity?
    @State private var members: [TeamMemberEntity] = []
    @State private var invites: [TeamInviteEntity] = []
    @State private var labels: [LabelEntity] = []
    @State private var boards: [BoardEntity] = []
    @State private var users: [UserEntity] = []
    @State private var allTeams: [TeamEntity] = []
    @State private var observationTask: Task<Void, Never>?
    @State private var showDeleteTeam = false
    @State private var deletingTeam = false
    @State private var deleteBoardTarget: BoardEntity?
    @State private var deletingBoard = false
    @State private var dangerError: String?

    var body: some View {
        ZStack {
            AppBackground()

            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    // Boards section
                    TeamBoardsSection(
                        boards: boards.filter { $0.archivedAt == nil },
                        accountId: accountId,
                        teamId: teamId,
                        isOwner: isOwner,
                        boardsApi: deps.boardsApi,
                        repositoriesApi: deps.repositoriesApi,
                        onDelete: { board in deleteBoardTarget = board }
                    )

                    // Repositories registry (server-only, read over tRPC —
                    // masterplan §6). A pure registry with "used by" chips;
                    // both the GitHub connect (App install / grant capture)
                    // and the grant-model "Reconnect GitHub" hop run in-app
                    // (EXP-45), web parity with repositories-section.tsx.
                    TeamRepositoriesSection(
                        accountId: accountId,
                        team: team,
                        isOwner: isOwner,
                        repositoriesApi: deps.repositoriesApi,
                        integrationsApi: deps.integrationsApi,
                        instanceBaseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
                        protectedRepositoryIds: protectedRepositoryIds
                    )

                    // Members section (includes invite controls)
                    TeamMembersSection(
                        accountId: accountId,
                        members: members,
                        users: users,
                        currentUserId: deps.auth.userId,
                        membersApi: deps.teamMembersApi,
                        teamId: teamId,
                        invites: invites.filter { $0.acceptedAt == nil },
                        invitesApi: deps.teamInvitesApi,
                        isOwner: isOwner,
                        instanceBaseURL: deps.auth.instanceBaseURL(forAccountId: accountId)
                    )

                    // Labels section
                    TeamLabelsSection(
                        accountId: accountId,
                        teamId: teamId,
                        labels: labels,
                        labelsApi: deps.labelsApi
                    )

                    // Delete team — owner-only (hidden for non-owners, full
                    // web parity), and never for the shared feedback team
                    // (the server rejects deleting it anyway).
                    if let team, team.slug != "feedback", isOwner {
                        VStack(alignment: .leading, spacing: 12) {
                            Text("Danger Zone")
                                .font(.subheadline.weight(.semibold))
                                .foregroundStyle(.red.opacity(0.8))

                            Button {
                                showDeleteTeam = true
                            } label: {
                                HStack {
                                    Image(systemName: "trash")
                                    Text("Delete Team")
                                }
                                .font(.subheadline.weight(.medium))
                                .foregroundStyle(.red.opacity(isOnlyTeam ? 0.4 : 1))
                                .frame(maxWidth: .infinity)
                                .padding(.vertical, 10)
                                // Full-capsule hit target — .plain hit-tests only opaque pixels.
                                .contentShape(Rectangle())
                            }
                            .glassButton()
                            .buttonStyle(.plain)
                            .disabled(isOnlyTeam)

                            if isOnlyTeam {
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
        .alert("Delete Team", isPresented: $showDeleteTeam) {
            Button("Cancel", role: .cancel) {}
            Button("Delete", role: .destructive) {
                Task { await deleteTeam() }
            }
            .disabled(deletingTeam)
        } message: {
            Text("This will permanently delete \(team?.name ?? "this team") and all its boards, issues, and data. This cannot be undone.")
        }
        .alert("Delete Board", isPresented: Binding(
            get: { deleteBoardTarget != nil },
            set: { if !$0 { deleteBoardTarget = nil } }
        )) {
            Button("Cancel", role: .cancel) { deleteBoardTarget = nil }
            Button("Delete", role: .destructive) {
                if let board = deleteBoardTarget {
                    Task { await deleteBoard(board) }
                }
            }
            .disabled(deletingBoard)
        } message: {
            Text("Move \(deleteBoardTarget?.name ?? "this board") and all its issues, comments and attachments to trash? You can restore it from team settings for 48 hours; after that it is permanently deleted.")
        }
    }

    /// Repository management is owner-only (the server enforces team-owner
    /// on the `repositories` router mutations); everyone else reads the registry.
    private var isOwner: Bool {
        guard let me = deps.auth.userId else { return false }
        return members.contains { $0.userId == me && $0.role == DomainContract.teamRoleOwner }
    }

    /// Repos backing a protected board — their remove affordance is hidden.
    /// Derived from the already-observed team boards (no extra query).
    private var protectedRepositoryIds: Set<String> {
        Set(boards.filter { $0.isProtected }.compactMap { $0.repositoryId })
    }

    /// The GRDB teams table mirrors the membership-scoped Electric shape,
    /// so "synced teams minus feedback" == "my personal teams".
    /// Deleting the last one is server-refused (EXP-82); empty-while-loading
    /// biases the affordance to disabled, the safe default.
    private var isOnlyTeam: Bool {
        allTeams.filter { $0.slug != "feedback" }.count <= 1
    }

    private func deleteTeam() async {
        deletingTeam = true
        defer { deletingTeam = false }
        do {
            try await deps.teamsApi.delete(accountId: accountId, teamId: teamId)
            // Deleting the LAST team is server-refused (EXP-82), so a
            // successful delete always leaves a surviving membership —
            // ensureDefault is idempotent and resolves it as the new
            // landing spot without creating anything.
            _ = try? await deps.teamsApi.ensureDefault(accountId: accountId)
            // Membership changed, so every shape's server-derived where clause
            // rotated — relaunch the pipeline so all 15 shapes re-scope
            // immediately instead of waiting out the in-flight live long-polls
            // (up to ~60s of "deleted team still there / new personal
            // team missing").
            await deps.syncManager.restartPipeline(accountId: accountId)
            await MainActor.run {
                // Pop the whole stack to root — parent views (server detail /
                // settings) may still target the deleted team.
                NotificationCenter.default.post(name: .teamDeleted, object: nil)
            }
        } catch {
            dangerError = error.trpcUserMessage
        }
    }

    private func deleteBoard(_ board: BoardEntity) async {
        deletingBoard = true
        defer { deletingBoard = false; deleteBoardTarget = nil }
        do {
            try await deps.teamsApi.deleteBoard(accountId: accountId, boardId: board.id)
        } catch {
            dangerError = error.trpcUserMessage
        }
    }

    private func startObserving() {
        observationTask = Task {
            guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
            Task {
                let obs = ValueObservation.tracking { db in
                    try TeamEntity.fetchOne(db, key: teamId)
                }
                for try await item in obs.values(in: pool) {
                    await MainActor.run { team = item }
                }
            }
            Task {
                let obs = ValueObservation.tracking { db in
                    try TeamMemberEntity.filter(Column("team_id") == teamId).fetchAll(db)
                }
                for try await items in obs.values(in: pool) {
                    await MainActor.run { members = items }
                }
            }
            Task {
                let obs = ValueObservation.tracking { db in
                    try TeamInviteEntity.filter(Column("team_id") == teamId).fetchAll(db)
                }
                for try await items in obs.values(in: pool) {
                    await MainActor.run { invites = items }
                }
            }
            Task {
                let obs = ValueObservation.tracking { db in
                    try LabelEntity.filter(Column("team_id") == teamId).fetchAll(db)
                }
                for try await items in obs.values(in: pool) {
                    await MainActor.run { labels = items }
                }
            }
            Task {
                let obs = ValueObservation.tracking { db in
                    try BoardEntity.filter(Column("team_id") == teamId).fetchAll(db)
                }
                for try await items in obs.values(in: pool) {
                    await MainActor.run { boards = items }
                }
            }
            Task {
                let obs = ValueObservation.tracking { db in try UserEntity.fetchAll(db) }
                for try await items in obs.values(in: pool) {
                    await MainActor.run { users = items }
                }
            }
            Task {
                let obs = ValueObservation.tracking { db in try TeamEntity.fetchAll(db) }
                for try await items in obs.values(in: pool) {
                    await MainActor.run { allTeams = items }
                }
            }
        }
    }
}
