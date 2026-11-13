import ExpCore
import ExpUI
import GRDB
import SwiftUI

/// #EXP-1616 THE REPO PAGE (repo-page.json, view `repo`; web
/// `repo-page.tsx`): the AppBar (back + the repo's full name ONLY; the
/// provider lives in Settings and on the Repositories row) over the pill tabs Pull requests · Runs · Branches · Settings. The PRs
/// are the synced `pull_requests` rows; branches and settings load on demand.
struct RepoPageView: View {
    let teamId: String
    let repositoryId: String
    let isOwner: Bool

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @State private var tab: RepoPage.Tab = .pulls
    /// nil = the first load is in flight (skeletons).
    @State private var repo: TeamRepo?
    @State private var loaded = false
    @State private var prs: [PullRequestEntity] = []

    var body: some View {
        ZStack {
            AppBackground()
            VStack(spacing: 0) {
                tabs
                ScrollView {
                    VStack(alignment: .leading, spacing: 24) {
                        content
                    }
                    .padding(16)
                }
            }
        }
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .principal) {
                Text(repo?.fullName ?? "")
                    .font(.headline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
        }
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .accessibilityIdentifier(RepoPage.TestIds.page)
        .task(id: "\(accountId)/\(repositoryId)") { await loadRepo() }
        .task(id: "\(accountId)/\(repositoryId)/prs") { await observePrs() }
    }

    // MARK: - Tabs

    /// THE pill tabs over `copy.tabs` (the `SessionScreenTabs` pattern: each
    /// segment hugs its label, the strip scrolls sideways past the width).
    private var tabs: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            GlassSegmentedControl(
                options: RepoPage.tabs,
                selection: tab,
                label: \.label,
                identifier: { "repo-tab-\($0.rawValue)" },
                hugging: true,
                onSelect: { tab = $0 }
            )
            .padding(.horizontal, 16)
        }
        .scrollBounceBehavior(.basedOnSize, axes: .horizontal)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.top, 8)
    }

    @ViewBuilder
    private var content: some View {
        if !loaded {
            SkeletonRows(rowHeight: ListItem.prRowPhone)
                .glassSection()
        } else if let repo {
            switch tab {
            case .pulls:
                RepoPullsTab(prs: prs)
            case .runs:
                EmptyState(title: RepoPage.Copy.Runs.empty, size: .list)
            case .branches:
                RepoBranchesTab(repo: repo, prs: prs)
            case .settings:
                RepoSettingsTab(repo: repo, isOwner: isOwner)
            }
        } else {
            EmptyState(title: "Repository not found", icon: AppIcons.settingsRepositories, size: .list)
        }
    }

    // MARK: - Data

    private func loadRepo() async {
        do {
            let repos = try await deps.repositoriesApi.list(accountId: accountId, teamId: teamId)
            repo = repos.first { $0.id == repositoryId }
        } catch {
            toaster.error(FeedbackStates.Errors.title(verb: "load", thing: "the repository"), description: error.trpcUserMessage)
        }
        loaded = true
    }

    private func observePrs() async {
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let id = repositoryId
        let observation = ValueObservation.tracking { db in
            try PullRequestEntity
                .filter(Column("repository_id") == id)
                .order(Column("number").asc)
                .fetchAll(db)
        }
        do {
            for try await rows in observation.removeDuplicates().values(in: pool) {
                prs = rows
            }
        } catch {}
    }
}

// MARK: - Pills

/// The provider `Badge` (`copy.provider`).
struct RepoProviderPill: View {
    let provider: String

    var body: some View {
        GlassPill(RepoPage.Copy.Provider.label(provider))
            .fixedSize()
            .accessibilityIdentifier("repo-provider")
    }
}

/// A checks rollup's `Chip{dot}` (`copy.checks[state]`): its label and its
/// dot in the theme's semantic colour.
struct RepoChecksPill: View {
    let state: String
    /// Phones' PR rows: the dot alone, so titles get the width (the label
    /// stays the accessibility label).
    var dotOnly = false

