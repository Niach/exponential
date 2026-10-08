import ExpUI
import ExpCore
import SwiftUI

/// "Reviews" (EXP-131): the open PRs awaiting review across EVERY member team
/// (EXP-1186, cross-team like the Inbox), one row per distinct PR (a batch
/// coding run's issues collapse into a single row), grouped by board — the
/// board header names its team, quietly, once the caller is in several. Its own bottom-bar destination beside My Work (EXP-147 —
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
    @Environment(\.pushRoute) private var pushRoute
    @State private var viewModel: ReviewsViewModel?
    @State private var mergeTarget: ReviewEntry?
    /// EXP-1145: the row whose Merge hit a member of an open PR stack, with
    /// the dialog's content (`PrStack.stackMergeChoice`).
    @State private var stackTarget: StackMergeTarget?
    /// EXP-897 Part 4: the batch PR whose issues the overlay lists.
    @State private var batchTarget: ReviewEntry?
    /// EXP-734: the agent run whose OWN pull request a merge confirm is
    /// pending for — its own alert, because the copy names no issues.
    @State private var runMergeTarget: RunReviewEntry?
    /// EXP-1244: the unlinked pull request whose merge confirm is pending.
    @State private var pullMergeTarget: PullMergeTarget?
    /// Merge failures keyed by `ReviewEntry.id` — rendered INLINE under the
    /// failing row (EXP-323). An alert made the reason modal. EXP-1233: a REAL
    /// content conflict (EXP-533) never lands here — it opens the recovery
    /// run's composer instead; only the refusals no rebase can fix caption.
    @State private var mergeErrors: [String: MergeFailure] = [:]
    @State private var merging: Set<String> = []

    // The conflict recovery run (EXP-323, desktop parity). EXP-825: it is
    // NAVIGATION into the Agent page composer, seeded with the builtin and
    // this row's pull request. EXP-1233: a merge refused by a REAL conflict
    // opens it at once (no "Fix conflicts" swap in the row's slot, no
    // caption); the context menu keeps it as a manual entry.
    @State private var steerEnabled = false
    /// EXP-1244: bumped on every appear so the openPulls fetch re-runs.
    @State private var appearTick = 0

    var body: some View {
        // EXP-734: agent runs parking their OWN pull request belong to no
        // board, so they get their own section after the board groups —
        // one per team (EXP-1186); EXP-1244: the pull requests nothing links
        // follow as repository bands. `ReviewsQueue.build` decides it all.
        let snapshot = viewModel?.snapshot(teams: teamState.teams) ?? ReviewsSnapshot()
        Group {
            if viewModel == nil {
                Color.clear
            } else if snapshot.isEmpty {
                emptyState
            } else {
                reviewList(snapshot)
            }
        }
        // EXP-1244: one openPulls fetch per team, again whenever the team
        // set changes (and on every appear: the id includes the appear tick).
        .task(id: PullsFetchKey(teamIds: teamState.teams.map(\.id).sorted(), tick: appearTick)) {
            guard let viewModel else { return }
            await viewModel.refreshPulls(teamIds: teamState.teams.map(\.id))
        }
        .task(id: accountId) {
            let config = await SteerConfigCache.load(accountId: accountId, api: deps.steerApi)
            steerEnabled = config.enabled
        }
        .onAppear {
            if viewModel == nil {
                viewModel = ReviewsViewModel(
                    accountId: accountId, db: deps.db, openPulls: deps.openPulls,
                    repositoriesApi: deps.repositoriesApi
                )
            }
            appearTick += 1
            // Re-arm on every appear: pushing an issue detail stops the
            // observation (onDisappear), popping back must resume it.
            viewModel?.startObserving()
        }
        .onDisappear {
            viewModel?.stopObserving()
        }
        // EXP-1215: the app's own alert card (`GlassAlert`), ×4. Each
        // presentation (the three cards, the batch sheet) on a zero-size
        // node of its own (EXP-240): stacked on one node SwiftUI drops the
        // second.
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(item: $mergeTarget) { entry in
                    GlassAlert(
                        prompt: Prompts.MergeIssuePr.copy(number: entry.prNumber, issueCount: entry.issues.count),
                        handlers: ["merge": { merge(entry) }]
                    )
                }
        }
        // EXP-1145: a member of an open PR stack asks first. The list stays
        // FLAT; only the dialog knows the stack. Copy byte-locked by the
        // `stack-merge-choice.json` fixture.
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(item: $stackTarget) { target in
                    GlassAlert(
                        title: PrStack.stackMergeChoiceTitle,
                        message: target.choice.body,
                        actions: [
                            GlassAlertAction(PrStack.stackMergeCancelLabel, role: .outline, isCancel: true, id: "cancel") {},
                            GlassAlertAction(PrStack.mergeThisPrLabel, role: .outline, id: "merge-this") {
                                // The bottom merges plainly, any other member lands
                                // the chain bottom-up THROUGH itself.
                                merge(
                                    target.entry,
                                    issueId: target.entry.representative.id,
                                    mergeStack: target.choice.mergeThisUsesStack
                                )
                            },
                            GlassAlertAction(PrStack.mergeStackLabel, role: .primary, id: "merge-stack") {
                                merge(target.entry, issueId: target.choice.topIssueId, mergeStack: true)
                            },
                        ]
                    )
                }
        }
        // EXP-734: a run's own pull request links no issue, so it confirms
        // with its own copy.
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(item: $runMergeTarget) { entry in
                    GlassAlert(
                        prompt: Prompts.MergeRunPr.copy(number: entry.prNumber),
                        handlers: ["merge": { merge(run: entry) }]
                    )
                }
        }
        // EXP-1244: a pull request nothing links merges straight through the
        // repository (`repositories.mergePull`).
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(item: $pullMergeTarget) { target in
                    GlassAlert(
                        prompt: Prompts.MergeExternalPr.copy(
                            repository: target.fullName,
                            number: target.pull.number,
                            base: target.pull.baseBranch
                        ),
                        handlers: ["merge": { merge(pull: target) }]
                    )
                }
        }
        // EXP-897 Part 4: a batch row's issues are the overlay's content.
        .sheet(item: $batchTarget) { entry in
            CoveredIssuesSheet(
                title: CoveredIssuesSheet.pullRequestTitle,
                issues: entry.issues
            ) { issueId in
                batchTarget = nil
                deps.deepLinkBus.navigateToIssue(issueId)
            }
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
    private func reviewList(_ snapshot: ReviewsSnapshot) -> some View {
        let groups = snapshot.groups
        let runs = snapshot.runs
        // EXP-1186: the team name rides the headers only when there is more
        // than one team to tell apart.
        let multiTeam = TeamGroups.isMultiTeam(teamState.teams)
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
                    boardHeader(
                        board: group.board,
                        teamName: multiTeam ? group.team.name : nil,
                        count: group.entries.count
                    )
                        .listRowInsets(EdgeInsets(top: 6, leading: 16, bottom: 2, trailing: 16))
                        .listRowBackground(Color.clear)
                }
            }

            // EXP-734: the runs' own pull requests, last — they belong to no
            // board, so they cannot ride a board section. EXP-1186: one
            // section per team.
            ForEach(runs) { group in
                Section {
                    ForEach(group.items) { entry in
                        runEntryRow(entry)
                            .listRowBackground(Color.clear)
                            .listRowSeparator(.hidden)
                            .listRowInsets(EdgeInsets(top: 1.5, leading: 16, bottom: 1.5, trailing: 16))
                    }
                } header: {
                    runsHeader(
                        teamName: multiTeam ? group.team.name : nil,
                        count: group.items.count
                    )
                        .listRowInsets(EdgeInsets(top: 6, leading: 16, bottom: 2, trailing: 16))
                        .listRowBackground(Color.clear)
                }
            }

            // EXP-1244: the open pull requests nothing links, one band per
            // team repository, after the runs.
            ForEach(snapshot.repos) { repo in
                Section {
                    ForEach(repo.pulls) { pull in
                        pullRow(pull, repo: repo)
                            .listRowBackground(Color.clear)
                            .listRowSeparator(.hidden)
                            .listRowInsets(EdgeInsets(top: 1.5, leading: 16, bottom: 1.5, trailing: 16))
                    }
                } header: {
                    repoHeader(
                        fullName: repo.fullName,
                        caption: multiTeam ? repo.team.name : ReviewsQueue.repoBandCaption,
                        count: repo.pulls.count
                    )
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
    private func boardHeader(board: BoardEntity, teamName: String?, count: Int) -> some View {
        HStack(spacing: 8) {
            // Board glyph tinted with the board color — same idiom as the
            // board switcher sheet, scaled down for a section header (EXP-449).
            AppIcon(BoardTypeDisplay.iconName(for: board), size: 13)
                .foregroundStyle(Color(hex: board.color ?? "#888888") ?? .gray)

            Text(board.name)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .lineLimit(1)

            teamCaption(teamName)

            Text("\(count)")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))

            Spacer()
        }
        .padding(.vertical, 6)
        .padding(.horizontal, 8)
        .textCase(nil)
    }

    /// EXP-734: the "Agent runs" section header — same shape as a board
    /// header, with the Actions glyph instead of a board's.
    @ViewBuilder
    private func runsHeader(teamName: String?, count: Int) -> some View {
        HStack(spacing: 8) {
            AppIcon(AppIcons.navActions, size: 13)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))

            Text("Agent runs")
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))

            teamCaption(teamName ?? ReviewsQueue.runBandCaption)

            Text("\(count)")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))

            Spacer()
        }
        .padding(.vertical, 6)
        .padding(.horizontal, 8)
        .textCase(nil)
    }

    /// EXP-1244: a repository band's header — the board header's shape with
    /// the PR-open glyph (web parity); the caption names the team on a multi-team
    /// list, else says the pull requests are not linked.
    @ViewBuilder
    private func repoHeader(fullName: String, caption: String, count: Int) -> some View {
        HStack(spacing: 8) {
            AppIcon(AppIcons.prOpen, size: 13)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))

            Text(fullName)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .lineLimit(1)

            teamCaption(caption)

            Text("\(count)")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))

            Spacer()
        }
        .padding(.vertical, 6)
        .padding(.horizontal, 8)
        .textCase(nil)
    }

    /// EXP-1186: the header's quiet team name — muted secondary text after
    /// the board (or "Agent runs"), only when the caller is in several teams.
    @ViewBuilder
    private func teamCaption(_ teamName: String?) -> some View {
        if let teamName {
            Text(teamName)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .lineLimit(1)
        }
    }

    /// EXP-734: one agent run's own pull request. EXP-1194: the row opens
    /// OUR review of it — the Changes face fed by `codingSessions.prFiles`
    /// (`AppRoute.runChanges`), like an issue row opens its issue's; Merge
    /// lives on the pill, the swipe and the context menu (with Open on GitHub),
    /// like the issue rows'.
    @ViewBuilder
    private func runEntryRow(_ entry: RunReviewEntry) -> some View {
        let key = runKey(entry)
        // The caption lives OUTSIDE the NavigationLink, like `entryRow`'s.
        VStack(alignment: .leading, spacing: 6) {
            NavigationLink(value: AppRoute.runChanges(
                accountId: accountId, sessionId: entry.session.id
            )) {
                HStack(alignment: .center, spacing: 10) {
                    AppIcon(AppIcons.prOpen, size: AppIcon.Size.small)
                        .foregroundStyle(IssueStatus.inReview.color)
                        .frame(width: 16)

                    VStack(alignment: .leading, spacing: 3) {
                        HStack(spacing: 6) {
                            if let prNumber = entry.prNumber {
                                Text("#\(prNumber)")
                                    .font(.caption.monospaced())
                                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            }
                            Text(entry.title)
                                .font(.subheadline)
                                .foregroundStyle(.white)
                                .lineLimit(1)
                        }

                        if let branch = entry.branch, !branch.isEmpty {
                            Text(branch)
                                .font(.caption.monospaced())
                                .lineLimit(1)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        }
                    }

                    Spacer(minLength: 8)

                    GlassPill("Merge") {
                        if merging.contains(key) {
                            ProgressView().controlSize(.mini)
                        } else {
                            AppIcon(AppIcons.prMerged, size: GlassPillTokens.glyphSm)
                        }
                    }
                    .contentShape(Capsule())
                    .onTapGesture {
                        guard !merging.contains(key) else { return }
                        runMergeTarget = entry
                    }
                    .accessibilityAddTraits(.isButton)
                    .accessibilityLabel("Merge pull request")
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
                .glassRow()
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .swipeActions(edge: .trailing) {
                Button { runMergeTarget = entry } label: {
                    Label("Merge", appIcon: AppIcons.prMerged)
                }
                .tint(DesignTokens.Semantic.green)
            }
            .contextMenu {
                Button {
                    runMergeTarget = entry
                } label: {
                    Label(DomainContract.diffUiMergePr, appIcon: AppIcons.prMerged)
                }
                if let url = entry.prUrl.flatMap(URL.init(string:)) {
                    Button {
                        openURL(url)
                    } label: {
                        Label(DomainContract.diffUiOpenOnGithub, appIcon: AppIcons.uiGithub)
                    }
                }
            }

            if let failure = mergeErrors[key] {
                Text(failure.message)
                    .font(.caption)
                    .foregroundStyle(DesignTokens.Semantic.red)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .glassCard()
            }
        }
    }

    /// EXP-1244: one open pull request nothing links — the run row's shape;
    /// a tap opens it on GitHub (there is no issue or run to open), Merge
    /// confirms and lands it through `repositories.mergePull`. A draft cannot
    /// merge.
    @ViewBuilder
    private func pullRow(_ pull: OpenPull, repo: RepoReviewGroup) -> some View {
        let key = pullKey(repositoryId: repo.repositoryId, number: pull.number)
        let target = PullMergeTarget(
            repositoryId: repo.repositoryId, fullName: repo.fullName, pull: pull
        )
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .center, spacing: 10) {
                AppIcon(AppIcons.prOpen, size: AppIcon.Size.small)
                    .foregroundStyle(IssueStatus.inReview.color)
                    .frame(width: 16)

                VStack(alignment: .leading, spacing: 3) {
                    HStack(spacing: 6) {
                        Text("#\(pull.number)")
                            .font(.caption.monospaced())
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        Text(pull.title)
                            .font(.subheadline)
                            .foregroundStyle(.white)
                            .lineLimit(1)
                        if pull.draft {
                            GlassPill("Draft")
                        }
                    }

                    if !pull.branch.isEmpty {
                        Text(pull.branch)
                            .font(.caption.monospaced())
                            .lineLimit(1)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                }

                Spacer(minLength: 8)

                GlassPill("Merge", enabled: !pull.draft && !merging.contains(key)) {
                    if merging.contains(key) {
                        ProgressView().controlSize(.mini)
                    } else {
                        AppIcon(AppIcons.prMerged, size: GlassPillTokens.glyphSm)
                    }
                }
                .contentShape(Capsule())
                .onTapGesture {
                    guard !pull.draft, !merging.contains(key) else { return }
                    pullMergeTarget = target
                }
                .accessibilityAddTraits(.isButton)
                .accessibilityLabel("Merge pull request")
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
            .contentShape(Rectangle())
            .onTapGesture {
                if let url = URL(string: pull.url) { openURL(url) }
            }
            .accessibilityAddTraits(.isButton)
            .contextMenu {
                if !pull.draft {
                    Button {
                        pullMergeTarget = target
                    } label: {
                        Label(DomainContract.diffUiMergePr, appIcon: AppIcons.prMerged)
                    }
                }
                if let url = URL(string: pull.url) {
                    Button {
                        openURL(url)
                    } label: {
                        Label(DomainContract.diffUiOpenOnGithub, appIcon: AppIcons.uiGithub)
                    }
                }
            }

            if let failure = mergeErrors[key] {
                Text(failure.message)
                    .font(.caption)
                    .foregroundStyle(DesignTokens.Semantic.red)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .glassCard()
            }
        }
    }

    private func pullKey(repositoryId: String, number: Int) -> String {
        "pull:\(repositoryId):\(number)"
    }

    /// EXP-1244: merging an unlinked pull request. Nothing syncs it away, so
    /// a success drops the row locally; a refusal captions it.
    private func merge(pull target: PullMergeTarget) {
        pullMergeTarget = nil
        let key = pullKey(repositoryId: target.repositoryId, number: target.pull.number)
        mergeErrors[key] = nil
        merging.insert(key)
        Task {
            do {
                try await deps.repositoriesApi.mergePull(
                    accountId: accountId,
                    repositoryId: target.repositoryId,
                    prNumber: target.pull.number
                )
                viewModel?.dropPull(repositoryId: target.repositoryId, number: target.pull.number)
            } catch {
                mergeErrors[key] = MergeFailure(error: error)
            }
            merging.remove(key)
        }
    }

    /// Merge/error state shares one keyspace with the issue entries, so a run
    /// key is namespaced.
    private func runKey(_ entry: RunReviewEntry) -> String { "run:\(entry.id)" }

    /// EXP-734: merging a run's OWN pull request. No local surgery: the server
    /// flips the session row's `pr_state` (and ends the run), and both land
    /// here through sync.
    private func merge(run entry: RunReviewEntry) {
        runMergeTarget = nil
        let key = runKey(entry)
        let sessionId = entry.session.id
        mergeErrors[key] = nil
        merging.insert(key)
        Task {
            do {
                try await deps.codingSessionsApi.mergePr(
                    accountId: accountId, sessionId: sessionId
                )
            } catch {
                mergeErrors[key] = MergeFailure(error: error)
            }
            merging.remove(key)
        }
    }

    private func entryRow(_ entry: ReviewEntry) -> some View {
        // The caption + recovery button live OUTSIDE the NavigationLink: a
        // control inside the link's label has its tap swallowed by the link.
        VStack(alignment: .leading, spacing: 6) {
            entryLink(entry)
            batchCaption(entry)
            if let failure = mergeErrors[entry.id] {
                mergeErrorCaption(entry, failure: failure)
            }
        }
    }

    /// EXP-897 Part 4: a batch row's issue count, which opens the overlay —
    /// OUTSIDE the link, so the control actually receives its tap.
    @ViewBuilder
    private func batchCaption(_ entry: ReviewEntry) -> some View {
        if entry.isBatch {
            HStack(spacing: 8) {
                GlassPill("\(entry.issues.count) issues", icon: AppIcons.prBatch)
                    .contentShape(Capsule())
                    .onTapGesture { batchTarget = entry }
                    .accessibilityAddTraits(.isButton)
                    .accessibilityLabel("Show the issues on this pull request")
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 12)
        }
    }

    @ViewBuilder
    private func entryLink(_ entry: ReviewEntry) -> some View {
        // The diff is what a reviewer wants first (EXP-168). EXP-1154: it is
        // the issue's own Work screen on its Changes face (the review page
        // is gone); Merge rides its bar, Close PR its `…` menu.
        NavigationLink(value: AppRoute.issueFace(
            accountId: accountId, id: entry.representative.id, face: .changes
        )) {
            // SLOP-16 r3: the row CONTENT is shared with the Related work
            // sheet (`PrReviewRowContent`); only the card around it is ours.
            PrReviewRowContent(issues: entry.issues) {
                mergeSlot(entry)
            }
            .glassRow()
        }
        .buttonStyle(.plain)
        .swipeActions(edge: .trailing) {
            Button { requestMerge(entry) } label: {
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
                requestMerge(entry)
            } label: {
                Label(DomainContract.diffUiMergePr, appIcon: AppIcons.prMerged)
            }
            if entry.isBatch {
                Button {
                    batchTarget = entry
                } label: {
                    Label("Show the issues", appIcon: AppIcons.prBatch)
                }
            }
            if canOpenFixConflicts(entry) {
                Button {
                    fixConflicts(entry, conflict: false)
                } label: {
                    Label("Fix merge conflicts", appIcon: AppIcons.uiBranch)
                }
            }
            if let url = prURL(entry) {
                Button {
                    openURL(url)
                } label: {
                    Label(DomainContract.diffUiOpenOnGithub, appIcon: AppIcons.uiGithub)
                }
            }
        }
    }

    /// The row's ONE trailing control, Reviews-only — the shared content
    /// hosts it in its trailing slot. Inline merge — same confirm-gated flow
    /// as the swipe action (EXP-248: uniform with the web/Android review
    /// rows). NOT a Button: nested in a NavigationLink label the link
    /// swallows its tap and only pushes the detail, so this uses the same
    /// contentShape + onTapGesture pattern as IssueListView's inline
    /// status/priority glyphs.
    ///
    /// EXP-1233: always Merge — a REAL conflict opens the recovery run's
    /// composer at once instead of swapping this slot (EXP-706's swap is
    /// gone), so a conflict resolved elsewhere is one tap away.
    private func mergeSlot(_ entry: ReviewEntry) -> some View {
        GlassPill("Merge") {
            mergeGlyph(entry, icon: AppIcons.prMerged)
        }
        .contentShape(Capsule())
        .onTapGesture {
            guard !merging.contains(entry.id) else { return }
            requestMerge(entry)
        }
        .accessibilityAddTraits(.isButton)
        .accessibilityLabel("Merge pull request")
    }

    /// The merge pill's glyph: a spinner while that row's merge is in flight.
    @ViewBuilder
    private func mergeGlyph(_ entry: ReviewEntry, icon: String) -> some View {
        if merging.contains(entry.id) {
            ProgressView().controlSize(.mini)
        } else {
            AppIcon(icon, size: GlassPillTokens.glyphSm)
        }
    }

    /// A refused merge (branch protection, a stale base, GitHub App errors)
    /// captions THIS row (EXP-323) — never a modal alert, and never anything
    /// the tab bar can cover. The caption is the REASON only. EXP-1233: a
    /// real conflict opens the recovery composer instead (no caption).
    @ViewBuilder
    private func mergeErrorCaption(_ entry: ReviewEntry, failure: MergeFailure) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(failure.message)
                .font(.caption)
                .foregroundStyle(DesignTokens.Semantic.red)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        // A bordered card, not a group container: the caption is one free
        // block that has to read as attached-but-separate from its row.
        .glassCard()
    }

    /// The recovery run rebases the PR's branch, so it needs one recorded —
    /// the same guard the desktop applies on its Reviews rows — and the relay
    /// to start it. The context menu's manual entry needs only this.
    private func canOpenFixConflicts(_ entry: ReviewEntry) -> Bool {
        steerEnabled && !(entry.branch ?? "").isEmpty
    }

    /// EXP-533/EXP-1233: a refusal OPENS the recovery run only for a REAL
    /// conflict (the server's `CONFLICT` / HTTP 409); a stale head, branch
    /// protection or an unreachable server refused the merge for a reason no
    /// rebase can fix.
    private func refusalOpensFixConflicts(_ entry: ReviewEntry, _ failure: MergeFailure) -> Bool {
        failure.isConflict && canOpenFixConflicts(entry)
    }

    private func prURL(_ entry: ReviewEntry) -> URL? {
        guard let prUrl = entry.prUrl else { return nil }
        return URL(string: prUrl)
    }

    /// EXP-1145: a member of an open PR stack opens the stack dialog, any
    /// other row the plain confirm.
    private func requestMerge(_ entry: ReviewEntry) {
        if let choice = PrStack.stackMergeChoice(
            entry.representative, issues: viewModel?.stackPool(for: entry.representative) ?? []
        ) {
            stackTarget = StackMergeTarget(entry: entry, choice: choice)
        } else {
            mergeTarget = entry
        }
    }

    private func merge(_ entry: ReviewEntry) {
        merge(entry, issueId: entry.representative.id, mergeStack: false)
    }

    /// The row's merge, plain or (EXP-1145) the stack merge through
    /// `issueId`; a refusal captions the row with the server's message,
    /// except a real conflict on a plain merge, which opens the recovery
    /// composer (EXP-1233).
    private func merge(_ entry: ReviewEntry, issueId: String, mergeStack: Bool) {
        mergeTarget = nil
        stackTarget = nil
        let key = entry.id
        mergeErrors[key] = nil
        merging.insert(key)
        Task {
            do {
                try await deps.issuesApi.mergePr(
                    accountId: accountId, issueId: issueId, mergeStack: mergeStack ? true : nil
                )
            } catch {
                let failure = MergeFailure(error: error, stackMerge: mergeStack)
                // EXP-1233: a real conflict opens the recovery composer,
                // flagged, with no caption; a stack merge's never does (the
                // member that stopped the chain may not be this row's PR, and
                // `stackMerge` already strips its conflict flag).
                if !mergeStack, refusalOpensFixConflicts(entry, failure) {
                    fixConflicts(entry, conflict: true)
                } else {
                    mergeErrors[key] = failure
                }
            }
            merging.remove(key)
        }
    }

    /// EXP-825: the recovery run is the composer with the "Fix merge
    /// conflicts" builtin picked and this row's pull request pre-picked —
    /// ANY linked issue resolves (the picker normalises by membership).
    /// `conflict` = a refused merge opened it (EXP-1233: the card says why).
    private func fixConflicts(_ entry: ReviewEntry, conflict: Bool) {
        pushRoute(.agent(
            accountId: accountId,
            // EXP-1186: on the PR's OWN team — Reviews is cross-team.
            seed: AgentComposerSeed(
                actionId: DomainContract.builtinFixConflictsId,
                prIssueId: entry.representative.id,
                teamId: viewModel?.teamId(of: entry),
                conflict: conflict
            )
        ))
    }
}

