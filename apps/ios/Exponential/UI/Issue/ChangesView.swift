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

    private let accountId: String
    private let issueId: String
    private let db: DatabaseManager
    private let issuesApi: IssuesApi
    private let repositoriesApi: RepositoriesApi

    private var observationTask: Task<Void, Never>?
    /// nil until the first issue row arrives; a flip re-fetches (Android's
    /// `distinctUntilChanged` on hasPr).
    private var hadPr: Bool?

    init(
        accountId: String,
        issueId: String,
        db: DatabaseManager,
        issuesApi: IssuesApi,
        repositoriesApi: RepositoriesApi
    ) {
        self.accountId = accountId
        self.issueId = issueId
        self.db = db
        self.issuesApi = issuesApi
        self.repositoriesApi = repositoriesApi
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
            expanded = files.count <= 3 ? Set(files.map(\.filename)) : []
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
}

/// The dedicated diff page (EXP-34): summary header (branch, PR-state badge,
/// totals, GitHub link) + per-file expandable unified patches with the shared
/// DiffRendering coloring. Pushed from ChangesSection's "View changes" on both
/// the PR tier and the pushed-branch tier. Horizontal panning stays inside
/// each file's code block — the page itself never scrolls sideways. Matches
/// the Android ChangesScreen's information hierarchy.
struct ChangesView: View {
    let issueId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var viewModel: ChangesViewModel?

    var body: some View {
        ZStack {
            AppBackground()

            if let vm = viewModel {
                content(vm)
            } else {
                ProgressView().tint(.white)
            }
        }
        .navigationTitle("Changes")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .onAppear {
            if viewModel == nil {
                viewModel = ChangesViewModel(
                    accountId: accountId,
                    issueId: issueId,
                    db: deps.db,
                    issuesApi: deps.issuesApi,
                    repositoriesApi: deps.repositoriesApi
                )
            }
            // Re-arm on every appear: pushing another screen stops the
            // observation (onDisappear), popping back must resume it.
            viewModel?.startObserving()
        }
        .onDisappear {
            viewModel?.stopObserving()
        }
    }

    @ViewBuilder
    private func content(_ vm: ChangesViewModel) -> some View {
        switch vm.load {
        case .loading:
            HStack(spacing: 8) {
                ProgressView().controlSize(.small).tint(.white)
                Text("Loading changes…")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
        case let .failed(message):
            Text("Couldn't load changes: \(message)")
                .font(.caption)
                .foregroundStyle(DesignTokens.Semantic.red)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 32)
        case let .loaded(files):
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 10) {
                    summaryHeader(issue: vm.issue, files: files)
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
                .padding(.horizontal, 16)
                .padding(.top, 4)
                .padding(.bottom, 24)
            }
        }
    }

    // MARK: - Summary header

    private func summaryHeader(issue: IssueEntity?, files: [PrFile]) -> some View {
        VStack(alignment: .leading, spacing: 8) {
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
                Text("\(files.count) \(files.count == 1 ? "file" : "files")")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text("+\(files.reduce(0) { $0 + $1.additions })")
                    .font(.caption.monospaced())
                    .foregroundStyle(.green)
                Text("−\(files.reduce(0) { $0 + $1.deletions })")
                    .font(.caption.monospaced())
                    .foregroundStyle(.red)
                Spacer()
                if let prUrl = issue?.prUrl, let url = URL(string: prUrl) {
                    Link("Open PR on GitHub", destination: url)
                        .font(.caption)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .glassSection()
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
