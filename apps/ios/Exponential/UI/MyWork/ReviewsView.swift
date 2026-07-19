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
    @State private var viewModel: ReviewsViewModel?
    @State private var mergeTarget: ReviewEntry?
    @State private var mergeError: String?

    var body: some View {
        let groups = viewModel?.groups(teamId: teamState.activeTeam?.id) ?? []
        Group {
            if viewModel == nil {
                Color.clear
            } else if groups.isEmpty {
                emptyState
            } else {
                reviewList(groups)
            }
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
        .alert(
            "Couldn't merge",
            isPresented: Binding(
                get: { mergeError != nil },
                set: { if !$0 { mergeError = nil } }
            )
        ) {
            Button("OK", role: .cancel) { mergeError = nil }
        } message: {
            Text(mergeError ?? "")
        }
    }

    private var emptyState: some View {
        VStack(spacing: 12) {
            Spacer()
            Image(systemName: IssueStatus.inReview.sfSymbol)
                .font(.title2)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text("No open pull requests")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Spacer()
        }
        .frame(maxWidth: .infinity)
    }

    @ViewBuilder
    private func reviewList(_ groups: [ReviewGroup]) -> some View {
        List {
            ForEach(groups) { group in
                Section {
                    ForEach(group.entries) { entry in
                        entryRow(entry)
                            .listRowBackground(Color.clear)
                            .listRowSeparator(.hidden)
                            .listRowInsets(EdgeInsets(top: 1.5, leading: 16, bottom: 1.5, trailing: 16))
                    }
                } header: {
                    boardHeader(board: group.board, count: group.entries.count)
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

    @ViewBuilder
    private func entryRow(_ entry: ReviewEntry) -> some View {
        // The Review detail (the diff + Merge/Close screen) is what a reviewer
        // wants first (EXP-168); the issue itself is one tap away in the menu.
        NavigationLink(value: AppRoute.changes(accountId: accountId, issueId: entry.representative.id)) {
            HStack(alignment: .top, spacing: 10) {
                // PR glyph — the in_review status icon, green (EXP-120/131).
                Image(systemName: IssueStatus.inReview.sfSymbol)
                    .font(.caption)
                    .foregroundStyle(IssueStatus.inReview.color)
                    .frame(width: 16)
                    .padding(.top, 1)

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
                        HStack(spacing: 3) {
                            Image(systemName: "arrow.triangle.branch")
                                .font(.caption2)
                            Text(branch)
                                .font(.caption.monospaced())
                                .lineLimit(1)
                        }
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                }

                Spacer(minLength: 0)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
        }
        .buttonStyle(.plain)
        .swipeActions(edge: .trailing) {
            Button { mergeTarget = entry } label: {
                Label("Merge", systemImage: "arrow.triangle.merge")
            }
            .tint(.green)
        }
        .contextMenu {
            Button {
                deps.deepLinkBus.navigateToIssue(entry.representative.id)
            } label: {
                Label("Open issue", systemImage: "doc.text")
            }
            Button {
                mergeTarget = entry
            } label: {
                Label("Merge PR", systemImage: "arrow.triangle.merge")
            }
            if let url = prURL(entry) {
                Button {
                    openURL(url)
                } label: {
                    Label("Open PR on GitHub", systemImage: "arrow.up.right.square")
                }
            }
        }
    }

    private func prURL(_ entry: ReviewEntry) -> URL? {
        guard let prUrl = entry.prUrl else { return nil }
        return URL(string: prUrl)
    }

    private func mergeMessage(_ entry: ReviewEntry) -> String {
        let pr = entry.prNumber.map { "#\($0)" } ?? "this pull request"
        var message = "Squash-merges PR \(pr) via the GitHub App."
        if entry.isBatch {
            message += " Completes all \(entry.issues.count) linked issues."
        }
        return message
    }

    private func merge(_ entry: ReviewEntry) {
        mergeTarget = nil
        let issueId = entry.representative.id
        Task {
            do {
                try await deps.issuesApi.mergePr(accountId: accountId, issueId: issueId)
            } catch {
                mergeError = error.localizedDescription
            }
        }
    }
}
