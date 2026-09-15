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
/// Two hosts, one bar recipe (`FloatingBottomBar`):
/// - Reviews (`reviewMode`): close-PR circle · Merge / Fix conflicts · GitHub,
///   plus the close-without-merge dialog (Close exists nowhere else on iOS).
/// - The Work screen: GitHub · Merge / Fix conflicts · the face switcher.
struct PrChangesFace<Trailing: View>: View {
    let issueId: String
    let reviewMode: Bool
    @ViewBuilder let trailing: () -> Trailing

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @Environment(\.openURL) private var openURL
    /// EXP-706: the file rows' chevron rotates instead of swapping glyph, so
    /// the screen needs the shared motion tokens (nil under Reduce Motion).
    @Environment(\.motion) private var motion
    @State private var viewModel: ChangesViewModel?
    @State private var mergeConfirm = false
    @State private var closeConfirm = false
    // "Fix conflicts" (EXP-323, desktop parity): a refused merge is usually a
    // conflict, so the bar offers the builtin recovery run seeded with THIS
    // pull request. EXP-825: NAVIGATION into the Agent page composer.
    @State private var steerEnabled = false

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
            if viewModel == nil {
                viewModel = ChangesViewModel(
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
            viewModel?.startObserving()
        }
        .task(id: accountId) {
            let config = await SteerConfigCache.load(accountId: accountId, api: deps.steerApi)
            steerEnabled = config.enabled
        }
        .onDisappear {
            viewModel?.stopObserving()
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
            Button("Close PR without merging", role: .destructive) { viewModel?.closePr() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Closes the pull request on GitHub without merging. Use this when the issue was dropped even though the work exists. The branch is kept and the PR can be reopened on GitHub.")
        }
    }

    /// The merge alert message — carries the PR number when known.
    private var mergeMessage: String {
        if let number = viewModel?.issue?.prNumber {
            return "Squash-merges PR #\(number) via the GitHub App. Any live coding session for it closes."
        }
        return "Squash-merges this pull request via the GitHub App. Any live coding session for it closes."
    }

    @ViewBuilder
    private func content(_ vm: ChangesViewModel) -> some View {
        let loadedFiles: [PrFile]? = {
            if case let .loaded(files) = vm.load { return files }
            return nil
        }()
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 10) {
                // The PR/branch header (and the floating action bar below)
                // come from synced issue fields, so they render in EVERY load
                // state — a diff-fetch failure must never strand a member
                // without Merge / Close. The stats line only shows once files
                // are loaded.
                if vm.issue != nil {
                    summaryHeader(vm: vm, files: loadedFiles)
                }

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
                case let .loaded(files):
                    if files.isEmpty {
                        Text("No changed files.")
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .padding(.vertical, 12)
                    }
                    ForEach(files) { file in
                        fileSection(file, expanded: vm.expanded.contains(file.filename)) {
                            vm.toggle(file.filename)
                        }
                    }
                }
            }
            .padding(.horizontal, 16)
            .padding(.top, 4)
            .padding(.bottom, 24)
        }
        .stickyHeaderFade()
    }

    // MARK: - Summary header

    private func summaryHeader(vm: ChangesViewModel, files: [PrFile]?) -> some View {
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
                    DiffSummaryRow(
                        files: files.count,
                        additions: files.reduce(0) { $0 + $1.additions },
                        deletions: files.reduce(0) { $0 + $1.deletions }
                    )
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
        !reviewMode || canReview(vm) || prURL(vm) != nil
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
                FloatingBottomBar {
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

    @ViewBuilder
    private func barLeading(_ vm: ChangesViewModel) -> some View {
        if reviewMode {
            if canReview(vm) {
                closeCircle(vm)
            }
        } else if let url = prURL(vm) {
            githubCircle(url)
        }
    }

    /// EXP-706: the bar's ONE primary action. A merge the server refused on
    /// a REAL conflict cannot succeed on a retry, so the recovery run
    /// REPLACES Merge in this slot — same white capsule, so the bar keeps
    /// exactly one thing to press.
    @ViewBuilder
    private func barCenter(_ vm: ChangesViewModel) -> some View {
        if canReview(vm) {
            if canFixConflicts(vm) {
                FloatingBarSolidPill(
                    accessibilityLabel: "Fix merge conflicts",
                    enabled: !vm.merging && !vm.closing,
                    action: openFixConflicts
                ) {
                    AppIcon(AppIcons.uiBranch, size: AppIcon.Size.medium, weight: .medium)
                    Text("Fix conflicts")
                        .font(.subheadline.weight(.medium))
                }
            } else {
                FloatingBarSolidPill(
                    accessibilityLabel: "Merge pull request",
                    enabled: !vm.merging && !vm.closing,
                    action: { mergeConfirm = true }
                ) {
                    if vm.merging {
                        ProgressView().controlSize(.small).tint(.black.opacity(0.6))
                    } else {
                        AppIcon(AppIcons.prMerged, size: AppIcon.Size.medium, weight: .medium)
                    }
                    Text("Merge PR")
                        .font(.subheadline.weight(.medium))
                }
            }
        }
    }

    @ViewBuilder
    private func barTrailing(_ vm: ChangesViewModel) -> some View {
        if reviewMode {
            if let url = prURL(vm) {
                githubCircle(url)
            }
        } else {
            trailing()
        }
    }

    private func closeCircle(_ vm: ChangesViewModel) -> some View {
        FloatingBarCircle(
            accessibilityLabel: "Close PR without merging",
            enabled: !vm.merging && !vm.closing,
            action: { closeConfirm = true }
        ) {
            if vm.closing {
                ProgressView().controlSize(.small).tint(.white)
            } else {
                AppIcon(AppIcons.uiClose, size: AppIcon.Size.medium, weight: .medium)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        }
    }

    private func githubCircle(_ url: URL) -> some View {
        FloatingBarCircle(accessibilityLabel: "Open PR on GitHub", action: { openURL(url) }) {
            AppIcon(AppIcons.uiExternalLink, size: AppIcon.Size.medium, weight: .medium)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
        }
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

    // MARK: - Per-file section

    /// One changed file: a tappable header (status letter, filename, +/−
    /// counts) over a collapsible unified patch with the shared line coloring.
    private func fileSection(_ file: PrFile, expanded: Bool, onToggle: @escaping () -> Void) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Button(action: onToggle) {
                HStack(spacing: 8) {
                    Text(Self.statusLetter(file.status))
                        .font(.caption.monospaced().weight(.bold))
                        .foregroundStyle(Self.statusColor(file.status))
                    Text(file.filename)
                        .font(.caption.monospaced())
                        .foregroundStyle(.white)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    Text("+\(file.additions)")
                        .font(.caption2.monospaced())
                        .foregroundStyle(.green)
                    Text("-\(file.deletions)")
                        .font(.caption2.monospaced())
                        .foregroundStyle(.red)
                    // EXP-706: ONE chevron that turns over, not two glyphs
                    // swapping — the rotation reads as the section opening.
                    AppIcon(AppIcons.uiChevronDown, size: 11)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .rotationEffect(.degrees(expanded ? 180 : 0))
                        .animation(motion.standard, value: expanded)
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(expanded ? "Collapse \(file.filename)" : "Expand \(file.filename)")
            .accessibilityIdentifier("changes-file-row")

            if expanded {
                if let patch = file.patch, !patch.isEmpty {
                    DiffPatchBlock(patch: patch)
                        .padding(.horizontal, 8)
                        .padding(.bottom, 8)
                } else {
                    Text(file.status == "renamed" ? "Renamed." : "No textual diff (binary or too large).")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .padding(.horizontal, 12)
                        .padding(.bottom, 10)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        // A gapped list item (one file among many), so it wears the row
        // hairline the borderless group no longer draws.
        .glassRow()
    }

    // GitHub file statuses: added / modified / removed / renamed / copied / changed.
    private static func statusLetter(_ status: String) -> String {
        switch status {
        case "added": "A"
        case "removed": "D"
        case "renamed": "R"
        case "copied": "C"
        default: "M"
        }
    }

    /// EXP-706: all four letters come from the shared semantic palette — A
    /// green, D red, R/C blue, M amber.
    private static func statusColor(_ status: String) -> Color {
        switch status {
        case "added": DesignTokens.Semantic.green
        case "removed": DesignTokens.Semantic.red
        case "renamed", "copied": DesignTokens.Semantic.blue
        default: DesignTokens.Semantic.yellow
        }
    }
}
