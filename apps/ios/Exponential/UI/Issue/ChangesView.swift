import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// Backs the dedicated diff page (EXP-34). Observes the issue row (so the diff
/// source flips live when a PR opens on a watched branch) and loads the changed
/// files from the tier that applies: PR present → `issues.prFiles`, otherwise
/// the pushed branch via `repositories.branchDiff`. Mirrors the Android
/// ChangesViewModel.
@MainActor @Observable
final class ChangesViewModel {
    enum LoadState {
        case loading
        case failed(String)
        case loaded([PrFile])
    }

    private(set) var issue: IssueEntity?
    private(set) var load: LoadState = .loading
    /// Filenames whose patch is expanded — ≤3 files start expanded, more start
    /// collapsed (reset on every reload).
    private(set) var expanded: Set<String> = []

    /// Membership gates the Merge / Close affordances (resolved from the issue's
    /// board → team, like IssueDetailViewModel.refreshPermissions). The
    /// server enforces the rule too; this just hides controls a viewer can't use.
    private(set) var permissions: TeamPermissions = .denied
    private(set) var merging = false
    private(set) var closing = false
    private(set) var actionError: String?

    private let accountId: String
    private let issueId: String
    private let db: DatabaseManager
    private let issuesApi: IssuesApi
    private let repositoriesApi: RepositoriesApi
    private let auth: AuthRepository

    private var observationTask: Task<Void, Never>?
    /// nil until the first issue row arrives; a flip re-fetches (Android's
    /// `distinctUntilChanged` on hasPr).
    private var hadPr: Bool?

    init(
        accountId: String,
        issueId: String,
        db: DatabaseManager,
        issuesApi: IssuesApi,
        repositoriesApi: RepositoriesApi,
        auth: AuthRepository
    ) {
        self.accountId = accountId
        self.issueId = issueId
        self.db = db
        self.issuesApi = issuesApi
        self.repositoriesApi = repositoriesApi
        self.auth = auth
    }

    func startObserving() {
        stopObserving() // restartable: the view re-arms on every appear
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let issueId = self.issueId
        let observation = ValueObservation.tracking { db in
            try IssueEntity.filter(Column("id") == issueId).fetchOne(db)
        }
        observationTask = Task { [weak self] in
            do {
                for try await row in observation.values(in: pool) {
                    guard let self, let row else { continue }
                    self.issue = row
                    self.refreshPermissions(for: row)
                    // Re-fetch when the diff source flips (a PR opens on a
                    // watched branch) — and once on the first row.
                    let hasPr = row.prUrl?.isEmpty == false
                    if self.hadPr != hasPr {
                        self.hadPr = hasPr
                        Task { await self.refresh() }
                    }
                }
            } catch {}
        }
    }

    func stopObserving() {
        observationTask?.cancel()
        observationTask = nil
    }

    func refresh() async {
        load = .loading
        do {
            let files: [PrFile]
            if issue?.prUrl?.isEmpty == false {
                files = try await issuesApi.prFiles(accountId: accountId, issueId: issueId).files
            } else {
                files = try await repositoriesApi.branchDiff(accountId: accountId, issueId: issueId)?.files ?? []
            }
            // Every file starts collapsed (EXP-248) — uniform with the web
            // and Android review detail.
            expanded = []
            load = .loaded(files)
        } catch {
            load = .failed(error.localizedDescription)
        }
    }

    func toggle(_ filename: String) {
        if expanded.contains(filename) {
            expanded.remove(filename)
        } else {
            expanded.insert(filename)
        }
    }

    /// Resolve membership from the issue's board → team (mirror of
    /// IssueDetailViewModel.refreshPermissions) so the review actions only show
    /// for members.
    private func refreshPermissions(for issue: IssueEntity) {
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let team: TeamEntity? = (try? pool.read { db -> TeamEntity? in
            let board = try BoardEntity.fetchOne(db, key: issue.boardId)
            return try board.flatMap { try TeamEntity.fetchOne(db, key: $0.teamId) }
        }) ?? nil
        permissions = TeamPermissions.resolve(
            team: team,
            currentUserId: auth.userId,
            isAdmin: auth.isAdmin,
            dbPool: pool
        )
    }

