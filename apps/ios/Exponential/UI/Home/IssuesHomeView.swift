import ExpUI
import ExpCore
import SwiftUI
import GRDB

/// Root of the Board tab: the issue list of the current board under the
/// page header (#EXP-1384 `shell.header`: the board's name as the large
/// title, the native search + inbox circles). The board is switched ONLY
/// through the Board tab's board menu (MobileTabBar) — switching swaps the
/// list in place, no push.
struct IssuesHomeView: View {
    let syncing: Bool
    let currentBoard: CurrentBoardRef?
    let boardLoader: MultiAccountBoardLoader?
    let onSelectBoard: (_ accountId: String, _ boardId: String) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(TeamState.self) private var teamState
    @Environment(\.scenePhase) private var scenePhase
    // EXP-698 r5: the getting-started checklist's live state. Owned HERE (the
    // Issues root) and published into the environment, so the root board's
    // empty state and this view's own "No boards yet" both render it while a
    // pushed `.board` does not. ONE box for this view's lifetime: a fresh one
    // per body pass would invalidate every reader on every render.
    @State private var gettingStartedContext = GettingStartedContext()
    @State private var preparingCreate = false
    @State private var createTarget: CreateTarget?
    @State private var showTeamSetup = false
    // The active account's locally-synced teams (nil until the first
    // observation delivers) — drives the zero-team empty state (EXP-188:
    // signups get no auto-created team, so an account can be team-less).
    @State private var syncedTeams: [TeamEntity]?
    // EXP-1115: true once the active account's boards shape has reached
    // up-to-date at least once (initial snapshot done, even at zero rows).
    // Until then a board-less-looking account is still syncing — the root
    // shows the spinner rather than flashing "No boards yet" (or, right after
    // a full resync's wipe, "No team yet") plus the getting-started checklist
    // while the snapshot lands. Android parity: `activeAccountBoardsSynced`.
    @State private var boardsSynced = false
    /// EXP-1373 (chats.json `shell.boardChips`): All issues · My issues ·
    /// Drafts (only while there are drafts) under the header; My issues =
    /// the viewer's assigned issues of the ACTIVE team across its boards,
    /// grouped by status like the board.
    @State private var chip: Chats.Shell.BoardChip = .all
    @State private var myIssues: MyIssuesViewModel?
    @State private var drafts: DraftsViewModel?

    private var gettingStarted: GettingStartedProgress { gettingStartedContext.progress }

    private struct CreateTarget: Identifiable {
        let accountId: String
        let teamId: String
        var id: String { "\(accountId)/\(teamId)" }
    }

