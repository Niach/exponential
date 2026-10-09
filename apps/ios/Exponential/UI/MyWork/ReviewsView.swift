import ExpUI
import ExpCore
import SwiftUI

/// "Reviews" (EXP-131): the open PRs awaiting review across EVERY member team
/// (EXP-1186, cross-team like the Inbox), grouped by board — the board band
/// names its team, quietly, once the caller is in several. Its own
/// bottom-bar destination beside My Work (EXP-147).
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

/// The bare list. EXP-1248: THE `PrRow` ×4 — ONE line per pull request (ring
/// lead · mono identifier · title), a PR TREE nested with tree guides, a
/// linear STACK on one rail down to its base branch (the word "stack" on its
/// top row), single PRs flat. The page only OPENS things: an issue PR opens
/// the issue's Guide, a run's own PR the run's Guide, an unlinked PR GitHub
/// (a muted external-link glyph). No Merge, no swipe, no context menu, no
/// counts on the bands — merging lives on the Guide's merge control.
struct ReviewsListContent: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(TeamState.self) private var teamState
    @Environment(\.openURL) private var openURL
    @Environment(\.pushRoute) private var pushRoute
    @State private var viewModel: ReviewsViewModel?
    /// EXP-1244: bumped on every appear so the openPulls fetch re-runs.
    @State private var appearTick = 0

    var body: some View {
        // EXP-734: agent runs parking their OWN pull request belong to no
        // board, so they get their own bands after the board bands — one per
        // team (EXP-1186); EXP-1244: the pull requests nothing links follow
        // as repository bands. `ReviewsQueue.build` decides it all.
        let snapshot = viewModel?.snapshot(teams: teamState.teams) ?? ReviewsSnapshot()
        Group {
            if viewModel == nil {
                Color.clear
            } else if snapshot.isEmpty {
                emptyState
            } else {
                reviewList(snapshot)
            }
        }
        // EXP-1244: one openPulls fetch per team, again whenever the team
        // set changes (and on every appear: the id includes the appear tick).
        .task(id: PullsFetchKey(teamIds: teamState.teams.map(\.id).sorted(), tick: appearTick)) {
            guard let viewModel else { return }
            await viewModel.refreshPulls(teamIds: teamState.teams.map(\.id))
        }
        .onAppear {
            if viewModel == nil {
                viewModel = ReviewsViewModel(
                    accountId: accountId, db: deps.db, openPulls: deps.openPulls,
                    repositoriesApi: deps.repositoriesApi
                )
            }
            appearTick += 1
            // Re-arm on every appear: pushing an issue detail stops the
            // observation (onDisappear), popping back must resume it.
            viewModel?.startObserving()
        }
        .onDisappear {
            viewModel?.stopObserving()
        }
    }

    private var emptyState: some View {
        VStack(spacing: 12) {
            Spacer()
            AppIcon(AppIcons.prOpen, size: 22)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text("No open pull requests")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Spacer()
        }
        .frame(maxWidth: .infinity)
    }

    private func reviewList(_ snapshot: ReviewsSnapshot) -> some View {
        // EXP-1186: the team name rides the bands only when there is more
        // than one team to tell apart.
        let multiTeam = TeamGroups.isMultiTeam(teamState.teams)
        return ScrollView {
            LazyVStack(alignment: .leading, spacing: 12) {
                ForEach(snapshot.groups) { group in
                    VStack(alignment: .leading, spacing: 0) {
                        GlassSectionBand(group.board.name) {
                            // The board glyph tinted with the board colour
                            // (EXP-449).
                            AppIcon(BoardTypeDisplay.iconName(for: group.board), size: 13)
                                .foregroundStyle(Color(hex: group.board.color ?? "#888888") ?? .gray)
                        } trailing: {
                            bandCaption(multiTeam ? group.team.name : nil)
                        }
                        ForEach(group.blocks) { block in
                            boardBlock(block, board: group.board)
                        }
                    }
                    .accessibilityIdentifier("reviews-board-\(group.board.id)")
                }

                // EXP-734: the runs' own pull requests — they belong to no
                // board. EXP-1186: one band per team.
                ForEach(snapshot.runs) { group in
                    VStack(alignment: .leading, spacing: 0) {
                        GlassSectionBand("Agent runs") {
                            AppIcon(AppIcons.navActions, size: 13)
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        } trailing: {
                            bandCaption(multiTeam ? group.team.name : ReviewsQueue.runBandCaption)
                        }
                        PrList(rows: group.items.map(runRow))
                    }
                }

                // EXP-1244: the open pull requests nothing links, one band per
                // team repository; a row opens GitHub.
                ForEach(snapshot.repos) { repo in
                    VStack(alignment: .leading, spacing: 0) {
                        GlassSectionBand(repo.fullName) {
                            AppIcon(AppIcons.prOpen, size: 13)
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        } trailing: {
                            bandCaption(multiTeam ? repo.team.name : ReviewsQueue.repoBandCaption)
                        }
                        PrList(rows: repo.pulls.map { pullRow($0, repo: repo) })
                    }
                }
            }
            .padding(.horizontal, 16)
            .padding(.top, 8)
            .padding(.bottom, 16)
        }
        .scrollContentBackground(.hidden)
        .background(Color.clear)
        .tabBarBottomInset()
    }

    /// The band's quiet trailing caption (the team on a multi-team list, else
    /// what the band holds). Never a count (EXP-1248).
    @ViewBuilder
    private func bandCaption(_ text: String?) -> some View {
        if let text {
            Text(text)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .lineLimit(1)
        }
    }

    /// One drawn block of a board band: a PR list (trees nest) or a stack
    /// rail over its base branch.
    @ViewBuilder
    private func boardBlock(_ block: ReviewsQueue.Block, board: BoardEntity) -> some View {
        switch block {
        case let .list(rows):
            PrList(rows: rows.map { row in
                let label = ReviewsQueue.reviewRowLabel(row.entry)
                return PrListRow(
                    id: row.entry.key,
                    identifier: label.identifier,
                    title: label.title,
                    depth: row.depth,
                    onOpen: { openGuide(row.entry) }
                )
            })
        case let .stack(entries, baseBranch):
            let byKey = Dictionary(entries.map { ($0.key, $0) }, uniquingKeysWith: { a, _ in a })
            StackRail(
                members: entries.map { entry in
                    let label = ReviewsQueue.reviewRowLabel(entry)
                    return StackRailMember(id: entry.key, identifier: label.identifier, title: label.title)
                },
                baseBranch: baseBranch ?? board.defaultBranch ?? "default branch",
                word: "stack",
                onOpen: { member in
                    if let entry = byKey[member.id] { openGuide(entry) }
                }
            )
        }
    }

    /// An issue PR opens the representative issue's Guide (EXP-1251).
    private func openGuide(_ entry: ReviewsQueue.Entry) {
        pushRoute(.issueFace(accountId: accountId, id: entry.representative.id, face: .guide))
    }

    /// EXP-734: one agent run's own pull request — `#N` + the run's name; it
    /// opens OUR review of it (EXP-1194).
    private func runRow(_ entry: RunReviewEntry) -> PrListRow {
        PrListRow(
            id: "run:\(entry.id)",
            identifier: entry.prNumber.map { "#\($0)" },
            title: entry.title,
            onOpen: {
                pushRoute(.runChanges(accountId: accountId, sessionId: entry.session.id))
            }
        )
    }

    /// EXP-1244: one open pull request nothing links — `#N` + its title, the
    /// word "draft" on a draft, a muted external-link glyph; it opens GitHub.
    private func pullRow(_ pull: OpenPull, repo: RepoReviewGroup) -> PrListRow {
        PrListRow(
            id: "pull:\(repo.repositoryId):\(pull.number)",
            identifier: "#\(pull.number)",
            title: pull.title,
            word: pull.draft ? "draft" : nil,
            external: true,
            onOpen: {
                if let url = URL(string: pull.url) { openURL(url) }
            }
        )
    }
}

/// EXP-1244: re-runs the openPulls fetch when the team set changes or the
/// list re-appears.
private struct PullsFetchKey: Equatable {
    let teamIds: [String]
    let tick: Int
}