    /// Squash-merge the PR via the GitHub App (EXP-131). Success needs no local
    /// write — Electric echoes the prState/status flips.
    func mergePr() {
        guard !merging else { return }
        merging = true
        actionError = nil
        Task {
            do {
                try await issuesApi.mergePr(accountId: accountId, issueId: issueId)
            } catch {
                actionError = error.localizedDescription
            }
            merging = false
        }
    }

    /// Close the PR WITHOUT merging (EXP-100 — the drop path). The prState flip
    /// arrives through Electric sync; failures caption the floating action bar.
    func closePr() {
        guard !closing else { return }
        closing = true
        actionError = nil
        Task {
            do {
                try await issuesApi.closePr(accountId: accountId, issueId: issueId)
            } catch {
                actionError = error.localizedDescription
            }
            closing = false
        }
    }
}

/// The dedicated diff + review page (EXP-34/156): summary header (branch,
/// PR-state badge, totals) + per-file expandable unified patches with the
/// shared DiffRendering coloring, with the review actions (Merge / Close /
/// GitHub) in a floating bottom bar (EXP-248 — uniform with web/Android).
/// Pushed from AgentPrCard's PR / branch rows.
/// Horizontal panning stays inside each file's code block — the page itself
/// never scrolls sideways. Matches the Android ChangesScreen's hierarchy.
struct ChangesView: View {
    let issueId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var viewModel: ChangesViewModel?
    @State private var mergeConfirm = false
    @State private var closeConfirm = false