/// EXP-1244: the unlinked pull request a merge confirm is pending for.
private struct PullMergeTarget: Identifiable {
    let repositoryId: String
    let fullName: String
    let pull: OpenPull
    var id: String { "\(repositoryId):\(pull.number)" }
}

/// EXP-1244: re-runs the openPulls fetch when the team set changes or the
/// list re-appears.
private struct PullsFetchKey: Equatable {
    let teamIds: [String]
    let tick: Int
}

/// EXP-1145: a Reviews row whose merge asks the stack question.
private struct StackMergeTarget {
    let entry: ReviewEntry
    let choice: PrStack.StackMergeChoice
}

/// SLOP-16 r3: THE pull-request row's content — the batch/PR glyph, the
/// identifier (a batch: `#n` + its issue count over the identifiers) and the
/// branch, then a trailing slot. Reviews wears it on a `.glassRow()` card
/// with its merge control.
struct PrReviewRowContent<Trailing: View>: View {
    /// The issues sharing the pull request, newest first; the first one
    /// represents it.
    let issues: [IssueEntity]
    @ViewBuilder let trailing: () -> Trailing

    private var representative: IssueEntity { issues[0] }
    private var isBatch: Bool { issues.count > 1 }
    private var identifiers: [String] { issues.compactMap(\.identifier) }

