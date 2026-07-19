import ExpUI
import ExpCore
import SwiftUI
import GRDB

/// Root of the Issues tab: the issue list of the current board, with an
/// inline board switcher in the navigation bar (board name + up/down
/// chevron → `BoardSwitcherSheet`). Replaces the old Boards overview as
/// the app's home — switching boards swaps the list in place, no push.
struct IssuesHomeView: View {
    let syncing: Bool
    let currentBoard: CurrentBoardRef?
    let boardLoader: MultiAccountBoardLoader?
    let onSelectBoard: (_ accountId: String, _ boardId: String) -> Void

    @Environment(AppDependencies.self) private var deps
    @State private var showSwitcher = false
    @State private var preparingCreate = false
    @State private var createTarget: CreateTarget?
    @State private var showTeamSetup = false
    // The active account's locally-synced teams (nil until the first
    // observation delivers) — drives the zero-team empty state (EXP-188:
    // signups get no auto-created team, so an account can be team-less).
    @State private var syncedTeams: [TeamEntity]?

    private struct CreateTarget: Identifiable {
        let accountId: String
        let teamId: String
        var id: String { "\(accountId)/\(teamId)" }
    }

    var body: some View {
        ZStack {
            AppBackground()

            if let current = currentBoard {
                IssueListView(boardId: current.boardId)
                    .environment(\.accountId, current.accountId)
                    // Remount on switch so the list view model rebinds to the
                    // selected board (it captures boardId at creation).
                    .id(current)
            } else if syncing {
                VStack(spacing: 12) {
                    ProgressView()
                        .tint(.white)
                    Text("Syncing...")
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
            } else {
                emptyStateHint
            }
        }
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .toolbar {
            ToolbarItem(placement: .principal) {
                switcherControl
            }
            ToolbarItem(placement: .topBarTrailing) {
                settingsButton
            }
        }
        .sheet(isPresented: $showSwitcher) {
            BoardSwitcherSheet(
                boardLoader: boardLoader,
                currentBoard: currentBoard,
                onSelect: { accountId, boardId in
                    showSwitcher = false
                    onSelectBoard(accountId, boardId)
                }
            )
            .presentationDetents([.medium, .large])
            .presentationDragIndicator(.visible)
            .presentationBackground(.ultraThinMaterial)
        }
        .sheet(item: $createTarget) { target in
            CreateBoardSheet(
                accountId: target.accountId,
                teamId: target.teamId,
                onCreated: { boardId in onSelectBoard(target.accountId, boardId) }
            )
            .presentationBackground(.ultraThinMaterial)
        }
        .sheet(isPresented: $showTeamSetup) {
            TeamSetupSheet()
                .presentationBackground(.ultraThinMaterial)
        }
        // Observe the active account's synced teams so the empty state can
        // distinguish "no boards yet" from "no team at all" (EXP-188).
        .task(id: deps.auth.activeAccountId) { await observeTeams() }
    }

    // MARK: - Switcher control

    private var hasAnyBoards: Bool {
        !(boardLoader?.groups ?? []).isEmpty
    }

    private var currentBoardName: String? {
        guard let current = currentBoard else { return nil }
        for group in boardLoader?.groups ?? [] where group.accountId == current.accountId {
            for block in group.teamBlocks {
                if let board = block.boards.first(where: { $0.id == current.boardId }) {
                    return board.name
                }
            }
        }
        return nil
    }

    /// One tappable control: current board name + the combobox-style
    /// up/down chevron. Disabled until there is anything to switch to.
    private var switcherControl: some View {
        Button {
            showSwitcher = true
        } label: {
            HStack(spacing: 5) {
                Text(currentBoardName ?? "Boards")
                    .font(.headline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Image(systemName: "chevron.up.chevron.down")
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!hasAnyBoards)
        .opacity(hasAnyBoards ? 1 : 0.5)
        .accessibilityLabel("Switch board")
    }

    private var settingsButton: some View {
        NavigationLink(value: AppRoute.settings) {
            Image(systemName: "gearshape")
                .font(.body)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(width: 32, height: 32)
                .contentShape(Circle())
        }
    }

    // MARK: - Empty state

    /// True when the teams observation has delivered and the account has no
    /// membership beyond the shared feedback team — the EXP-188 zero-team
    /// state (fresh signup that skipped onboarding's team step, or an owner
    /// who deleted their last team).
    private var hasNoTeam: Bool {
        guard let syncedTeams else { return false }
        return !syncedTeams.contains { $0.slug != "feedback" }
    }

    // Nothing synced yet. Team-less accounts get the create-or-join choice
    // (EXP-188 — there is no auto-created team to target a board at);
    // everyone else gets the create-first-board path.
    @ViewBuilder
    private var emptyStateHint: some View {
        if hasNoTeam {
            VStack(spacing: 12) {
                Image(systemName: "person.2")
                    .font(.title2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                Text("No team yet")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text("Create a team, or join one with an invite link from a teammate.")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .multilineTextAlignment(.center)

                Button {
                    showTeamSetup = true
                } label: {
                    HStack(spacing: 6) {
                        Image(systemName: "plus")
                            .font(.caption.weight(.semibold))
                        Text("Create or join a team")
                            .font(.subheadline.weight(.medium))
                    }
                    .foregroundStyle(.white)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 10)
                    .glassButton()
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, 40)
        } else {
            VStack(spacing: 12) {
                Image(systemName: "tray")
                    .font(.title2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                Text("No boards yet")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text("Create your first board to get started.")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .multilineTextAlignment(.center)

                Button {
                    Task { await beginCreateBoard() }
                } label: {
                    HStack(spacing: 6) {
                        if preparingCreate {
                            ProgressView().controlSize(.small).tint(.white)
                        } else {
                            Image(systemName: "plus")
                                .font(.caption.weight(.semibold))
                        }
                        Text("Create board")
                            .font(.subheadline.weight(.medium))
                    }
                    .foregroundStyle(.white)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 10)
                    .glassButton()
                }
                .buttonStyle(.plain)
                .disabled(preparingCreate)
            }
            .padding(.horizontal, 40)
        }
    }

    /// Resolve the default team, then open the create sheet targeting it.
    /// A team-less account never reaches this path (the empty state offers
    /// create-or-join instead), so a nil resolution just no-ops.
    private func beginCreateBoard() async {
        guard !preparingCreate, let accountId = deps.auth.activeAccountId else { return }
        preparingCreate = true
        defer { preparingCreate = false }
        if let team = await resolveDefaultTeam(accountId: accountId) {
            createTarget = CreateTarget(accountId: accountId, teamId: team.id)
        }
    }

    /// Resolve the account's default team (teams.getDefault NEVER creates —
    /// EXP-188; oldest non-feedback membership or nil).
    private func resolveDefaultTeam(accountId: String) async -> TeamResult? {
        // `try?` flattens the optional (SE-0230): a thrown error and a nil
        // resolution both land here as nil — either way there's no team.
        guard let team = try? await deps.teamsApi.getDefault(accountId: accountId) else {
            return nil
        }
        // If the resolved team isn't in the local synced set yet, the sync
        // pipeline is lagging behind a membership change — the in-flight live
        // long-polls keep the OLD scope for up to ~60s, so anything created
        // next would "show up nowhere". Relaunch the pipeline so the fresh
        // scope syncs in seconds (EXP-46; same drain-lag gap as EXP-43).
        var alreadySynced = false
        if let pool = try? deps.db.pool(forAccountId: accountId) {
            alreadySynced = (try? await pool.read { db in
                try TeamEntity.fetchOne(db, key: team.id) != nil
            }) ?? false
        }
        if !alreadySynced {
            await deps.syncManager.restartPipeline(accountId: accountId)
        }
        return team
    }

    /// Long-lived teams observation for the active account (cancelled and
    /// restarted by `.task(id:)` when the account switches).
    private func observeTeams() async {
        syncedTeams = nil
        guard let accountId = deps.auth.activeAccountId,
              let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let obs = ValueObservation.tracking { db in
            try TeamEntity.fetchAll(db)
        }
        do {
            for try await teams in obs.values(in: pool) {
                await MainActor.run { syncedTeams = teams }
            }
        } catch {
            // Observation ended (pool closed on sign-out) — leave the last
            // snapshot in place; the .task(id:) restart handles account swaps.
        }
    }
}
