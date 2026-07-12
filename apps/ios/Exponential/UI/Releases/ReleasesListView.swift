import ExpUI
import ExpCore
import SwiftUI
import GRDB

/// Workspace Releases list (EXP-56): every release in the workspace, unshipped
/// first (by target date), then shipped (most recent first) — the shared
/// compareReleases contract. Progress is pure client work over the already-
/// synced issues shape (issues.release_id). The root of the Releases tab,
/// showing the current project's workspace.
struct ReleasesListView: View {
    let workspaceId: String
    /// Pushes the release detail after a successful create (AppNavigator owns
    /// the path).
    let onOpenRelease: (String) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var releases: [ReleaseEntity] = []
    @State private var issuesByRelease: [String: [IssueEntity]] = [:]
    @State private var showCreate = false
    @State private var observationTasks: [Task<Void, Never>] = []

    private var sortedReleases: [ReleaseEntity] {
        releases.sorted(by: compareReleases)
    }

    var body: some View {
        ZStack {
            AppBackground()

            if releases.isEmpty {
                emptyState
            } else {
                ScrollView {
                    LazyVStack(spacing: 8) {
                        ForEach(sortedReleases, id: \.id) { release in
                            releaseRow(release)
                        }
                    }
                    .padding(16)
                }
                // A tab root now — the floating bar overlays the bottom (EXP-36).
                .tabBarBottomInset()
            }
        }
        .navigationTitle("Releases")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    showCreate = true
                } label: {
                    Image(systemName: "plus")
                        .font(.body)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .frame(width: 32, height: 32)
                        .contentShape(Circle())
                }
                .accessibilityLabel("New release")
            }
        }
        .sheet(isPresented: $showCreate) {
            CreateReleaseSheet(
                loadCandidates: { await addCandidates() },
                onCreate: { name, issueIds in
                    await createRelease(name: name, issueIds: issueIds)
                }
            )
            .presentationBackground(.ultraThinMaterial)
        }
        .onAppear { startObserving() }
        .onDisappear { stopObserving() }
    }

    // MARK: - Rows

    @ViewBuilder
    private func releaseRow(_ release: ReleaseEntity) -> some View {
        let progress = releaseProgress(issues: issuesByRelease[release.id] ?? [])
        NavigationLink(value: AppRoute.releaseDetail(accountId: accountId, id: release.id)) {
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 8) {
                    Image(systemName: "shippingbox")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    Text(release.name)
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                    ReleaseStatePill(release: release, isComplete: progress.isComplete)
                    Spacer()
                }

                HStack(spacing: 6) {
                    if let targetDate = release.targetDate {
                        HStack(spacing: 3) {
                            Image(systemName: "calendar")
                                .font(.caption2)
                            Text(formatReleaseTargetDate(targetDate))
                                .font(.caption)
                        }
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        Text("·")
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                    Text(progressText(progress))
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    Spacer()
                    ProgressView(value: progress.fraction)
                        .progressViewStyle(.linear)
                        .tint(DesignTokens.Semantic.green)
                        .frame(width: 96)
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
        }
        .buttonStyle(.plain)
    }

    private var emptyState: some View {
        VStack(spacing: 12) {
            Image(systemName: "shippingbox")
                .font(.title2)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text("No releases yet")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Text("Bundle issues into a release to track what ships together and when.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .multilineTextAlignment(.center)

            Button {
                showCreate = true
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: "plus")
                        .font(.caption.weight(.semibold))
                    Text("New release")
                        .font(.subheadline.weight(.medium))
                }
                .foregroundStyle(.white)
                .padding(.horizontal, 16)
                .padding(.vertical, 10)
                .glassButton()
            }
            .buttonStyle(.plain)
        }
        .padding(.horizontal, 40)
    }

    // MARK: - Mutations

    /// Create WITH the creation-time issue bundle (EXP-62): the sheet picks
    /// the issues BEFORE the release exists and the server attaches them in
    /// the same transaction. A nil name lets the server auto-name
    /// (`Release N`). Poll GRDB for the Electric-synced row so the pushed
    /// detail doesn't flash its "Release not found" branch, then navigate
    /// REGARDLESS — the detail shows its loading state until sync lands.
    /// Returns an error message on failure (the sheet renders it inline and
    /// stays open), nil on success.
    private func createRelease(name: String?, issueIds: [String]) async -> String? {
        guard !issueIds.isEmpty else { return nil }
        do {
            let id = try await deps.releasesApi.create(
                accountId: accountId,
                workspaceId: workspaceId,
                name: name,
                issueIds: issueIds
            )
            await waitForSyncedRelease(id: id)
            showCreate = false
            onOpenRelease(id)
            return nil
        } catch {
            return error.localizedDescription
        }
    }

    /// Candidates for the creation sheet's issue picker: the workspace's
    /// still-actionable issues (no release exists yet, so nothing to exclude).
    private func addCandidates() async -> [IssueEntity] {
        await loadAddableReleaseIssues(
            pool: try? deps.db.pool(forAccountId: accountId),
            workspaceId: workspaceId,
            excludingReleaseId: nil
        )
    }

    private func waitForSyncedRelease(id: String) async {
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        // 150ms × 33 ≈ 5s cap.
        for _ in 0..<33 {
            let row = try? await pool.read { db in
                try ReleaseEntity.fetchOne(db, key: id)
            }
            if row != nil { return }
            try? await Task.sleep(nanoseconds: 150_000_000)
        }
    }

    // MARK: - Observation

    private func startObserving() {
        stopObserving()
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let workspaceId = workspaceId

        let releaseObs = ValueObservation.tracking { db in
            try ReleaseEntity
                .filter(Column("workspace_id") == workspaceId)
                .fetchAll(db)
        }
        let releaseTask = Task { @MainActor in
            do {
                for try await rows in releaseObs.values(in: pool) {
                    releases = rows
                }
            } catch {}
        }

        // Issues bundled into ANY release — grouped per release for progress.
        // Cross-workspace bleed is impossible: an issue's release always lives
        // in the issue's own workspace, so grouping by release_id suffices.
        let issueObs = ValueObservation.tracking { db in
            try IssueEntity
                .filter(Column("release_id") != nil)
                .fetchAll(db)
        }
        let issueTask = Task { @MainActor in
            do {
                for try await rows in issueObs.values(in: pool) {
                    issuesByRelease = Dictionary(grouping: rows) { $0.releaseId ?? "" }
                }
            } catch {}
        }

        observationTasks = [releaseTask, issueTask]
    }

    private func stopObserving() {
        for task in observationTasks { task.cancel() }
        observationTasks = []
    }
}
