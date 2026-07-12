import ExpUI
import ExpCore
import SwiftUI
import GRDB

/// Release detail (EXP-56): header (name, target date, shipped state, PR
/// pill), ship/unship, delete (confirmed), and the release's issues grouped by
/// status like the project board. Rows navigate to the issue detail; the
/// toolbar "+" opens the multi-select add-issues sheet; the swipe/context
/// action unbundles a row (setIssueRelease null — issues survive). Mobile
/// never launches coding — no run affordance here.
struct ReleaseDetailView: View {
    let releaseId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss
    @State private var release: ReleaseEntity?
    @State private var issues: [IssueEntity] = []
    @State private var vanished = false
    @State private var showAddIssues = false
    @State private var showDeleteConfirm = false
    @State private var error: String?
    @State private var observationTasks: [Task<Void, Never>] = []
    @State private var vanishGraceTask: Task<Void, Never>?

    private var progress: ReleaseProgress {
        releaseProgress(issues: issues)
    }

    private func issuesForStatus(_ status: IssueStatus) -> [IssueEntity] {
        issues
            .filter { $0.status == status.rawValue && $0.archivedAt == nil }
            .sorted { ($0.sortOrder ?? 0) < ($1.sortOrder ?? 0) }
    }

    var body: some View {
        ZStack {
            AppBackground()

            if let release {
                List {
                    Section {
                        headerCard(release)
                            .listRowBackground(Color.clear)
                            .listRowSeparator(.hidden)
                            .listRowInsets(EdgeInsets(top: 8, leading: 16, bottom: 8, trailing: 16))
                    }

                    if issues.isEmpty {
                        Section {
                            emptyIssues
                                .listRowBackground(Color.clear)
                                .listRowSeparator(.hidden)
                                .listRowInsets(EdgeInsets(top: 24, leading: 16, bottom: 8, trailing: 16))
                        }
                    } else {
                        // 'Issues N' section header fronting the
                        // status-grouped list (EXP-62 redesign).
                        Section {
                            HStack(spacing: 6) {
                                Text("Issues")
                                    .font(.subheadline.weight(.semibold))
                                    .foregroundStyle(.white)
                                Text("\(issues.count)")
                                    .font(.caption)
                                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                Spacer()
                            }
                            .listRowBackground(Color.clear)
                            .listRowSeparator(.hidden)
                            .listRowInsets(EdgeInsets(top: 14, leading: 20, bottom: 2, trailing: 16))
                        }

                        // Status-grouped issues — the same grouping/order the
                        // project board list uses (IssueStatus.displayOrder,
                        // empty groups hidden).
                        ForEach(IssueStatus.displayOrder, id: \.self) { status in
                            let statusIssues = issuesForStatus(status)
                            if !statusIssues.isEmpty {
                                Section {
                                    ForEach(statusIssues, id: \.id) { issue in
                                        issueRow(issue)
                                            .listRowBackground(Color.clear)
                                            .listRowSeparator(.hidden)
                                            .listRowInsets(EdgeInsets(top: 1.5, leading: 16, bottom: 1.5, trailing: 16))
                                            .swipeActions(edge: .trailing, allowsFullSwipe: false) {
                                                Button(role: .destructive) {
                                                    Task { await removeFromRelease(issue) }
                                                } label: {
                                                    Label("Remove", systemImage: "xmark.circle.fill")
                                                }
                                            }
                                            .contextMenu {
                                                Button(role: .destructive) {
                                                    Task { await removeFromRelease(issue) }
                                                } label: {
                                                    Label("Remove from release", systemImage: "xmark.circle")
                                                }
                                            }
                                    }
                                } header: {
                                    statusHeader(status: status, count: statusIssues.count)
                                        .listRowInsets(EdgeInsets(top: 6, leading: 16, bottom: 2, trailing: 16))
                                        .listRowBackground(Color.clear)
                                }
                            }
                        }
                    }
                }
                .listStyle(.plain)
                .contentMargins(.horizontal, 0, for: .scrollContent)
                .contentMargins(.top, 0, for: .scrollContent)
                .environment(\.defaultMinListRowHeight, 0)
                .listSectionSpacing(0)
                .scrollContentBackground(.hidden)
                .background(Color.clear)
            } else if vanished {
                VStack(spacing: 12) {
                    Image(systemName: "shippingbox")
                        .font(.title2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    Text("Release not found")
                        .font(.subheadline)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    Text("This release may have been deleted.")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                .padding(.horizontal, 40)
            } else {
                ProgressView().tint(.white)
            }
        }
        .navigationTitle("Release")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .toolbar {
            if let release {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        showAddIssues = true
                    } label: {
                        Image(systemName: "plus")
                    }
                    .accessibilityLabel("Add issues")
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        Task { await toggleShipped(release) }
                    } label: {
                        Text(release.shippedAt == nil ? "Mark shipped" : "Unship")
                            .font(.subheadline.weight(.medium))
                    }
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Menu {
                        Button("Delete release", role: .destructive) {
                            showDeleteConfirm = true
                        }
                    } label: {
                        Image(systemName: "ellipsis.circle")
                    }
                }
            }
        }
        .sheet(isPresented: $showAddIssues) {
            AddIssuesSheet(
                loadCandidates: { await addCandidates() },
                onConfirm: { ids in
                    Task { await addIssues(ids) }
                }
            )
            .presentationBackground(.ultraThinMaterial)
        }
        .alert("Delete Release", isPresented: $showDeleteConfirm) {
            Button("Delete", role: .destructive) {
                Task {
                    if await deleteRelease() {
                        dismiss()
                    }
                }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Its issues are kept — they just leave the release. This cannot be undone.")
        }
        .onAppear { startObserving() }
        .onDisappear { stopObserving() }
    }

