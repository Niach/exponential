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
    @Environment(TeamState.self) private var teamState
    @Environment(\.scenePhase) private var scenePhase
    @State private var showSwitcher = false
    // EXP-698 r5: the getting-started checklist's live state. Owned HERE (the
    // Issues root) and published into the environment, so the root board's
    // empty state and this view's own "No boards yet" both render it while a
    // pushed `.board` does not. ONE box for this view's lifetime: a fresh one
    // per body pass would invalidate every reader on every render.
    @State private var gettingStartedContext = GettingStartedContext()
    /// What the board switcher asked for while it was still on screen — run
    /// on its dismissal, never in the same transaction (see the sheet below).
    @State private var pendingSwitcherAction: SwitcherAction?
    @State private var preparingCreate = false
    @State private var createTarget: CreateTarget?
    @State private var showTeamSetup = false
    // The active account's locally-synced teams (nil until the first
    // observation delivers) — drives the zero-team empty state (EXP-188:
    // signups get no auto-created team, so an account can be team-less).
    @State private var syncedTeams: [TeamEntity]?

    /// What the switcher parked for after it closes.
    private enum SwitcherAction {
        case createBoard
        case createTeam
    }

    private var gettingStarted: GettingStartedProgress { gettingStartedContext.progress }

    private struct CreateTarget: Identifiable {
        let accountId: String
        let teamId: String
        var id: String { "\(accountId)/\(teamId)" }
    }

    var body: some View {
        ZStack {
            AppBackground()

            if let current = currentBoard {
                IssueListView(boardId: current.boardId, showsSettingsButton: true)
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
        .environment(\.gettingStarted, gettingStartedContext)
        .onAppear {
            gettingStartedContext.onCreateBoard = { Task { await beginCreateBoard() } }
        }
        // The GitHub install state is a server-only read (§refreshGithubStatus)
        // — a transient failure at bind would otherwise leave the step
        // "available" for the whole session, and connecting the App happens
        // out in Safari anyway, so re-read it on every foreground.
        .onChange(of: scenePhase) { _, phase in
            if phase == .active { gettingStarted.refreshGithubStatus() }
        }
        // Rebind the checklist's observations whenever the account or the team
        // in view changes; a team-less account has nothing to check off.
        .task(id: checklistKey) {
            guard let accountId = deps.auth.activeAccountId, let teamId = checklistTeamId else {
                gettingStarted.stop()
                return
            }
            gettingStarted.bind(accountId: accountId, teamId: teamId, deps: deps)
        }
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .toolbar {
            ToolbarItem(placement: .principal) {
                switcherControl
            }
            // With a board mounted, IssueListView owns the trailing group so
            // the order is filter → settings (EXP-331); the gear only renders
            // from here on the board-less branches (syncing / empty state).
            if currentBoard == nil {
                ToolbarItem(placement: .topBarTrailing) {
                    SettingsToolbarLink()
                }
            }
        }
        // The two creation entries PARK their intent and run it once the
        // switcher has finished dismissing (the same hand-off
        // `IssueDetailView.promoteMoveTarget` makes): closing this sheet and
        // presenting the next one in ONE transaction makes SwiftUI drop the
        // second presentation, since both hang off this node.
        .sheet(isPresented: $showSwitcher, onDismiss: runPendingSwitcherAction) {
            BoardSwitcherSheet(
                boardLoader: boardLoader,
                currentBoard: currentBoard,
                onSelect: { accountId, boardId in
                    showSwitcher = false
                    onSelectBoard(accountId, boardId)
                },
                // EXP-698 r5: the switcher is where a board or a team is
                // created from, exactly like Android's — reaching either used
                // to mean leaving the sheet for Settings.
                onCreateBoard: {
                    pendingSwitcherAction = .createBoard
                    showSwitcher = false
                },
                onCreateTeam: {
                    pendingSwitcherAction = .createTeam
                    showSwitcher = false
                }
            )
        }
        .sheet(item: $createTarget) { target in
            CreateBoardSheet(
                accountId: target.accountId,
                teamId: target.teamId,
                onCreated: { boardId in onSelectBoard(target.accountId, boardId) }
            )
        }
        .sheet(isPresented: $showTeamSetup) {
            TeamSetupSheet()
        }
        // Observe the active account's synced teams so the empty state can
        // distinguish "no boards yet" from "no team at all" (EXP-188).
        .task(id: deps.auth.activeAccountId) { await observeTeams() }
    }

    /// Runs whatever the switcher parked, now that it is gone and this node
    /// owns no presentation.
    private func runPendingSwitcherAction() {
        guard let action = pendingSwitcherAction else { return }
        pendingSwitcherAction = nil
        switch action {
        case .createBoard:
            Task { await beginCreateBoard() }
        case .createTeam:
            showTeamSetup = true
        }
    }

    // MARK: - Getting started (EXP-698 r5)

    /// The team the checklist is scoped to: the board in view wins (it is the
    /// one the empty state belongs to), else the active team, else the account's
    /// only synced team.
    private var checklistTeamId: String? {
        if let board = currentBoardEntity { return board.teamId }
        return teamState.activeTeam?.id ?? syncedTeams?.first?.id
    }

    private var checklistKey: String {
        "\(deps.auth.activeAccountId ?? "")/\(checklistTeamId ?? "")"
    }

    // MARK: - Switcher control

    private var hasAnyBoards: Bool {
        !(boardLoader?.groups ?? []).isEmpty
    }

    private var currentBoardEntity: BoardEntity? {
        guard let current = currentBoard else { return nil }
        for group in boardLoader?.groups ?? [] where group.accountId == current.accountId {
            for block in group.teamBlocks {
                if let board = block.boards.first(where: { $0.id == current.boardId }) {
                    return board
                }
            }
        }
        return nil
    }

    private var currentBoardName: String? {
        currentBoardEntity?.name
    }

    /// One tappable control: Android's `BoardSwitcherControl` chip (EXP-698
    /// r7) — the board's own glyph + its name + the combobox-style up/down
    /// expander in a single Md glass pill, instead of the bare headline this
    /// used to be.
    ///
    /// With nowhere to switch to the pill stays ENABLED-looking: this is the
    /// screen's TITLE as much as it is a control, and `enabled: false` would
    /// drop the board name to quaternary, which reads as "this board is
    /// broken". Only the expander glyph dims and the tap goes nowhere —
    /// Android's `onClick = if (enabled) onClick else null`.
    private var switcherControl: some View {
        GlassPill(
            currentBoardName ?? "Issues",
            size: .md,
            mode: .action {
                guard hasAnyBoards else { return }
                showSwitcher = true
            }
        ) {
            // Board glyph tinted with the board color — same idiom as the
            // board switcher sheet this control opens (EXP-449).
            if let board = currentBoardEntity {
                AppIcon(BoardTypeDisplay.iconName(for: board), size: GlassPillTokens.glyphMd)
                    .foregroundStyle(Color(hex: board.color ?? "#888888") ?? .gray)
            }
        } trailing: {
            AppIcon(AppIcons.navTeamSwitcher, size: GlassPillTokens.glyphMd)
                .foregroundStyle(.white.opacity(hasAnyBoards ? TextOpacity.secondary : TextOpacity.quaternary))
        }
        // MUST stay on the tappable element: the styleguide/screenshot suites
        // reach the switcher via `app.buttons["Switch board"]`.
        .accessibilityLabel("Switch board")
    }

    // MARK: - Empty state

    /// True when the teams observation has delivered and the account has no
    /// membership at all — the EXP-188 zero-team state (fresh signup that
    /// skipped onboarding's team step, or an owner who deleted their last
    /// team). Every synced team counts: EXP-364 killed the feedback-team
    /// special case, so a team slugged `feedback` is an ordinary team.
    private var hasNoTeam: Bool {
        guard let syncedTeams else { return false }
        return syncedTeams.isEmpty
    }

    // Nothing synced yet. Team-less accounts get the create-or-join choice
    // (EXP-188 — there is no auto-created team to target a board at);
    // everyone else gets the create-first-board path.
    @ViewBuilder
    private var emptyStateHint: some View {
        if hasNoTeam {
            VStack(spacing: 12) {
                AppIcon(AppIcons.settingsMembers, size: 22)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                Text("No team yet")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text("Create a team, or join one with an invite link from a teammate.")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .multilineTextAlignment(.center)

                GlassPill(
                    "Create or join a team",
                    icon: AppIcons.uiAdd,
                    size: .md,
                    mode: .action { showTeamSetup = true }
                )
            }
            .padding(.horizontal, 40)
        } else {
            // EXP-698 r5: the shared empty-state shape (glyph in a 48pt washed
            // circle, headline, one line, primary pill) with the
            // getting-started checklist under it — the same block the empty
            // BOARD renders, and the same one web and the IDE draw for a
            // board-less team.
            ScrollView {
                VStack(spacing: 24) {
                    VStack(spacing: 12) {
                        AppIcon(AppIcons.navBoards, size: AppIcon.Size.large)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .frame(width: 48, height: 48)
                            .background(GlassTokens.fillActive, in: Circle())

                        Text("No boards yet")
                            .font(.headline)
                            .foregroundStyle(.white)

                        Text("Create a board to start tracking work.")
                            .font(.subheadline)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .multilineTextAlignment(.center)

                        GlassPill(
                            "Create board",
                            size: .md,
                            mode: .action { Task { await beginCreateBoard() } },
                            primary: true,
                            enabled: !preparingCreate
                        ) {
                            if preparingCreate {
                                ProgressView()
                                    .controlSize(.small)
                                    .tint(DesignTokens.Palette.primaryForeground)
                            } else {
                                AppIcon(AppIcons.uiAdd, size: GlassPillTokens.glyphMd)
                            }
                        }
                    }
                    .frame(maxWidth: .infinity)

                    if let accountId = deps.auth.activeAccountId, let teamId = checklistTeamId {
                        GettingStartedCards(
                            progress: gettingStarted,
                            accountId: accountId,
                            teamId: teamId,
                            onCreateBoard: { Task { await beginCreateBoard() } }
                        )
                    }
                }
                .padding(.horizontal, 16)
                .padding(.top, 48)
                .padding(.bottom, 24)
            }
            .scrollBounceBehavior(.basedOnSize)
            .tabBarBottomInset()
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

/// The nav-bar Settings gear — one definition shared by IssuesHomeView's
/// board-less branches and IssueListView's Root-mode trailing group, so the
/// glyph and hit target stay identical wherever it renders (EXP-331).
struct SettingsToolbarLink: View {
    var body: some View {
        NavigationLink(value: AppRoute.settings) {
            AppIcon(AppIcons.navSettings, size: AppIcon.Size.medium)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(width: 32, height: 32)
                .contentShape(Circle())
        }
        // Icon-only: without an explicit label VoiceOver reads the raw asset
        // name, and the screenshot suites had no stable way to find it.
        .accessibilityLabel("Settings")
        .accessibilityIdentifier("nav-settings-link")
    }
}