    var body: some View {
        ZStack {
            AppBackground()

            if shownChip == .mine, let myIssues {
                MyIssuesListContent(model: myIssues)
                    .environment(\.accountId, deps.auth.activeAccountId ?? "")
                    .tabBarBottomInset()
            } else if shownChip == .drafts, let drafts {
                DraftsListContent(viewModel: drafts)
                    .environment(\.accountId, deps.auth.activeAccountId ?? "")
                    .tabBarBottomInset()
            } else if let current = currentBoard {
                IssueListView(boardId: current.boardId, ownsHeader: false)
                    .environment(\.accountId, current.accountId)
                    // Remount on switch so the list view model rebinds to the
                    // selected board (it captures boardId at creation).
                    .id(current)
            } else if syncing || !boardsSynced {
                // EXP-1275: the first load is a placeholder, not "Syncing...".
                SkeletonRows()
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            } else {
                emptyStateHint
            }
        }
        .safeAreaInset(edge: .top, spacing: 0) {
            if currentBoard != nil || shownChip != .all {
                boardChips
            }
        }
        .environment(\.gettingStarted, gettingStartedContext)
        .onAppear {
            gettingStartedContext.onCreateBoard = { Task { await beginCreateBoard() } }
            if let accountId = deps.auth.activeAccountId {
                if myIssues == nil {
                    myIssues = MyIssuesViewModel(accountId: accountId, db: deps.db, auth: deps.auth)
                }
                if drafts == nil {
                    drafts = DraftsViewModel(accountId: accountId, db: deps.db, api: deps.issueDraftsApi)
                }
            }
            myIssues?.teamId = teamState.activeTeamId
            drafts?.teamId = teamState.activeTeamId
            myIssues?.startObserving()
            drafts?.startObserving()
        }
        .onDisappear {
            myIssues?.stopObserving()
            drafts?.stopObserving()
        }
        .onChange(of: teamState.activeTeamId) { _, teamId in
            myIssues?.teamId = teamId
            drafts?.teamId = teamId
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
        // #EXP-1384: the page header; the board list's own stands down.
        .pageHeader(currentBoardName ?? Chats.Copy.Tabs.board)
        // A board picked in the board menu starts on All issues.
        .onChange(of: currentBoard) { _, _ in chip = .all }
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

    // MARK: - Board chips (EXP-1373)

    private var draftCount: Int { drafts?.rows.count ?? 0 }

    /// Drafts only while there are drafts; a stale Drafts pick falls back.
    private var chips: [Chats.Shell.BoardChip] {
        Chats.Shell.boardChips.filter { $0 != .drafts || draftCount > 0 }
    }

    private var shownChip: Chats.Shell.BoardChip {
        chips.contains(chip) ? chip : .all
    }

    private func chipLabel(_ chip: Chats.Shell.BoardChip) -> String {
        switch chip {
        case .all: chip.label
        case .mine: myIssues.map { "\(chip.label) \($0.count)" } ?? chip.label
        case .drafts: "\(chip.label) \(draftCount)"
        }
    }

    private var boardChips: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(chips, id: \.self) { option in
                    GlassPill(
                        chipLabel(option),
                        size: .md,
                        mode: .select(isSelected: shownChip == option) { chip = option }
                    )
                    .accessibilityIdentifier("\(Chats.TestIds.boardChip)-\(option.rawValue)")
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 6)
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

    // MARK: - Current board

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
            EmptyState(.noTeam, actionLabel: "Create or join a team") {
                showTeamSetup = true
            }
        } else {
            // EXP-1274 `boards`: the shared empty state with the
            // getting-started checklist under it — the same block the empty
            // BOARD renders, and the same one web and the IDE draw for a
            // board-less team.
            ScrollView {
                VStack(spacing: 24) {
                    EmptyState(.boards, actions: {
                        GlassPill(
                            EmptyStateCopy.State.boards.action ?? "",
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
                    })

                    if let accountId = deps.auth.activeAccountId, let teamId = checklistTeamId {
                        GettingStartedList(
                            progress: gettingStarted,
                            accountId: accountId,
                            teamId: teamId,
                            onCreateBoard: { Task { await beginCreateBoard() } }
                        )
                    }
                }
                .padding(.horizontal, 16)
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
    /// restarted by `.task(id:)` when the account switches). Tracks the
    /// boards shape's offset row too (EXP-1115): both feed the same
    /// empty-state decision, so they land in one emission.
    private func observeTeams() async {
        syncedTeams = nil
        boardsSynced = false
        guard let accountId = deps.auth.activeAccountId,
              let pool = try? deps.db.pool(forAccountId: accountId) else {
            // No pool means no shape will ever report live: fall through to
            // the empty state as before EXP-1115, never an endless spinner.
            boardsSynced = true
            return
        }
        let obs = ValueObservation.tracking { db -> ([TeamEntity], Bool) in
            let teams = try TeamEntity.fetchAll(db)
            let live = try ElectricOffset.fetchOne(db, key: "boards")?.isLive ?? false
            return (teams, live)
        }
        do {
            for try await (teams, live) in obs.values(in: pool) {
                await MainActor.run {
                    syncedTeams = teams
                    boardsSynced = live
                }
            }
        } catch {
            // Observation ended (pool closed on sign-out) — leave the last
            // snapshot in place; the .task(id:) restart handles account swaps.
        }
    }
}


