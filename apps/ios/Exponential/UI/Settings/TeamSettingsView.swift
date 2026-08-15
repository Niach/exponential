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
    @State private var labels: [LabelEntity] = []
    @State private var boards: [BoardEntity] = []
    @State private var users: [UserEntity] = []
    // Each observation loop is stored and cancelled individually — a single
    // wrapper task would NOT propagate cancellation into unstructured inner
    // `Task {}` loops, and the view re-arms on every appear, so leaked loops
    // would accumulate per push/pop.
    @State private var observationTasks: [Task<Void, Never>] = []
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
                        boards: boards,
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
                        instanceBaseURL: deps.auth.instanceBaseURL(forAccountId: accountId)
                    )

                    // Members section (view/manage only — inviting is web-only,
                    // EXP-216)
                    TeamMembersSection(
                        accountId: accountId,
                        members: members,
                        users: users,
                        currentUserId: deps.auth.userId,
                        membersApi: deps.teamMembersApi,
                        isOwner: isOwner
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
                                    AppIcon(AppIcons.uiDelete, size: AppIcon.Size.medium)
                                    Text("Delete Team")
                                }
                                .font(.subheadline.weight(.medium))
                                .foregroundStyle(.red)
                                .frame(maxWidth: .infinity)
                                .padding(.vertical, 10)
                                // Full-capsule hit target — .plain hit-tests only opaque pixels.
                                .contentShape(Rectangle())
                            }
                            .glassButton()
                            .buttonStyle(.plain)

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
        .onDisappear { stopObserving() }
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

    private func deleteTeam() async {
        deletingTeam = true
        defer { deletingTeam = false }
        do {
            // An owner may delete ANY of their teams, including the last one
            // (EXP-188 dropped the last-team guard) — a team-less account
            // lands on the zero-team create-or-join empty state.
            try await deps.teamsApi.delete(accountId: accountId, teamId: teamId)
            // Membership changed, so every shape's server-derived where clause
            // rotated — relaunch the pipeline so all shapes re-scope
            // immediately instead of waiting out the in-flight live long-polls
            // (up to ~60s of "deleted team still there").
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
        stopObserving() // restartable: the view re-arms on every appear
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let teamId = teamId

        observationTasks.append(Task {
            let obs = ValueObservation.tracking { db in
                try TeamEntity.fetchOne(db, key: teamId)
            }
            do {
                for try await item in obs.values(in: pool) {
                    await MainActor.run { team = item }
                }
            } catch {}
        })
        observationTasks.append(Task {
            let obs = ValueObservation.tracking { db in
                try TeamMemberEntity.filter(Column("team_id") == teamId).fetchAll(db)
            }
            do {
                for try await items in obs.values(in: pool) {
                    await MainActor.run { members = items }
                }
            } catch {}
        })
        observationTasks.append(Task {
            let obs = ValueObservation.tracking { db in
                try LabelEntity.filter(Column("team_id") == teamId).fetchAll(db)
            }
            do {
                for try await items in obs.values(in: pool) {
                    await MainActor.run { labels = items }
                }
            } catch {}
        })
        observationTasks.append(Task {
            let obs = ValueObservation.tracking { db in
                try BoardEntity.filter(Column("team_id") == teamId).fetchAll(db)
            }
            do {
                for try await items in obs.values(in: pool) {
                    await MainActor.run { boards = items }
                }
            } catch {}
        })
        observationTasks.append(Task {
            let obs = ValueObservation.tracking { db in try UserEntity.fetchAll(db) }
            do {
                for try await items in obs.values(in: pool) {
                    await MainActor.run { users = items }
                }
            } catch {}
        })
    }

    private func stopObserving() {
        for task in observationTasks { task.cancel() }
        observationTasks = []
    }
}