    var body: some View {
        ZStack {
            AppBackground()

            if let vm = viewModel {
                content(vm)
            } else {
                ProgressView().tint(.white)
            }
        }
        .navigationTitle("Review")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        // Floating review actions (EXP-248) — reserves scroll clearance like
        // the issue-detail bottom bar.
        .safeAreaInset(edge: .bottom) {
            if let vm = viewModel {
                changesBottomBar(vm)
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
        // Close-without-merge (EXP-100) — the drop path; exact copy from the
        // former ChangesSection.
        .confirmationDialog(
            "Close pull request?",
            isPresented: $closeConfirm,
            titleVisibility: .visible
        ) {
            Button("Close PR without merging", role: .destructive) { viewModel?.closePr() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Closes the pull request on GitHub without merging — use this when the issue was dropped even though the work exists. The branch is kept and the PR can be reopened on GitHub.")
        }
    }

    /// The merge alert message — carries the PR number when known.
    private var mergeMessage: String {
        if let number = viewModel?.issue?.prNumber {
            return "Squash-merges PR #\(number) via the GitHub App."
        }
        return "Squash-merges this pull request via the GitHub App."
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
                // without Merge / Close (Close exists nowhere else on iOS).
                // The stats line only shows once files are loaded.
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
                    Text(prState.capitalized)
                        .font(.caption2.weight(.medium))
                        .foregroundStyle(.white)
                        .padding(.horizontal, 8)
                        .padding(.vertical, 3)
                        .glassButton()
                }
                // Stats depend on the diff fetch — shown only once it lands.
                if let files {
                    Text("\(files.count) \(files.count == 1 ? "file" : "files")")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    Text("+\(files.reduce(0) { $0 + $1.additions })")
                        .font(.caption.monospaced())
                        .foregroundStyle(.green)
                    Text("−\(files.reduce(0) { $0 + $1.deletions })")
                        .font(.caption.monospaced())
                        .foregroundStyle(.red)
                }
                Spacer()
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .glassSection()
    }

    // MARK: - Floating action bar

    /// Review actions (EXP-248): a floating bottom bar matching the main
    /// tab bar / issue-detail bar chrome — dismiss (icon), Merge (labeled,
    /// center), open on GitHub (icon). Merge/dismiss show for members on an
    /// open PR; the GitHub circle whenever a PR exists. Hidden entirely when
    /// there is no PR to act on (pushed-branch tier). A failed merge/close
    /// captions the bar, right where the user just tapped.
    @ViewBuilder
    private func changesBottomBar(_ vm: ChangesViewModel) -> some View {
        let issue = vm.issue
        let canReview = vm.permissions.isMember
            && issue?.prState == DomainContract.prStateOpen
            && (issue?.prUrl?.isEmpty == false)
        let prURL = issue?.prUrl.flatMap { URL(string: $0) }
        if canReview || prURL != nil {
            VStack(spacing: 8) {
                // A merge/close failure captions the bar that produced it —
                // the summary header at the top of the scroll is off-screen
                // once the actions moved down here (EXP-248 follow-up).
                if let actionError = vm.actionError {
                    Text(actionError)
                        .font(.caption)
                        .foregroundStyle(DesignTokens.Semantic.red)
                        .multilineTextAlignment(.center)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 8)
                        .glassSection()
                        .padding(.horizontal, 16)
                }

                HStack(spacing: 12) {
                    if canReview {
                        Button {
                            closeConfirm = true
                        } label: {
                            barCircle {
                                if vm.closing {
                                    ProgressView().controlSize(.small).tint(.white)
                                } else {
                                    Image(systemName: "xmark")
                                        .font(.body.weight(.medium))
                                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                }
                            }
                        }
                        .buttonStyle(.plain)
                        .disabled(vm.merging || vm.closing)
                        .accessibilityLabel("Close PR without merging")

                        Button {
                            mergeConfirm = true
                        } label: {
                            HStack(spacing: 8) {
                                if vm.merging {
                                    ProgressView().controlSize(.small).tint(.white)
                                } else {
                                    Image(systemName: "arrow.triangle.merge")
                                        .font(.body.weight(.medium))
                                }
                                Text("Merge")
                                    .font(.subheadline.weight(.medium))
                            }
                            .foregroundStyle(.white)
                            .padding(.horizontal, 28)
                            .frame(height: 52)
                            .background(.ultraThinMaterial, in: Capsule())
                            .overlay(
                                Capsule().stroke(Color.white.opacity(0.12), lineWidth: 0.5)
                            )
                            .shadow(color: .black.opacity(0.35), radius: 16, y: 6)
                            .contentShape(Capsule())
                        }
                        .buttonStyle(.plain)
                        .disabled(vm.merging || vm.closing)
                        .accessibilityLabel("Merge pull request")
                    }

                    if let prURL {
                        Link(destination: prURL) {
                            barCircle {
                                Image(systemName: "arrow.up.right.square")
                                    .font(.body.weight(.medium))
                                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            }
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("Open PR on GitHub")
                    }
                }
            }
            .padding(.top, 8)
            .padding(.bottom, 4)
        }
    }

    /// Icon-only circle — same chrome as MobileTabBar / IssueDetailBottomBar
    /// (ultraThinMaterial, white-12% hairline, soft shadow).
    private func barCircle<Content: View>(@ViewBuilder content: () -> Content) -> some View {
        content()
            .frame(width: 52, height: 52)
            .background(.ultraThinMaterial, in: Circle())
            .overlay(
                Circle().stroke(Color.white.opacity(0.12), lineWidth: 0.5)
            )
            .shadow(color: .black.opacity(0.35), radius: 16, y: 6)
            .contentShape(Circle())
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
                    Text("−\(file.deletions)")
                        .font(.caption2.monospaced())
                        .foregroundStyle(.red)
                    Image(systemName: expanded ? "chevron.up" : "chevron.down")
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(expanded ? "Collapse \(file.filename)" : "Expand \(file.filename)")

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
        .glassSection()
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

    private static func statusColor(_ status: String) -> Color {
        switch status {
        case "added": .green
        case "removed": .red
        case "renamed", "copied": Accent.indigo
        default: .white.opacity(TextOpacity.secondary)
        }
    }
}