    var body: some View {
        if let check = RepoPage.Copy.checks(state) {
            GlassPill(dotOnly ? "" : check.label, dot: Self.color(check.tone))
                .fixedSize()
                .accessibilityLabel(check.label)
        }
    }

    static func color(_ tone: RepoPage.Copy.Tone) -> Color {
        switch tone {
        case .green: DesignTokens.Semantic.green
        case .amber: DesignTokens.Semantic.yellow
        case .red: DesignTokens.Semantic.red
        }
    }
}

// MARK: - Pull requests

/// `Section{tree}` of `RepoPage.prTree` rows: THE PR row with its tree
/// guides; trailing the checks chip, then the merge-train status pill.
private struct RepoPullsTab: View {
    let prs: [PullRequestEntity]

    var body: some View {
        let rows = RepoPage.prTree(prs.map(RepoPage.Pr.init))
        if rows.isEmpty {
            EmptyState(title: RepoPage.Copy.Pulls.empty, size: .list)
        } else {
            let byId = Dictionary(prs.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
            let guides = TreeGuides.compute(depths: rows.map(\.depth))
            VStack(alignment: .leading, spacing: 0) {
                ForEach(Array(rows.enumerated()), id: \.element.id) { index, row in
                    PrRow(identifier: row.identifier, title: row.title, subtitle: row.subtitle, guide: guides[index]) {
                        HStack(spacing: 6) {
                            if let checks = row.checks {
                                RepoChecksPill(state: checks, dotOnly: true)
                            }
                            if let pr = byId[row.id] {
                                MergeStatusPill(
                                    view: WorkMergePill.statusPill(pr, stack: false, checks: 0, batchSize: nil, canApprove: false),
                                    action: nil,
                                    identifier: "repo-pr-status"
                                )
                            }
                        }
                    }
                    .accessibilityIdentifier("repo-pr-row")
                }
            }
            .glassSection()
            .accessibilityIdentifier("repo-pulls")
        }
    }
}

// MARK: - Branches

/// `Section` of `RepoPage.branches` rows: the name, the sha7, a Default
/// pill on the default branch and the live PR's `#n`.
private struct RepoBranchesTab: View {
    let repo: TeamRepo
    let prs: [PullRequestEntity]

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @State private var branches: [EcodeBranch]?

    var body: some View {
        Group {
            if let branches {
                let rows = RepoPage.branches(
                    branches.map { (name: $0.name, sha: $0.sha) },
                    defaultBranch: repo.defaultBranch,
                    prs: prs.map(RepoPage.Pr.init)
                )
                if rows.isEmpty {
                    EmptyState(title: RepoPage.Copy.Branches.empty, size: .list)
                } else {
                    VStack(spacing: 0) {
                        ForEach(rows, id: \.name) { row in
                            branchRow(row)
                        }
                    }
                    .glassSection()
                    .accessibilityIdentifier("repo-branches")
                }
            } else {
                SkeletonRows(rowHeight: 52)
                    .glassSection()
            }
        }
        .task(id: repo.id) { await load() }
    }

    private func branchRow(_ row: RepoPage.BranchRow) -> some View {
        HStack(spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
                Text(row.name)
                    .font(.expMono(.subheadline))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .truncationMode(.middle)
                if !row.sha.isEmpty {
                    Text(row.sha)
                        .font(.expMono(.caption))
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                }
            }
            Spacer(minLength: 8)
            if let number = row.prNumber {
                Text("#\(number)")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .fixedSize()
            }
            if row.isDefault {
                GlassPill(RepoPage.Copy.Branches.default)
                    .fixedSize()
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .flatRow()
    }

    private func load() async {
        do {
            if repo.isEcode {
                branches = try await deps.repositoriesApi.branches(accountId: accountId, repositoryId: repo.id)
            } else {
                branches = try await deps.repositoriesApi.listBranches(accountId: accountId, repositoryId: repo.id)
                    .map { EcodeBranch(name: $0, sha: "") }
            }
        } catch {
            branches = []
            toaster.error(FeedbackStates.Errors.title(verb: "load", thing: "the branches"), description: error.trpcUserMessage)
        }
    }
}

// MARK: - Settings

/// `SettingsRow`s: provider, default branch (owners), the clone URL,
/// backups newest first with Restore… (owners), the mirror (owners).
private struct RepoSettingsTab: View {
    let repo: TeamRepo
    let isOwner: Bool

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.toaster) private var toaster
    @State private var settings: EcodeRepoSettings?
    @State private var branches: [String] = []
    @State private var mirrorUrl = ""
    @State private var busy = false
    @State private var restoreTarget: EcodeRepoSettings.Backup?