    // MARK: - Header

    /// Header card (EXP-62 redesign): badge + title with state/target chips,
    /// description, then a labeled progress block and the PR chip — a clear
    /// hierarchy instead of loose stacked rows (Android/desktop parity).
    @ViewBuilder
    private func headerCard(_ release: ReleaseEntity) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .top, spacing: 12) {
                ZStack {
                    RoundedRectangle(cornerRadius: 10)
                        .fill(DesignTokens.Semantic.green.opacity(0.12))
                    Image(systemName: "shippingbox")
                        .font(.body)
                        .foregroundStyle(DesignTokens.Semantic.green)
                }
                .frame(width: 40, height: 40)

                VStack(alignment: .leading, spacing: 4) {
                    Text(release.name)
                        .font(.title3.weight(.semibold))
                        .foregroundStyle(.white)
                        .lineLimit(2)
                    HStack(spacing: 8) {
                        ReleaseStatePill(release: release, isComplete: progress.isComplete)
                        if let targetDate = release.targetDate {
                            metaChip(icon: "calendar", text: "Target \(formatReleaseTargetDate(targetDate))")
                        }
                    }
                }
                Spacer(minLength: 0)
            }

            if let description = release.description, !description.isEmpty {
                Text(description)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .padding(.top, 12)
            }

            HStack {
                Text("Progress")
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Spacer()
                Text(progressText(progress))
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .padding(.top, 14)

            ProgressView(value: progress.fraction)
                .progressViewStyle(.linear)
                .tint(DesignTokens.Semantic.green)
                .padding(.top, 6)

            if release.prUrl != nil {
                prPill(release)
                    .padding(.top, 12)
            }

            if let error {
                Text(error)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .padding(.top, 8)
            }
        }
        .padding(16)
        .glassSection()
    }

    /// Small glass meta chip: optional icon + label (target date etc.).
    @ViewBuilder
    private func metaChip(icon: String?, text: String) -> some View {
        HStack(spacing: 4) {
            if let icon {
                Image(systemName: icon)
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            Text(text)
                .font(.caption2)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 3)
        .glassButton()
    }

    /// The release PR (integration branch → default), linking out to GitHub
    /// when set. Merged renders purple, otherwise green (web parity).
    @ViewBuilder
    private func prPill(_ release: ReleaseEntity) -> some View {
        if let prUrl = release.prUrl, let url = URL(string: prUrl) {
            let merged = release.prState == DomainContract.prStateMerged
            Link(destination: url) {
                HStack(spacing: 4) {
                    Image(systemName: merged ? "arrow.triangle.merge" : "arrow.triangle.branch")
                        .font(.caption2)
                        .foregroundStyle(merged ? Accent.indigo : DesignTokens.Semantic.green)
                    if let prNumber = release.prNumber {
                        Text("#\(prNumber)")
                            .font(.caption.monospaced())
                            .foregroundStyle(.white)
                    }
                    if let prState = release.prState {
                        Text(prState.capitalized)
                            .font(.caption2)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                }
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .glassButton()
            }
        }
    }

    private var emptyIssues: some View {
        VStack(spacing: 8) {
            Image(systemName: "checklist")
                .font(.title3)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text("No issues in this release")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Text("Tap + to add issues to this release.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity)
    }

    // MARK: - Issue rows (project-board look)

    @ViewBuilder
    private func statusHeader(status: IssueStatus, count: Int) -> some View {
        HStack(spacing: 8) {
            Image(systemName: status.sfSymbol)
                .font(.caption)
                .foregroundStyle(status.color)
            Text(status.label)
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

    @ViewBuilder
    private func issueRow(_ issue: IssueEntity) -> some View {
        NavigationLink(value: AppRoute.issue(accountId: accountId, id: issue.id)) {
            HStack(spacing: 10) {
                Image(systemName: IssuePriority.from(issue.priority).sfSymbol)
                    .font(.caption)
                    .foregroundStyle(IssuePriority.from(issue.priority).color)
                    .frame(width: 16)

                Text(issue.identifier ?? "")
                    .font(.caption.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                    .frame(minWidth: 60, alignment: .leading)

                Image(systemName: IssueStatus.from(issue.status).sfSymbol)
                    .font(.caption)
                    .foregroundStyle(IssueStatus.from(issue.status).color)
                    .frame(width: 16)

                Text(issue.title)
                    .font(.subheadline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .layoutPriority(1)

                Spacer()
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
        }
        .buttonStyle(.plain)
    }

    // MARK: - Mutations

    private func toggleShipped(_ release: ReleaseEntity) async {
        do {
            try await deps.releasesApi.markShipped(
                accountId: accountId,
                id: release.id,
                shipped: release.shippedAt == nil
            )
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func deleteRelease() async -> Bool {
        do {
            try await deps.releasesApi.delete(accountId: accountId, id: releaseId)
            return true
        } catch {
            self.error = error.localizedDescription
            return false
        }
    }

    private func removeFromRelease(_ issue: IssueEntity) async {
        do {
            try await deps.releasesApi.setIssueRelease(
                accountId: accountId,
                issueId: issue.id,
                releaseId: nil
            )
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func addIssues(_ ids: [String]) async {
        guard !ids.isEmpty else { return }
        do {
            try await deps.releasesApi.addIssues(
                accountId: accountId,
                releaseId: releaseId,
                issueIds: ids
            )
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }

    /// Candidates for the add-issues sheet: the shared loader over this
    /// release's workspace, excluding issues already in THIS release
    /// (other-release issues stay offered — the server records both timeline
    /// sides). One-shot read — the sheet is transient.
    private func addCandidates() async -> [IssueEntity] {
        guard let release else { return [] }
        return await loadAddableReleaseIssues(
            pool: try? deps.db.pool(forAccountId: accountId),
            workspaceId: release.workspaceId,
            excludingReleaseId: releaseId
        )
    }

    // MARK: - Observation

    private func startObserving() {
        stopObserving()
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let releaseId = releaseId

        let releaseObs = ValueObservation.tracking { db in
            try ReleaseEntity.fetchOne(db, key: releaseId)
        }
        let releaseTask = Task { @MainActor in
            do {
                for try await row in releaseObs.values(in: pool) {
                    if let row {
                        vanishGraceTask?.cancel()
                        vanishGraceTask = nil
                        release = row
                        vanished = false
                    } else if release != nil {
                        // It synced once and then disappeared — deleted
                        // (possibly from another client).
                        release = nil
                        vanished = true
                    } else if vanishGraceTask == nil {
                        // Never synced yet: a just-created release can lag the
                        // list's 5s poll on a slow link, and the creator must
                        // see the spinner — not "Release not found". Flip only
                        // after a grace window with still no row (self-heals
                        // either way once the row lands).
                        vanishGraceTask = Task { @MainActor in
                            try? await Task.sleep(nanoseconds: 8_000_000_000)
                            guard !Task.isCancelled, release == nil else { return }
                            vanished = true
                        }
                    }
                }
            } catch {}
        }

        let issueObs = ValueObservation.tracking { db in
            try IssueEntity
                .filter(Column("release_id") == releaseId)
                .fetchAll(db)
        }
        let issueTask = Task { @MainActor in
            do {
                for try await rows in issueObs.values(in: pool) {
                    issues = rows
                }
            } catch {}
        }

        observationTasks = [releaseTask, issueTask]
    }

    private func stopObserving() {
        for task in observationTasks { task.cancel() }
        observationTasks = []
        vanishGraceTask?.cancel()
        vanishGraceTask = nil
    }
}
