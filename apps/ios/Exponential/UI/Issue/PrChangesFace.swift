import ExpCore
import ExpUI
import SwiftUI

/// EXP-893: an issue's PR / pushed-branch diff as a FACE — the content, the
/// merge confirm, the recovery run and the floating bar, lifted out of
/// `ChangesView` (EXP-34/156/248) so the Work screen's Changes face and the
/// Reviews page draw the same thing. Observes the issue row (so the diff
/// source flips live when a PR opens) and loads the files from the tier that
/// applies (`ChangesViewModel`).
///
/// Two hosts, one bar recipe (`FloatingBottomBar`). EXP-895 locks the leading
/// slot to the phone's FILE SHEET on both, and moves GitHub up into the nav
/// bar's action slot:
/// - Reviews (`reviewMode`): file sheet · Merge / Fix conflicts · close-PR
///   circle, plus the close-without-merge dialog (Close exists nowhere else
///   on iOS).
/// - The Work screen: file sheet · Merge / Fix conflicts · the face switcher.
///
/// EXP-952: the Work screen hands its OWN `ChangesViewModel` in (`model`),
/// so its switcher counts the very files this face draws — before the face
/// was ever opened, and never a different list. nil = this face creates and
/// drives its own, which is what the Reviews page does.
struct PrChangesFace<Trailing: View>: View {
    let issueId: String
    let reviewMode: Bool
    /// EXP-952: an injected model the HOST owns (its lifecycle is the host's:
    /// this face never starts or stops it). nil = own one, created on appear.
    var model: ChangesViewModel? = nil
    @ViewBuilder let trailing: () -> Trailing

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.openURL) private var openURL
    /// The face's OWN model, when none was injected.
    @State private var ownModel: ChangesViewModel?
    @State private var mergeConfirm = false
    @State private var closeConfirm = false
    // "Fix conflicts" (EXP-323, desktop parity): a refused merge is usually a
    // conflict, so the bar offers the builtin recovery run seeded with THIS
    // pull request. EXP-825: NAVIGATION into the Agent page composer.
    @State private var steerEnabled = false
    /// EXP-895: the phone's file list, off the bar's leading slot.
    @State private var fileSheet = false
    /// The path the sheet last picked — the card list expands it and jumps.
    @State private var selectedPath: String?

    /// The one model this face reads: the host's, else its own.
    private var viewModel: ChangesViewModel? { model ?? ownModel }

    var body: some View {
        Group {
            if let vm = viewModel {
                content(vm)
            } else {
                ProgressView().tint(.white)
            }
        }
        // Floating review actions (EXP-248) — reserves scroll clearance like
        // the issue-detail bottom bar.
        .safeAreaInset(edge: .bottom) {
            if let vm = viewModel {
                bottomBar(vm)
            }
        }
        .onAppear {
            // EXP-952: an injected model is the host's to start and stop.
            guard model == nil else { return }
            if ownModel == nil {
                ownModel = ChangesViewModel(
                    accountId: accountId,
                    issueId: issueId,
                    db: deps.db,
                    issuesApi: deps.issuesApi,
                    repositoriesApi: deps.repositoriesApi,
                    auth: deps.auth
                )
            }
            // Re-arm on every appear: pushing another screen stops the
            // observation (onDisappear), popping back must resume it.
            ownModel?.startObserving()
        }
        .task(id: accountId) {
            let config = await SteerConfigCache.load(accountId: accountId, api: deps.steerApi)
            steerEnabled = config.enabled
        }
        .onDisappear {
            ownModel?.stopObserving()
        }
        // Squash-merge (EXP-131) — confirm-gated like the Reviews list.
        .alert("Merge pull request?", isPresented: $mergeConfirm) {
            Button("Merge") { viewModel?.mergePr() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(mergeMessage)
        }
        // Close-without-merge (EXP-100) — the drop path; Reviews only.
        .confirmationDialog(
            "Close pull request?",
            isPresented: $closeConfirm,
            titleVisibility: .visible
        ) {
            Button(DomainContract.diffUiClosePr, role: .destructive) { viewModel?.closePr() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Closes the pull request on GitHub without merging. Use this when the issue was dropped even though the work exists. The branch is kept and the PR can be reopened on GitHub.")
        }
        // EXP-895: the file list. A column beside the cards leaves neither
        // readable on a phone, so it is a bottom sheet off the bar.
        .sheet(isPresented: $fileSheet) {
            DiffFileListSheet(
                files: viewModel.flatMap(loadedFiles) ?? [],
                selected: selectedPath,
                onSelect: { selectedPath = $0 }
            )
        }
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                // The Work screen carries its own (it owns that toolbar across
                // every face); this is the Reviews page's.
                if reviewMode, let vm = viewModel, let url = prURL(vm) {
                    githubToolbarButton(url)
                }
            }
        }
    }

    /// The merge alert message — carries the PR number when known.
    private var mergeMessage: String {
        if let number = viewModel?.issue?.prNumber {
            return "Squash-merges PR #\(number) via the GitHub App. Any live coding session for it closes."
        }
        return "Squash-merges this pull request via the GitHub App. Any live coding session for it closes."
    }

    /// The loaded files, or nil while the fetch is out / failed.
    private func loadedFiles(_ vm: ChangesViewModel) -> [Diff.File]? {
        if case let .loaded(files) = vm.load { return files }
        return nil
    }

    private func content(_ vm: ChangesViewModel) -> some View {
        let files = loadedFiles(vm)
        // EXP-895: the ONE diff view. EXP-916: every card starts OPEN here
        // too — a review that has to tap every file to read it is not a
        // review; only the size rule folds a huge file away.
        return DiffFileList(
            files: files ?? [],
            emptyLabel: files == nil ? nil : "No changed files.",
            focusPath: selectedPath,
            accessibilityId: "changes-file-cards",
            header: {
                // The PR/branch header (and the floating action bar below)
                // come from synced issue fields, so they render in EVERY load
                // state — a diff-fetch failure must never strand a member
                // without Merge / Close. The stats line only shows once files
                // are loaded.
                if vm.issue != nil {
                    summaryHeader(vm: vm, files: files)
                }
                loadStatus(vm)
            }
        )
    }

    @ViewBuilder
    private func loadStatus(_ vm: ChangesViewModel) -> some View {
        switch vm.load {
        case .loading:
            HStack(spacing: 8) {
                ProgressView().controlSize(.small).tint(.white)
                Text("Loading changes…")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.top, 4)
        case let .failed(message):
            Text("Couldn't load changes: \(message)")
                .font(.caption)
                .foregroundStyle(DesignTokens.Semantic.red)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.top, 4)
        case .loaded:
            EmptyView()
        }
    }

    // MARK: - Summary header

    private func summaryHeader(vm: ChangesViewModel, files: [Diff.File]?) -> some View {
        let issue = vm.issue
        return VStack(alignment: .leading, spacing: 8) {
            if let branch = issue?.branch, !branch.isEmpty {
                Text(branch)
                    .font(.caption.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            HStack(spacing: 8) {
                if let prState = issue?.prState, !prState.isEmpty {
                    GlassPill(prState.capitalized)
                }
                // Stats depend on the diff fetch — shown only once it lands.
                if let files {
                    DiffSummaryRow(totals: Diff.totals(files))
                }
                Spacer(minLength: 0)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .glassCard()
    }

    // MARK: - Floating action bar

    /// Membership + an open PR gate Merge / Close; the server enforces the
    /// rule too — this only hides controls a viewer cannot use.
    private func canReview(_ vm: ChangesViewModel) -> Bool {
        vm.permissions.isMember
            && vm.issue?.prState == DomainContract.prStateOpen
            && (vm.issue?.prUrl?.isEmpty == false)
    }

    private func prURL(_ vm: ChangesViewModel) -> URL? {
        vm.issue?.prUrl.flatMap { URL(string: $0) }
    }

    /// Reviews shows the bar only with something to act on (the pushed-branch
    /// tier has none); the Work screen always carries the switcher.
    private func barVisible(_ vm: ChangesViewModel) -> Bool {
        !reviewMode || canReview(vm) || loadedFiles(vm)?.isEmpty == false
    }

    /// Review actions (EXP-248) on the shared floating bar. A failed
    /// merge/close captions the bar, right where the user just tapped —
    /// the reason only (EXP-706): a conflict's recovery run took the Merge
    /// slot, so repeating it here would be two buttons for one action.
    @ViewBuilder
    private func bottomBar(_ vm: ChangesViewModel) -> some View {
        if barVisible(vm) {
            VStack(spacing: 8) {
                if let actionError = vm.actionError {
                    Text(actionError)
                        .font(.caption)
                        .foregroundStyle(DesignTokens.Semantic.red)
                        .multilineTextAlignment(.center)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 8)
                        .glassCard()
                        .padding(.horizontal, 16)
                }
                // EXP-916: the centred cluster ×3 — files · Merge PR ·
                // reject on the Reviews page, files · Merge PR · switcher on
                // the Work screen's Changes face. One bar for both.
                FloatingBarCluster {
                    barLeading(vm)
                } center: {
                    barCenter(vm)
                } trailing: {
                    barTrailing(vm)
                }
            }
            // EXP-642: the store slide's pop-out rect is measured off the
            // review bar (`PopRects`). `contain` keeps its buttons queryable.
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("pr-merge-bar")
        }
    }

    /// EXP-895: the bar's LEADING slot is the phone's file list, on every
    /// Changes surface — GitHub moved up into the nav bar's action slot, where
    /// a phone header has room for it. The locked layout is
    /// `[file sheet][merge capsule][switcher]`.
    @ViewBuilder
    private func barLeading(_ vm: ChangesViewModel) -> some View {
        if let files = loadedFiles(vm), !files.isEmpty {
            DiffFilesBarCircle(count: files.count) { fileSheet = true }
        }
    }

    /// EXP-706: the bar's ONE primary action. A merge the server refused on
    /// a REAL conflict cannot succeed on a retry, so the recovery run
    /// REPLACES Merge in this slot — same capsule, so the bar keeps exactly
    /// one thing to press.
    ///
    /// EXP-916: a SOLID white pill hugging its label, the one solid thing on
    /// the bar, on both hosts (Android's `BarSolidPill`).
    @ViewBuilder
    private func barCenter(_ vm: ChangesViewModel) -> some View {
        if canReview(vm) {
            let fix = canFixConflicts(vm)
            FloatingBarSolidPill(
                accessibilityLabel: fix ? "Fix merge conflicts" : "Merge pull request",
                enabled: !vm.merging && !vm.closing,
                action: fix ? openFixConflicts : { mergeConfirm = true }
            ) {
                if vm.merging {
                    ProgressView().controlSize(.small).tint(.black.opacity(0.6))
                } else {
                    AppIcon(
                        fix ? AppIcons.uiBranch : AppIcons.prMerged,
                        size: FloatingBarTokens.glyph,
                        weight: .medium
                    )
                }
                Text(fix ? "Fix conflicts" : DomainContract.diffUiMergePr)
                    .font(.subheadline.weight(.medium))
            }
        }
    }

    /// The Work screen's face switcher; on the Reviews page (which has no
    /// switcher) the close-without-merge circle, the one control that exists
    /// nowhere else on iOS.
    @ViewBuilder
    private func barTrailing(_ vm: ChangesViewModel) -> some View {
        if reviewMode {
            if canReview(vm) {
                closeCircle(vm)
            }
        } else {
            trailing()
        }
    }

    private func closeCircle(_ vm: ChangesViewModel) -> some View {
        FloatingBarCircle(
            accessibilityLabel: DomainContract.diffUiClosePr,
            enabled: !vm.merging && !vm.closing,
            action: { closeConfirm = true }
        ) {
            if vm.closing {
                ProgressView().controlSize(.small).tint(.white)
            } else {
                // EXP-916: the PR-closed mark, not a generic ✕.
                AppIcon(AppIcons.prClosed, size: FloatingBarTokens.glyph, weight: .medium)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        }
    }

    /// EXP-895: GitHub in the nav bar's ACTION slot. The Work screen hosts its
    /// own (it owns that toolbar across faces); the Reviews page's is here.
    private func githubToolbarButton(_ url: URL) -> some View {
        Button { openURL(url) } label: {
            AppIcon(AppIcons.uiGithub, size: AppIcon.Size.medium, weight: .medium)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
        }
        .accessibilityLabel(DomainContract.diffUiOpenOnGithub)
        .accessibilityIdentifier("changes-github-action")
    }

    // MARK: - Fix conflicts (EXP-323)

    /// EXP-825: the recovery run is the composer with the "Fix merge
    /// conflicts" builtin picked and THIS pull request pre-picked.
    private func openFixConflicts() {
        pushRoute(.agent(
            accountId: accountId,
            seed: AgentComposerSeed(
                actionId: DomainContract.builtinFixConflictsId,
                prIssueId: issueId
            )
        ))
    }

    /// Whether the bar offers the recovery run in place of Merge (EXP-323 /
    /// EXP-533, promoted to the Merge slot by EXP-706): only after a MERGE
    /// failure the server diagnosed as a REAL content conflict — the run ends
    /// in a merge, the opposite of what a failed CLOSE asked for, and it
    /// rebases the PR's branch, so one must be recorded.
    private func canFixConflicts(_ vm: ChangesViewModel) -> Bool {
        steerEnabled
            && vm.actionErrorFrom == .merge
            && vm.actionErrorIsConflict
            && canReview(vm)
            && !(vm.issue?.branch ?? "").isEmpty
    }

}