    var body: some View {
        Group {
            if repo.isEcode && settings == nil {
                SkeletonRows(rowHeight: 52)
                    .glassSection()
            } else {
                VStack(alignment: .leading, spacing: 24) {
                    generalSection
                    if repo.isEcode, let settings {
                        backupsSection(settings)
                        mirrorSection(settings)
                    }
                }
                .accessibilityIdentifier("repo-settings")
            }
        }
        .task(id: repo.id) { await load() }
        // The app's own alert card, on a zero-size node of its own (EXP-240).
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(item: $restoreTarget) { backup in
                    GlassAlert(
                        title: RepoPage.Copy.Settings.restoreTitle,
                        message: nil,
                        actions: [
                            GlassAlertAction(RepoPage.Copy.Add.cancel, role: .outline, isDefault: true, isCancel: true, id: "cancel") {},
                            GlassAlertAction(RepoPage.Copy.Settings.restoreConfirm, role: .destructive, id: "restore") {
                                act(verb: "restore", thing: "the backup") {
                                    try await deps.repositoriesApi.restore(
                                        accountId: accountId, repositoryId: repo.id, backupId: backup.id
                                    )
                                }
                            },
                        ]
                    )
                }
        }
    }

    private var defaultBranch: String { settings?.defaultBranch ?? repo.defaultBranch }

