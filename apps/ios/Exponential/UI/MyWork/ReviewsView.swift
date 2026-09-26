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
    @Environment(\.pushRoute) private var pushRoute
    @State private var viewModel: ReviewsViewModel?
    @State private var mergeTarget: ReviewEntry?
    /// EXP-897: the stack a "Merge the whole stack?" confirm is pending for —
    /// always the LOWEST row, whose issue id the server walks the chain from.
    @State private var stackMergeTarget: ReviewRow?
    /// EXP-897 Part 4: the batch PR whose issues the overlay lists.
    @State private var batchTarget: ReviewEntry?
    /// EXP-734: the agent run whose OWN pull request a merge confirm is
    /// pending for — its own alert, because the copy names no issues.
    @State private var runMergeTarget: RunReviewEntry?
    /// EXP-1072: the workflow whose FINAL pull request a merge confirm is
    /// pending for. The row leaves once the Electric echo of the workflow's
    /// `final_pr_state` moves off `open`.
    @State private var workflowMergeTarget: WorkflowReviewEntry?
    /// Merge failures keyed by `ReviewEntry.id` — rendered INLINE under the
    /// failing row (EXP-323). An alert made the reason modal and gave the
    /// conflict-recovery run nowhere to live. Each failure also records whether
    /// the server diagnosed a REAL content conflict (EXP-533), which is the
    /// only case the recovery run can fix.
    @State private var mergeErrors: [String: MergeFailure] = [:]
    @State private var merging: Set<String> = []

    // "Fix conflicts" (EXP-323, desktop parity): a failed merge is usually a
    // conflict, so the row offers the builtin recovery run. EXP-825: it is
    // NAVIGATION into the Agent page composer, seeded with the builtin and
    // this row's pull request.
    @State private var steerEnabled = false

    var body: some View {
        let groups = viewModel?.groups(teamId: teamState.activeTeam?.id) ?? []
        // EXP-734: agent runs parking their OWN pull request belong to no
        // board, so they get their own section after the board groups.
        let runs = viewModel?.runEntries(teamId: teamState.activeTeam?.id) ?? []
        // EXP-1072: a workflow's ONE final pull request is the workflow's own
        // PR — its own section, between the boards and the runs.
        let workflows = viewModel?.workflowEntries(teamId: teamState.activeTeam?.id) ?? []
        Group {
            if viewModel == nil {
                Color.clear
            } else if groups.isEmpty && runs.isEmpty && workflows.isEmpty {
                emptyState
            } else {
                reviewList(groups, runs: runs, workflows: workflows)
            }
        }
        .task(id: accountId) {
            let config = await SteerConfigCache.load(accountId: accountId, api: deps.steerApi)
            steerEnabled = config.enabled
        }
        .onAppear {
            if viewModel == nil {
                viewModel = ReviewsViewModel(accountId: accountId, db: deps.db)
            }
            // Re-arm on every appear: pushing an issue detail stops the
            // observation (onDisappear), popping back must resume it.
            viewModel?.startObserving()
        }
        .onDisappear {
            viewModel?.stopObserving()
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
        // EXP-734: a run's own pull request completes no issue, so it confirms
        // with its own copy.
        .alert(
            "Merge pull request?",
            isPresented: Binding(
                get: { runMergeTarget != nil },
                set: { if !$0 { runMergeTarget = nil } }
            ),
            presenting: runMergeTarget
        ) { entry in
            Button("Merge") { merge(run: entry) }
            Button("Cancel", role: .cancel) { runMergeTarget = nil }
        } message: { entry in
            let pr = entry.prNumber.map { "#\($0)" } ?? "this pull request"
            Text("Squash-merges PR \(pr) via the GitHub App. Any live coding session for it closes.")
        }
        // EXP-1072: a workflow's final PR completes the whole workflow, so it
        // confirms with its own copy (web parity).
        .alert(
            workflowMergeTarget?.prNumber.map { "Merge PR #\($0)?" } ?? "Merge pull request?",
            isPresented: Binding(
                get: { workflowMergeTarget != nil },
                set: { if !$0 { workflowMergeTarget = nil } }
            ),
            presenting: workflowMergeTarget
        ) { entry in
            Button("Merge") { merge(workflow: entry) }
            Button("Cancel", role: .cancel) { workflowMergeTarget = nil }
        } message: { entry in
            Text(
                "Squash-merges the final pull request of the workflow \"\(entry.workflow.name)\" (\(entry.branch)) into the repository's default branch via the GitHub App. This completes the workflow and moves every landed issue to the team's PR-merge status."
            )
        }
        // EXP-897: merging a STACK is one call on the bottom row — the server
        // resolves the top and merges every unmerged member below it.
        .alert(
            "Merge the whole stack?",
            isPresented: Binding(
                get: { stackMergeTarget != nil },
                set: { if !$0 { stackMergeTarget = nil } }
            ),
            presenting: stackMergeTarget
        ) { row in
            Button("Merge") { mergeStack(row) }
            Button("Cancel", role: .cancel) { stackMergeTarget = nil }
        } message: { row in
            Text("\(row.stackSize) pull requests, bottom-up.")
        }
        // EXP-897 Part 4: a batch row's issues are the overlay's content.
        .sheet(item: $batchTarget) { entry in
            PrGraphIssueSheet(title: "In this pull request", issues: entry.issues) { issueId in
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
    private func reviewList(
        _ groups: [ReviewGroup],
        runs: [RunReviewEntry],
        workflows: [WorkflowReviewEntry]
    ) -> some View {
        List {
            ForEach(groups) { group in
                Section {
                    // EXP-965: the stack's connector — one guide per row, off
                    // the group's depths (`TreeGuides`, the ×4 rule).
                    let guides = TreeGuides.compute(depths: group.rows.map(\.depth))
                    ForEach(Array(group.rows.enumerated()), id: \.element.id) { index, row in
                        entryRow(row, guide: guides[index])
                            .listRowBackground(Color.clear)
                            .listRowSeparator(.hidden)
                            .listRowInsets(EdgeInsets(top: 1.5, leading: 16, bottom: 1.5, trailing: 16))
                    }
                } header: {
                    boardHeader(board: group.board, count: group.rows.count)
                        .listRowInsets(EdgeInsets(top: 6, leading: 16, bottom: 2, trailing: 16))
                        .listRowBackground(Color.clear)
                }
            }

            // EXP-1072: the workflows' FINAL pull requests — the one human
            // sign-off of a whole run. Each merges through its workflow.
            if !workflows.isEmpty {
                Section {
                    ForEach(workflows) { entry in
                        workflowEntryRow(entry)
                            .listRowBackground(Color.clear)
                            .listRowSeparator(.hidden)
                            .listRowInsets(EdgeInsets(top: 1.5, leading: 16, bottom: 1.5, trailing: 16))
                    }
                } header: {
                    workflowsHeader(count: workflows.count)
                        .listRowInsets(EdgeInsets(top: 6, leading: 16, bottom: 2, trailing: 16))
                        .listRowBackground(Color.clear)
                }
            }

            // EXP-734: the runs' own pull requests, last — they belong to no
            // board, so they cannot ride a board section.
            if !runs.isEmpty {
                Section {
                    ForEach(runs) { entry in
                        runEntryRow(entry)
                            .listRowBackground(Color.clear)
                            .listRowSeparator(.hidden)
                            .listRowInsets(EdgeInsets(top: 1.5, leading: 16, bottom: 1.5, trailing: 16))
                    }
                } header: {
                    runsHeader(count: runs.count)
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

    /// EXP-734: the "Agent runs" section header — same shape as a board
    /// header, with the Actions glyph instead of a board's.
    @ViewBuilder
    private func runsHeader(count: Int) -> some View {
        HStack(spacing: 8) {
            AppIcon(AppIcons.navActions, size: 13)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))

            Text("Agent runs")
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

    /// EXP-1072: the "Workflows" section header — same shape as the runs'
    /// header, with the Workflows glyph and the web's trailing caption.
    @ViewBuilder
    private func workflowsHeader(count: Int) -> some View {
        HStack(spacing: 8) {
            AppIcon(AppIcons.navWorkflows, size: 13)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))

            Text("Workflows")
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))

            Text("\(count)")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))

            Spacer()

            Text("final pull requests")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
        }
        .padding(.vertical, 6)
        .padding(.horizontal, 8)
        .textCase(nil)
    }

    /// EXP-1072: one workflow's final pull request. The row opens the
    /// workflow (its page carries the graph and the gate); Merge completes it.
    /// A real conflict swaps Merge for the recovery run, like an issue row.
    @ViewBuilder
    private func workflowEntryRow(_ entry: WorkflowReviewEntry) -> some View {
        let key = entry.id
        VStack(alignment: .leading, spacing: 6) {
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
                        Text(entry.workflow.name)
                            .font(.subheadline)
                            .foregroundStyle(.white)
                            .lineLimit(1)
                    }

                    if !entry.branch.isEmpty {
                        Text(entry.branch)
                            .font(.caption.monospaced())
                            .lineLimit(1)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                }

                Spacer(minLength: 8)

                if canFixConflicts(workflow: entry) {
                    GlassPill("Fix conflicts", icon: AppIcons.uiBranch)
                        .contentShape(Capsule())
                        .onTapGesture { fixConflicts(workflow: entry) }
                        .accessibilityAddTraits(.isButton)
                        .accessibilityLabel("Fix merge conflicts")
                } else if entry.mergeAction == .merge {
                    // EXP-1094: a final PR merges only while it is open.
                    GlassPill(ReviewsMerge.mergeLabel) {
                        if merging.contains(key) {
                            ProgressView().controlSize(.mini)
                        } else {
                            AppIcon(AppIcons.prMerged, size: GlassPillTokens.glyphSm)
                        }
                    }
                    .contentShape(Capsule())
                    .onTapGesture {
                        guard !merging.contains(key) else { return }
                        workflowMergeTarget = entry
                    }
                    .accessibilityAddTraits(.isButton)
                    .accessibilityLabel("Merge pull request")
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
            .contentShape(Rectangle())
            .onTapGesture {
                pushRoute(.workflow(accountId: accountId, id: entry.workflow.id))
            }
            .swipeActions(edge: .trailing) {
                if entry.mergeAction == .merge {
                    Button { workflowMergeTarget = entry } label: {
                        Label(ReviewsMerge.mergeLabel, appIcon: AppIcons.prMerged)
                    }
                    .tint(DesignTokens.Semantic.green)
                }
            }
            .contextMenu {
                Button {
                    pushRoute(.workflow(accountId: accountId, id: entry.workflow.id))
                } label: {
                    Label("Open workflow", appIcon: AppIcons.navWorkflows)
                }
                if entry.mergeAction == .merge {
                    Button {
                        workflowMergeTarget = entry
                    } label: {
                        Label(DomainContract.diffUiMergePr, appIcon: AppIcons.prMerged)
                    }
                }
                if canFixConflicts(workflow: entry) {
                    Button {
                        fixConflicts(workflow: entry)
                    } label: {
                        Label("Fix merge conflicts", appIcon: AppIcons.uiBranch)
                    }
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

    /// EXP-1072: merging a workflow's FINAL pull request. GitHub's acceptance
    /// completes the workflow and moves every landed issue to the team's
    /// PR-merge status; the row leaves through sync (`final_pr_state`).
    private func merge(workflow entry: WorkflowReviewEntry) {
        workflowMergeTarget = nil
        let key = entry.id
        let workflowId = entry.workflow.id
        mergeErrors[key] = nil
        merging.insert(key)
        Task {
            do {
                try await deps.workflowsApi.mergeFinalPr(accountId: accountId, id: workflowId)
            } catch {
                mergeErrors[key] = MergeFailure(error: error)
            }
            merging.remove(key)
        }
    }

    /// EXP-1072: the recovery run is offered only on a REAL conflict (EXP-533)
    /// and only with steering on; the integration branch is always recorded.
    private func canFixConflicts(workflow entry: WorkflowReviewEntry) -> Bool {
        steerEnabled && mergeErrors[entry.id]?.isConflict == true && !entry.branch.isEmpty
    }

    /// EXP-1072: the composer with "Fix merge conflicts" picked and the
    /// WORKFLOW id as the `pr` value — the server resolves a workflow id to
    /// its final pull request, and the picker offers it as an option.
    private func fixConflicts(workflow entry: WorkflowReviewEntry) {
        pushRoute(.agent(
            accountId: accountId,
            seed: AgentComposerSeed(
                actionId: DomainContract.builtinFixConflictsId,
                prIssueId: entry.workflow.id
            )
        ))
    }

    /// EXP-734: one agent run's own pull request. There is no issue and no
    /// diff screen behind it, so the row opens the PR on GitHub; Merge lives
    /// on the swipe and in the context menu, like the issue rows'.
    @ViewBuilder
    private func runEntryRow(_ entry: RunReviewEntry) -> some View {
        let key = runKey(entry)
        VStack(alignment: .leading, spacing: 6) {
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
            .onTapGesture {
                if let url = entry.prUrl.flatMap(URL.init(string:)) { openURL(url) }
            }
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

    /// EXP-965: the gap the rows sit apart — this list's `listRowInsets`
    /// (1.5 above + 1.5 below), which the connector runs through.
    private static let rowGap: CGFloat = 3

    @ViewBuilder
    private func entryRow(_ row: ReviewRow, guide: TreeGuide) -> some View {
        let entry = row.entry
        // The caption + recovery button live OUTSIDE the NavigationLink: a
        // control inside the link's label has its tap swallowed by the link.
        VStack(alignment: .leading, spacing: 6) {
            entryLink(row)
            stackCaption(row)
            if let failure = mergeErrors[entry.id] {
                mergeErrorCaption(entry, failure: failure)
            }
        }
        // EXP-897: 14 pt per stack level (`TreeGuides.indentPerLevel`, the ×4
        // measure); EXP-965 draws the connector in the gutter it opens, and
        // spans the list's own row gap so the branch is one line, not a dash
        // per row.
        .treeGuides(guide, gap: Self.rowGap)
    }

    /// EXP-897: the row's stack line — what it is built ON and (Part 4) a
    /// batch row's issue count (EXP-1094: Merge stack moved to the row's ONE
    /// merge slot),
    /// which opens the overlay. All of it OUTSIDE the link, so every control
    /// here actually receives its tap.
    @ViewBuilder
    private func stackCaption(_ row: ReviewRow) -> some View {
        let entry = row.entry
        if row.stackedOn != nil || entry.isBatch {
            HStack(spacing: 8) {
                if let below = row.stackedOn {
                    Text("on top of #\(below)")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                }
                if entry.isBatch {
                    GlassPill("\(entry.issues.count) issues", icon: AppIcons.prBatch)
                        .contentShape(Capsule())
                        .onTapGesture { batchTarget = entry }
                        .accessibilityAddTraits(.isButton)
                        .accessibilityLabel("Show the issues on this pull request")
                }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 12)
        }
    }

    /// EXP-897: merging the whole stack — ONE call on the BOTTOM row, which
    /// the server walks up from. Failures caption that row like any other.
    private func mergeStack(_ row: ReviewRow) {
        stackMergeTarget = nil
        let issueId = row.entry.representative.id
        let key = row.entry.id
        mergeErrors[key] = nil
        merging.insert(key)
        Task {
            do {
                try await deps.issuesApi.mergePr(
                    accountId: accountId, issueId: issueId, mergeStack: true
                )
            } catch {
                mergeErrors[key] = MergeFailure(error: error)
            }
            merging.remove(key)
        }
    }

    @ViewBuilder
    private func entryLink(_ row: ReviewRow) -> some View {
        let entry = row.entry
        // The Review detail (the diff + Merge/Close screen) is what a reviewer
        // wants first (EXP-168); the issue itself is one tap away in the menu.
        NavigationLink(value: AppRoute.changes(accountId: accountId, issueId: entry.representative.id)) {
            HStack(alignment: .center, spacing: 10) {
                // PR glyph — the in_review status icon, green, vertically
                // centered like the Android row (EXP-248). EXP-897 Part 4: a
                // BATCH pull request wears the `pr-batch` glyph instead, so
                // one row for several issues reads as one at a glance.
                AppIcon(
                    entry.isBatch ? AppIcons.prBatch : AppIcons.prOpen,
                    size: AppIcon.Size.small
                )
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
                //
                // EXP-706: once the merge failed on a REAL conflict, merging
                // again is the one thing that cannot work — so the recovery
                // run REPLACES Merge in this slot instead of crowding a
                // second button into the caption below. One trailing action,
                // always the one worth tapping.
                //
                // EXP-1094: exactly ONE merge control per row, from the shared
                // rule: a stack's bottom merges the stack, an upper member and
                // a live workflow's node PR carry a muted reason instead.
                if canFixConflicts(entry) {
                    GlassPill("Fix conflicts", icon: AppIcons.uiBranch)
                    .contentShape(Capsule())
                    .onTapGesture {
                        fixConflicts(entry)
                    }
                    .accessibilityAddTraits(.isButton)
                    .accessibilityLabel("Fix merge conflicts")
                } else {
                    switch row.mergeAction {
                    case .merge:
                        GlassPill(ReviewsMerge.mergeLabel) {
                            mergeGlyph(entry, icon: AppIcons.prMerged)
                        }
                        .contentShape(Capsule())
                        .onTapGesture {
                            guard !merging.contains(entry.id) else { return }
                            mergeTarget = entry
                        }
                        .accessibilityAddTraits(.isButton)
                        .accessibilityLabel("Merge pull request")
                    case .mergeStack:
                        GlassPill(ReviewsMerge.mergeStackLabel) {
                            mergeGlyph(entry, icon: AppIcons.prStack)
                        }
                        .contentShape(Capsule())
                        .onTapGesture {
                            guard !merging.contains(entry.id) else { return }
                            stackMergeTarget = row
                        }
                        .accessibilityAddTraits(.isButton)
                        .accessibilityLabel("Merge the whole stack")
                    case .none:
                        if let reason = row.mergeDisabledReason {
                            Text(reason)
                                .font(.caption)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                .lineLimit(1)
                                .accessibilityIdentifier("review-merge-reason")
                        }
                    }
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
        }
        .buttonStyle(.plain)
        .swipeActions(edge: .trailing) {
            // EXP-1094: the same ONE control the row shows.
            switch row.mergeAction {
            case .merge:
                Button { mergeTarget = entry } label: {
                    Label(ReviewsMerge.mergeLabel, appIcon: AppIcons.prMerged)
                }
                .tint(DesignTokens.Semantic.green)
            case .mergeStack:
                Button { stackMergeTarget = row } label: {
                    Label(ReviewsMerge.mergeStackLabel, appIcon: AppIcons.prStack)
                }
                .tint(DesignTokens.Semantic.green)
            case .none:
                EmptyView()
            }
        }
        .contextMenu {
            Button {
                deps.deepLinkBus.navigateToIssue(entry.representative.id)
            } label: {
                Label("Open issue", appIcon: AppIcons.uiIssue)
            }
            switch row.mergeAction {
            case .merge:
                Button {
                    mergeTarget = entry
                } label: {
                    Label(DomainContract.diffUiMergePr, appIcon: AppIcons.prMerged)
                }
            case .mergeStack:
                Button {
                    stackMergeTarget = row
                } label: {
                    Label(ReviewsMerge.mergeStackLabel, appIcon: AppIcons.prStack)
                }
            case .none:
                EmptyView()
            }
            if entry.isBatch {
                Button {
                    batchTarget = entry
                } label: {
                    Label("Show the issues", appIcon: AppIcons.prBatch)
                }
            }
            if canFixConflicts(entry) {
                Button {
                    fixConflicts(entry)
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

    /// The merge pill's glyph: a spinner while that row's merge is in flight.
    @ViewBuilder
    private func mergeGlyph(_ entry: ReviewEntry, icon: String) -> some View {
        if merging.contains(entry.id) {
            ProgressView().controlSize(.mini)
        } else {
            AppIcon(icon, size: GlassPillTokens.glyphSm)
        }
    }

    /// A refused merge (conflicts, branch protection, GitHub App errors)
    /// captions THIS row (EXP-323) — never a modal alert, and never anything
    /// the tab bar can cover. EXP-706: the caption is the REASON only; the
    /// recovery run took the row's Merge slot, so repeating it here would be
    /// two buttons for one action.
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
    /// the same guard the desktop applies on its Reviews rows. EXP-533: only a
    /// REAL conflict (the server's `CONFLICT` / HTTP 409) is offered the run; a
    /// stale head, branch protection or an unreachable server refused the merge
    /// for a reason no rebase can fix.
    private func canFixConflicts(_ entry: ReviewEntry) -> Bool {
        steerEnabled &&
            mergeErrors[entry.id]?.isConflict == true &&
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
                mergeErrors[key] = MergeFailure(error: error)
            }
            merging.remove(key)
        }
    }

    /// EXP-825: the recovery run is the composer with the "Fix merge
    /// conflicts" builtin picked and this row's pull request pre-picked —
    /// ANY linked issue resolves (the picker normalises by membership).
    private func fixConflicts(_ entry: ReviewEntry) {
        pushRoute(.agent(
            accountId: accountId,
            seed: AgentComposerSeed(
                actionId: DomainContract.builtinFixConflictsId,
                prIssueId: entry.representative.id
            )
        ))
    }
}