    var body: some View {
        HStack(alignment: .center, spacing: 10) {
            // PR glyph — the in_review status icon, green, vertically
            // centered like the Android row (EXP-248). EXP-897 Part 4: a
            // BATCH pull request wears the `pr-batch` glyph instead, so
            // one row for several issues reads as one at a glance.
            AppIcon(
                isBatch ? AppIcons.prBatch : AppIcons.prOpen,
                size: AppIcon.Size.small
            )
            .foregroundStyle(IssueStatus.inReview.color)
            .frame(width: 16)

            VStack(alignment: .leading, spacing: 3) {
                if isBatch {
                    HStack(spacing: 6) {
                        if let prNumber = representative.prNumber {
                            Text("#\(prNumber)")
                                .font(.caption.monospaced())
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        }
                        Text("\(issues.count) issues")
                            .font(.subheadline)
                            .foregroundStyle(.white)
                    }
                    if !identifiers.isEmpty {
                        Text(identifiers.joined(separator: ", "))
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .lineLimit(1)
                    }
                } else {
                    HStack(spacing: 6) {
                        if let identifier = representative.identifier {
                            Text(identifier)
                                .font(.caption.monospaced())
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        }
                        Text(representative.title)
                            .font(.subheadline)
                            .foregroundStyle(.white)
                            .lineLimit(1)
                    }
                }

                if let branch = representative.branch, !branch.isEmpty {
                    Text(branch)
                        .font(.caption.monospaced())
                        .lineLimit(1)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            }

            Spacer(minLength: 8)

            trailing()
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
    }
}