    private var generalSection: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                Text(RepoPage.Copy.Settings.provider)
                    .foregroundStyle(.white)
                Spacer(minLength: 8)
                RepoProviderPill(provider: repo.provider)
            }
            .settingsRowPadding()
            GlassPickerRow(
                RepoPage.Copy.Settings.defaultBranch,
                selection: Binding(
                    get: { defaultBranch },
                    set: { branch in
                        guard branch != defaultBranch else { return }
                        act(verb: "set", thing: "the default branch") {
                            try await deps.repositoriesApi.setDefaultBranch(
                                accountId: accountId, repositoryId: repo.id, branch: branch
                            )
                        }
                    }
                ),
                options: branches.isEmpty ? [defaultBranch] : branches,
                label: { $0 },
                enabled: isOwner && repo.isEcode && !busy
            )
            .settingsRowPadding()
            if let remoteUrl = settings?.remoteUrl, !remoteUrl.isEmpty {
                HStack(spacing: 12) {
                    Text(RepoPage.Copy.Settings.cloneUrl)
                        .foregroundStyle(.white)
                        .fixedSize()
                    Spacer(minLength: 8)
                    Text(remoteUrl)
                        .font(.expMono(.caption))
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .textSelection(.enabled)
                }
                .settingsRowPadding()
            }
        }
        .glassSection()
    }

    private func backupsSection(_ settings: EcodeRepoSettings) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            GlassSectionBand(RepoPage.Copy.Settings.backups)
            Group {
                if settings.backups.isEmpty {
                    EmptyState(title: RepoPage.Copy.Settings.noBackups, size: .list)
                } else {
                    VStack(spacing: 0) {
                        ForEach(settings.backups) { backup in
                            HStack(spacing: 12) {
                                Text(Self.when(backup.createdAt))
                                    .foregroundStyle(.white)
                                    .lineLimit(1)
                                Spacer(minLength: 8)
                                Text(Self.bytes(backup.bytes))
                                    .font(.caption)
                                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                    .fixedSize()
                                if isOwner {
                                    GlassPill(
                                        RepoPage.Copy.Settings.restore,
                                        mode: .action { restoreTarget = backup },
                                        enabled: !busy
                                    )
                                    .fixedSize()
                                    .accessibilityIdentifier("repo-backup-restore")
                                }
                            }
                            .settingsRowPadding()
                        }
                    }
                }
            }
            .glassSection()
            .accessibilityIdentifier("repo-backups")
        }
    }

    private func mirrorSection(_ settings: EcodeRepoSettings) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            GlassSectionBand(RepoPage.Copy.Settings.mirror)
            VStack(spacing: 0) {
                GlassToggleRow(
                    RepoPage.Copy.Add.mirror,
                    isOn: Binding(
                        get: { settings.mirrorEnabled },
                        set: { enabled in saveMirror(enabled: enabled) }
                    )
                )
                .disabled(!isOwner || busy || mirrorUrl.trimmingCharacters(in: .whitespaces).isEmpty)
                .settingsRowPadding()
                HStack(spacing: 12) {
                    Text(RepoPage.Copy.Settings.mirrorUrl)
                        .foregroundStyle(.white)
                        .fixedSize()
                    GlassTextField(
                        "https://github.com/owner/name",
                        text: $mirrorUrl,
                        bordered: false,
                        accessibilityIdentifier: "repo-mirror-url"
                    )
                    .multilineTextAlignment(.trailing)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .keyboardType(.URL)
                    .disabled(!isOwner || busy)
                    .onSubmit {
                        let trimmed = mirrorUrl.trimmingCharacters(in: .whitespaces)
                        if trimmed != (settings.mirrorUrl ?? "") { saveMirror(enabled: settings.mirrorEnabled) }
                    }
                }
                .settingsRowPadding()
            }
            .glassSection()
        }
    }

    private func saveMirror(enabled: Bool) {
        let trimmed = mirrorUrl.trimmingCharacters(in: .whitespaces)
        act(verb: "save", thing: "the mirror") {
            try await deps.repositoriesApi.setMirror(
                accountId: accountId, repositoryId: repo.id, mirrorUrl: trimmed.isEmpty ? nil : trimmed, enabled: enabled
            )
        }
    }

    // MARK: - Data

    private func load() async {
        guard repo.isEcode else { return }
        do {
            async let next = deps.repositoriesApi.settings(accountId: accountId, repositoryId: repo.id)
            async let list = deps.repositoriesApi.branches(accountId: accountId, repositoryId: repo.id)
            let (loadedSettings, loadedBranches) = try await (next, list)
            settings = loadedSettings
            mirrorUrl = loadedSettings.mirrorUrl ?? ""
            branches = loadedBranches.map(\.name)
        } catch {
            toaster.error(
                FeedbackStates.Errors.title(verb: "load", thing: "the repository settings"),
                description: error.trpcUserMessage
            )
        }
    }

    /// A failed action = a toast with Retry; a success reloads.
    private func act(verb: String, thing: String, _ operation: @escaping @MainActor () async throws -> Void) {
        busy = true
        Task {
            defer { busy = false }
            do {
                try await operation()
                await load()
            } catch {
                toaster.error(
                    FeedbackStates.Errors.title(verb: verb, thing: thing),
                    description: error.trpcUserMessage,
                    action: ToastAction(label: FeedbackStates.Errors.retry) {
                        Task { @MainActor in act(verb: verb, thing: thing, operation) }
                    }
                )
            }
        }
    }

    static func when(_ wire: String) -> String {
        guard let date = WireTimestamps.parse(wire) else { return wire }
        return date.formatted(date: .abbreviated, time: .shortened)
    }

    /// Web `formatBytes`.
    static func bytes(_ bytes: Int) -> String {
        if bytes < 1024 { return "\(bytes) B" }
        if bytes < 1024 * 1024 { return String(format: "%.1f KB", Double(bytes) / 1024) }
        return String(format: "%.1f MB", Double(bytes) / (1024 * 1024))
    }
}

private extension View {
    /// A settings row's inset (the repositories list rhythm).
    func settingsRowPadding() -> some View {
        padding(.horizontal, 12)
            .padding(.vertical, 10)
    }
}
