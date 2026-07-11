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
    /// Pushes the release detail after a one-tap create (AppNavigator owns
    /// the path).
    let onOpenRelease: (String) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var releases: [ReleaseEntity] = []
    @State private var issuesByRelease: [String: [IssueEntity]] = [:]
    @State private var creating = false
    @State private var error: String?
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
                        if let error {
                            Text(error)
                                .font(.caption)
                                .foregroundStyle(.red)
                        }
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
                    Task { await createRelease() }
                } label: {
                    Image(systemName: "plus")
                        .font(.body)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .frame(width: 32, height: 32)
                        .contentShape(Circle())
                }
                .disabled(creating)
                .accessibilityLabel("New release")
            }
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
                Task { await createRelease() }
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
            .disabled(creating)
        }
        .padding(.horizontal, 40)
    }

    // MARK: - Mutations

    /// One-tap create: the server auto-names (`Release N`) and returns the id.
    /// Poll GRDB for the Electric-synced row so the pushed detail doesn't
    /// flash its "Release not found" branch, then navigate REGARDLESS — the
    /// detail shows its loading state until sync lands.
    private func createRelease() async {
        guard !creating else { return }
        creating = true
        defer { creating = false }
        do {
            let id = try await deps.releasesApi.create(accountId: accountId, workspaceId: workspaceId)
            error = nil
            await waitForSyncedRelease(id: id)
            onOpenRelease(id)
        } catch {
            self.error = error.localizedDescription
        }
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
