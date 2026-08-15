import ExpUI
import ExpCore
import SwiftUI

/// "Reviews" (EXP-131): the active team's open PRs awaiting review, one row
/// per distinct PR (a batch coding run's issues collapse into a single row),
/// grouped by board. Its own bottom-bar destination beside My Work (EXP-147 —
/// it used to be a My Work segment).
struct ReviewsView: View {
    var body: some View {
        ZStack {
            AppBackground()
            ReviewsListContent()
        }
        .navigationTitle("Reviews")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
    }
}

/// The bare list — same glass row language as `MyIssuesListContent`, no chrome
/// of its own.
struct ReviewsListContent: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(TeamState.self) private var teamState
    @Environment(\.openURL) private var openURL
    @State private var viewModel: ReviewsViewModel?
    @State private var mergeTarget: ReviewEntry?
    /// Merge failures keyed by `ReviewEntry.id` — rendered INLINE under the
    /// failing row (EXP-323). An alert made the reason modal and gave the
    /// conflict-recovery run nowhere to live.
    @State private var mergeErrors: [String: String] = [:]
    @State private var merging: Set<String> = []

    // "Fix conflicts" (EXP-323, desktop parity): a failed merge is usually a
    // conflict, so the row offers the builtin run on any machine the caller can
    // reach — own or shared with the team (EXP-432).
    @State private var fixTarget: ReviewEntry?
    @State private var steerEnabled = false
    @State private var devices: [SteerDevice]?
    @State private var startCandidates: [StartCodingSheet.IssueOption] = []
    @State private var runCaption: String?
    @State private var runError: String?

    var body: some View {
        let groups = viewModel?.groups(teamId: teamState.activeTeam?.id) ?? []
        Group {
            if viewModel == nil {
                Color.clear
            } else if groups.isEmpty {
                emptyState
            } else {
                reviewList(groups)
            }
        }
        .task(id: accountId) {
            let config = await SteerConfigCache.load(accountId: accountId, api: deps.steerApi)
            steerEnabled = config.enabled
            await refreshDevices()
        }
        .onAppear {
            if viewModel == nil {
                viewModel = ReviewsViewModel(accountId: accountId, db: deps.db)
            }
            // Re-arm on every appear: pushing an issue detail stops the
            // observation (onDisappear), popping back must resume it.
            viewModel?.startObserving()
            // Refresh presence on every appear (the .task doesn't re-run on
            // pop-back). A no-op until steering resolves enabled.
            Task { await refreshDevices() }
        }
        .onDisappear {
            viewModel?.stopObserving()
        }
        .sheet(item: $fixTarget) { entry in
            StartCodingSheet(
                devices: devices ?? [],
                issues: startCandidates,
                preselectedIds: [],
                teamId: teamState.activeTeam?.id,
                initialTab: .actions,
                preselectedActionId: DomainContract.builtinFixConflictsId,
                preselectedPrIssueId: entry.representative.id,
                onStart: { device, issueIds, options in
                    start(on: device, issueIds: issueIds, options: options)
                },
                onRunAction: { device, action, options, inputs in
                    runAction(on: device, action: action, options: options, inputs: inputs)
                }
            )
        }
        .alert(
            "Merge pull request?",
            isPresented: Binding(
                get: { mergeTarget != nil },
                set: { if !$0 { mergeTarget = nil } }
            ),
            presenting: mergeTarget
        ) { entry in
            Button("Merge") { merge(entry) }
            Button("Cancel", role: .cancel) { mergeTarget = nil }
        } message: { entry in
            Text(mergeMessage(entry))
        }
    }

    private var emptyState: some View {
        VStack(spacing: 12) {
            Spacer()
            AppIcon(AppIcons.prOpen, size: 22)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text("No open pull requests")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Spacer()
        }
        .frame(maxWidth: .infinity)
    }

    @ViewBuilder
    private func reviewList(_ groups: [ReviewGroup]) -> some View {
        List {
            ForEach(groups) { group in
                Section {
                    ForEach(group.entries) { entry in
                        entryRow(entry)
                            .listRowBackground(Color.clear)
                            .listRowSeparator(.hidden)
                            .listRowInsets(EdgeInsets(top: 1.5, leading: 16, bottom: 1.5, trailing: 16))
                    }
                } header: {
                    boardHeader(board: group.board, count: group.entries.count)
                        .listRowInsets(EdgeInsets(top: 6, leading: 16, bottom: 2, trailing: 16))
                        .listRowBackground(Color.clear)
                }
            }
        }
        .listStyle(.plain)
        // Same compact-list treatment as MyIssuesListContent (EXP-80).
        .contentMargins(.horizontal, 0, for: .scrollContent)
        .contentMargins(.top, 0, for: .scrollContent)
        .environment(\.defaultMinListRowHeight, 0)
        .listSectionSpacing(0)
        .scrollContentBackground(.hidden)
        .background(Color.clear)
        .tabBarBottomInset()
    }

    @ViewBuilder
    private func boardHeader(board: BoardEntity, count: Int) -> some View {
        HStack(spacing: 8) {
            // Board glyph tinted with the board color — same idiom as the
            // board switcher sheet, scaled down for a section header (EXP-449).
            AppIcon(BoardTypeDisplay.iconName(for: board), size: 13)
                .foregroundStyle(Color(hex: board.color ?? "#888888") ?? .gray)

            Text(board.name)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))

            Text("\(count)")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))

            Spacer()
        }
        .padding(.vertical, 6)
        .padding(.horizontal, 8)
        .textCase(nil)
    }

    @ViewBuilder
    private func entryRow(_ entry: ReviewEntry) -> some View {
        // The caption + recovery button live OUTSIDE the NavigationLink: a
        // control inside the link's label has its tap swallowed by the link.
        VStack(alignment: .leading, spacing: 6) {
            entryLink(entry)
            if let message = mergeErrors[entry.id] {
                mergeErrorCaption(entry, message: message)
            }
        }
    }

    @ViewBuilder
    private func entryLink(_ entry: ReviewEntry) -> some View {
        // The Review detail (the diff + Merge/Close screen) is what a reviewer
        // wants first (EXP-168); the issue itself is one tap away in the menu.
        NavigationLink(value: AppRoute.changes(accountId: accountId, issueId: entry.representative.id)) {
            HStack(alignment: .center, spacing: 10) {
                // PR glyph — the in_review status icon, green, vertically
                // centered like the Android row (EXP-248).
                AppIcon(AppIcons.prOpen, size: AppIcon.Size.small)
                    .foregroundStyle(IssueStatus.inReview.color)
                    .frame(width: 16)

                VStack(alignment: .leading, spacing: 3) {
                    if entry.isBatch {
                        HStack(spacing: 6) {
                            if let prNumber = entry.prNumber {
                                Text("#\(prNumber)")
                                    .font(.caption.monospaced())
                                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            }
                            Text("\(entry.issues.count) issues")
                                .font(.subheadline)
                                .foregroundStyle(.white)
                        }
                        if !entry.identifiers.isEmpty {
                            Text(entry.identifiers.joined(separator: ", "))
                                .font(.caption)
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                .lineLimit(1)
                        }
                    } else {
                        HStack(spacing: 6) {
                            if let identifier = entry.representative.identifier {
                                Text(identifier)
                                    .font(.caption.monospaced())
                                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            }
                            Text(entry.representative.title)
                                .font(.subheadline)
                                .foregroundStyle(.white)
                                .lineLimit(1)
                        }
                    }

                    if let branch = entry.branch, !branch.isEmpty {
                        Text(branch)
                            .font(.caption.monospaced())
                            .lineLimit(1)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                }

                Spacer(minLength: 8)

                // Inline merge — same confirm-gated flow as the swipe action
                // (EXP-248: uniform with the web/Android review rows). NOT a
                // Button: nested in a NavigationLink label the link swallows
                // its tap and only pushes the detail, so this uses the same
                // contentShape + onTapGesture pattern as IssueListView's
                // inline status/priority glyphs.
                HStack(spacing: 4) {
                    if merging.contains(entry.id) {
                        ProgressView().controlSize(.mini)
                    } else {
                        AppIcon(AppIcons.prMerged, size: 11)
                    }
                    Text("Merge")
                        .font(.caption.weight(.medium))
                }
                .foregroundStyle(.white)
                .padding(.horizontal, 10)
                .padding(.vertical, 6)
                .background(.white.opacity(0.08), in: Capsule())
                .overlay(Capsule().stroke(.white.opacity(0.12), lineWidth: 0.5))
                .contentShape(Capsule())
                .onTapGesture {
                    guard !merging.contains(entry.id) else { return }
                    mergeTarget = entry
                }
                .accessibilityAddTraits(.isButton)
                .accessibilityLabel("Merge pull request")
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
        }
        .buttonStyle(.plain)
        .swipeActions(edge: .trailing) {
            Button { mergeTarget = entry } label: {
                Label("Merge", appIcon: AppIcons.prMerged)
            }
            .tint(DesignTokens.Semantic.green)
        }
        .contextMenu {
            Button {
                deps.deepLinkBus.navigateToIssue(entry.representative.id)
            } label: {
                Label("Open issue", appIcon: AppIcons.uiIssue)
            }
            Button {
                mergeTarget = entry
            } label: {
                Label("Merge PR", appIcon: AppIcons.prMerged)
            }
            if canFixConflicts(entry) {
                Button {
                    fixTarget = entry
                } label: {
                    Label("Fix merge conflicts", appIcon: AppIcons.uiBranch)
                }
            }
            if let url = prURL(entry) {
                Button {
                    openURL(url)
                } label: {
                    Label("Open PR on GitHub", appIcon: AppIcons.uiExternalLink)
                }
            }
        }
    }

    /// A refused merge (conflicts, branch protection, GitHub App errors)
    /// captions THIS row (EXP-323) — never a modal alert, and never anything
    /// the tab bar can cover. A conflict is the common case, so the builtin
    /// recovery run sits right next to the reason.
    @ViewBuilder
    private func mergeErrorCaption(_ entry: ReviewEntry, message: String) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(message)
                .font(.caption)
                .foregroundStyle(DesignTokens.Semantic.red)
                .fixedSize(horizontal: false, vertical: true)

            if canFixConflicts(entry) {
                Button {
                    fixTarget = entry
                } label: {
                    HStack(spacing: 4) {
                        AppIcon(AppIcons.uiBranch, size: 11)
                        Text("Fix conflicts")
                            .font(.caption.weight(.medium))
                    }
                    .foregroundStyle(.white)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 6)
                    .background(.white.opacity(0.08), in: Capsule())
                    .overlay(Capsule().stroke(.white.opacity(0.12), lineWidth: 0.5))
                }
                .buttonStyle(.plain)
            }

            if let runCaption {
                Text(runCaption)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
            if let runError {
                Text(runError)
                    .font(.caption)
                    .foregroundStyle(DesignTokens.Semantic.red)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .glassSection()
    }

    /// The recovery run rebases the PR's branch, so it needs one recorded —
    /// the same guard the desktop applies on its Reviews rows.
    private func canFixConflicts(_ entry: ReviewEntry) -> Bool {
        steerEnabled &&
            mergeErrors[entry.id] != nil &&
            !(entry.branch ?? "").isEmpty
    }

    private func prURL(_ entry: ReviewEntry) -> URL? {
        guard let prUrl = entry.prUrl else { return nil }
        return URL(string: prUrl)
    }

    private func mergeMessage(_ entry: ReviewEntry) -> String {
        let pr = entry.prNumber.map { "#\($0)" } ?? "this pull request"
        var message = "Squash-merges PR \(pr) via the GitHub App. Any live coding session for it closes."
        if entry.isBatch {
            message += " Completes all \(entry.issues.count) linked issues."
        }
        return message
    }

    private func merge(_ entry: ReviewEntry) {
        mergeTarget = nil
        let issueId = entry.representative.id
        let key = entry.id
        mergeErrors[key] = nil
        merging.insert(key)
        Task {
            do {
                try await deps.issuesApi.mergePr(accountId: accountId, issueId: issueId)
            } catch {
                mergeErrors[key] = error.localizedDescription
            }
            merging.remove(key)
        }
    }

    private func refreshDevices() async {
        guard steerEnabled else {
            devices = nil
            return
        }
        // EXP-432: team-scoped, so a teammate's shared server can host the
        // run. EXP-481: read off the synced devices shape, not the network.
        devices = await DeviceQueries.onlineStartTargets(
            db: deps.db, accountId: accountId,
            teamId: teamState.activeTeam?.id, userId: deps.auth.userId
        )
        startCandidates = await StartCodingSheet.IssueOption.loadCandidates(
            db: deps.db,
            accountId: accountId,
            teamId: teamState.activeTeam?.id
        )
    }

    /// Actions-mode launch from the unified sheet (EXP-323 — the "Fix merge
    /// conflicts" builtin, which always rides its teamId). The run surfaces in
    /// the Agents list via sync like any other session.
    private func runAction(
        on device: SteerDevice,
        action: ActionDto,
        options: SteerStartOptions,
        inputs: [String: String]
    ) {
        runCaption = nil
        runError = nil
        let label = device.deviceLabel
        Task {
            do {
                try await deps.steerApi.startSession(
                    accountId: accountId,
                    actionId: action.id,
                    deviceId: device.deviceId,
                    teamId: action.isBuiltin ? action.teamId : nil,
                    options: options,
                    inputs: inputs.isEmpty ? nil : inputs
                )
                runCaption = "Run sent to \(label). It'll appear under Agents when it spins up."
                Task {
                    try? await Task.sleep(for: .seconds(30))
                    runCaption = nil
                }
            } catch {
                runError = error.localizedDescription
            }
        }
    }

    /// Issues-tab launch from the same sheet (flipping tabs must not dead-end).
    private func start(on device: SteerDevice, issueIds: [String], options: SteerStartOptions) {
        guard !issueIds.isEmpty else { return }
        runCaption = nil
        runError = nil
        let label = device.deviceLabel
        Task {
            do {
                if issueIds.count > 1 {
                    try await deps.steerApi.startSession(
                        accountId: accountId,
                        issueIds: issueIds,
                        deviceId: device.deviceId,
                        options: options
                    )
                } else {
                    try await deps.steerApi.startSession(
                        accountId: accountId,
                        issueId: issueIds[0],
                        deviceId: device.deviceId,
                        options: options
                    )
                }
                runCaption = "Start sent to \(label). It'll appear under Agents when it spins up."
                Task {
                    try? await Task.sleep(for: .seconds(30))
                    runCaption = nil
                }
            } catch {
                runError = error.localizedDescription
            }
        }
    }
}
