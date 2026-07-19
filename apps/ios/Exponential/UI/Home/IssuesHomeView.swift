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
        // Android-parity self-heal (EXP-82): Android's home bootstrap calls
        // teams.ensureDefault on every appearance, so an account that
        // ends up team-less (legacy signup, or an owner deleted a shared
        // team out from under us) heals itself. Deleting your LAST
        // team is server-refused, so this can never resurrect a
        // deliberately deleted one.
        .task { await healDefaultTeam() }
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

    // Nothing synced yet — offer to create the first board inline (a board
    // is backed by a GitHub repo, connected in the create sheet).
    private var emptyStateHint: some View {
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

    /// Resolve (creating if needed) the default team, then open the create
    /// sheet targeting it.
    private func beginCreateBoard() async {
        guard !preparingCreate, let accountId = deps.auth.activeAccountId else { return }
        preparingCreate = true
        defer { preparingCreate = false }
        if let team = await resolveDefaultTeam(accountId: accountId) {
            createTarget = CreateTarget(accountId: accountId, teamId: team.id)
        }
    }

    private func healDefaultTeam() async {
        guard let accountId = deps.auth.activeAccountId else { return }
        _ = await resolveDefaultTeam(accountId: accountId)
    }

    /// Resolve (creating if needed) the account's default team.
    private func resolveDefaultTeam(accountId: String) async -> TeamResult? {
        guard let team = try? await deps.teamsApi.ensureDefault(accountId: accountId) else {
            return nil
        }
        // If the team isn't in the local synced set, ensureDefault
        // just CREATED it — the membership change rotates every shape's
        // server-derived where clause, and the in-flight live long-polls
        // would keep the OLD scope for up to ~60s, so anything created
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
}
