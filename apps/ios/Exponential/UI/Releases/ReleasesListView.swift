import ExpUI
import ExpCore
import SwiftUI
import GRDB

/// Workspace Releases list (EXP-56): every release in the workspace, unshipped
/// first (by target date), then shipped (most recent first) — the shared
/// compareReleases contract. Progress is pure client work over the already-
/// synced issues shape (issues.release_id). Pushed from the Issues screen's
/// toolbar (the tab bar is full).
struct ReleasesListView: View {
    let workspaceId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var releases: [ReleaseEntity] = []
    @State private var issuesByRelease: [String: [IssueEntity]] = [:]
    @State private var showCreate = false
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
            CreateReleaseSheet { name, description in
                Task { await createRelease(name: name, description: description) }
            }
            .presentationDetents([.medium])
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

    private func createRelease(name: String, description: String?) async {
        do {
            try await deps.releasesApi.create(
                accountId: accountId,
                CreateReleaseInput(workspaceId: workspaceId, name: name, description: description)
            )
            error = nil
        } catch {
            self.error = error.localizedDescription
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
                    issuesByRelease = Dictionary(grouping: rows.filter { $0.releaseId != nil }) {
                        $0.releaseId ?? ""
                    }
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

// MARK: - Shared release display helpers

/// "Shipped <date>" (emerald) when shipped_at is set; "Ready" (outline
/// emerald) when all non-dropped issues are done and the release is unshipped;
/// nothing otherwise. Mirrors the web's ReleaseStatePill.
struct ReleaseStatePill: View {
    let release: ReleaseEntity
    let isComplete: Bool

    var body: some View {
        if release.shippedAt != nil {
            pill(text: shippedText, filled: true)
        } else if isComplete {
            pill(text: "Ready", filled: false)
        }
    }

    private var shippedText: String {
        if let date = parseTimestamp(release.shippedAt) {
            return "Shipped \(AppDateFormatters.MMMd.string(from: date))"
        }
        return "Shipped"
    }

    @ViewBuilder
    private func pill(text: String, filled: Bool) -> some View {
        Text(text)
            .font(.caption2.weight(.medium))
            .foregroundStyle(DesignTokens.Semantic.green)
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(
                Capsule().fill(filled ? DesignTokens.Semantic.green.opacity(0.12) : .clear)
            )
            .overlay(
                Capsule().strokeBorder(DesignTokens.Semantic.green.opacity(0.4), lineWidth: 1)
            )
    }
}

/// "N of M done" — denominator excludes cancelled + duplicate (§10.2).
func progressText(_ progress: ReleaseProgress) -> String {
    progress.total == 0
        ? "No issues"
        : "\(progress.done) of \(progress.denominator) done"
}

func formatReleaseTargetDate(_ dateString: String) -> String {
    guard let date = AppDateFormatters.yyyyMMdd.date(from: dateString) else { return dateString }
    return AppDateFormatters.MMMd.string(from: date)
}

/// Parse a synced ISO-8601 timestamp string (with or without fractional
/// seconds, or the Postgres `yyyy-MM-dd HH:mm:ss+00` form).
func parseTimestamp(_ s: String?) -> Date? {
    guard let s else { return nil }
    let withFractional = ISO8601DateFormatter()
    withFractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    if let date = withFractional.date(from: s) { return date }
    if let date = ISO8601DateFormatter().date(from: s) { return date }
    // Postgres-style "yyyy-MM-dd HH:mm:ss+00" — normalize the space to a T.
    let normalized = s.replacingOccurrences(of: " ", with: "T")
    return withFractional.date(from: normalized) ?? ISO8601DateFormatter().date(from: normalized)
}
