import ExpUI
import ExpCore
import GRDB
import SwiftUI

/// A board's page (#EXP-2109: reached through Teams › a team): the detail
/// header with the board's glyph and name, then EXP-1373 / #EXP-1836
/// (chats.json `shell.boardChips`) All issues · My issues · Drafts (only while
/// there are drafts) as THE session screen's segmented tab strip; My issues =
/// the viewer's assigned issues on THIS board (`Chats.boardMineIssues`),
/// grouped by status like the board.
struct BoardPageView: View {
    let boardId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(TeamState.self) private var teamState
    @Environment(\.accountId) private var accountId
    @Environment(\.scenePhase) private var scenePhase
    // EXP-698 r5: the getting-started checklist's live state, published into
    // the environment for the empty board. ONE box for this view's lifetime: a
    // fresh one per body pass would invalidate every reader on every render.
    @State private var gettingStartedContext = GettingStartedContext()
    @State private var createBoardOpen = false
    @State private var chip: Chats.Shell.BoardChip = .all
    @State private var myIssues: MyIssuesViewModel?
    @State private var drafts: DraftsViewModel?
    @Environment(\.pushRoute) private var pushRoute

    private var gettingStarted: GettingStartedProgress { gettingStartedContext.progress }

    private var board: BoardEntity? { teamState.boards.first { $0.id == boardId } }

    var body: some View {
        ZStack {
            AppBackground()

            if shownChip == .mine, let myIssues {
                MyIssuesListContent(model: myIssues)
            } else if shownChip == .drafts, let drafts {
                DraftsListContent(viewModel: drafts)
            } else {
                IssueListView(boardId: boardId)
            }
        }
        .safeAreaInset(edge: .top, spacing: 0) { boardChips }
        .environment(\.gettingStarted, gettingStartedContext)
        .pageHeader(
            board?.name ?? "", pushed: true,
            glyph: board.map { .icon(BoardTypeDisplay.iconName(for: $0), color: $0.color ?? "#888888") }
        )
        .onAppear {
            gettingStartedContext.onCreateBoard = { createBoardOpen = true }
            // The New issue page, the share sheet and the intents default to
            // the last board shown.
            SharedBoardMirror.writeLastUsed(accountId: accountId, boardId: boardId)
            if myIssues == nil {
                myIssues = MyIssuesViewModel(accountId: accountId, db: deps.db, auth: deps.auth)
            }
            if drafts == nil {
                drafts = DraftsViewModel(accountId: accountId, db: deps.db, api: deps.issueDraftsApi)
            }
            myIssues?.boardId = boardId
            drafts?.teamId = board?.teamId
            myIssues?.startObserving()
            drafts?.startObserving()
        }
        .onDisappear {
            myIssues?.stopObserving()
            drafts?.stopObserving()
        }
        .onChange(of: board?.teamId) { _, teamId in
            drafts?.teamId = teamId
        }
        // The GitHub install state is a server-only read (§refreshGithubStatus)
        // — a transient failure at bind would otherwise leave the step
        // "available" for the whole session, and connecting the App happens
        // out in Safari anyway, so re-read it on every foreground.
        .onChange(of: scenePhase) { _, phase in
            if phase == .active { gettingStarted.refreshGithubStatus() }
        }
        .task(id: "\(accountId)/\(board?.teamId ?? "")") {
            guard let teamId = board?.teamId else {
                gettingStarted.stop()
                return
            }
            gettingStarted.bind(accountId: accountId, teamId: teamId, deps: deps)
        }
        // #EXP-1830: a My issues widget's header lands on the chip.
        .onChange(of: deps.deepLinkBus.pendingMyIssues, initial: true) { _, pending in
            guard pending else { return }
            deps.deepLinkBus.pendingMyIssues = false
            chip = .mine
        }
        .sheet(isPresented: $createBoardOpen) {
            if let teamId = board?.teamId {
                CreateBoardSheet(accountId: accountId, teamId: teamId) { created in
                    pushRoute(.board(accountId: accountId, id: created))
                }
            }
        }
    }

    // MARK: - Board chips (EXP-1373)

    private var draftCount: Int { drafts?.rows.count ?? 0 }

    /// Drafts only while there are drafts; a stale Drafts pick falls back.
    private var chips: [Chats.Shell.BoardChip] {
        Chats.Shell.boardChips.filter { $0 != .drafts || draftCount > 0 }
    }

    private var shownChip: Chats.Shell.BoardChip {
        chips.contains(chip) ? chip : .all
    }

    /// The count after My issues and Drafts (none after All issues or at zero).
    private func chipCount(_ chip: Chats.Shell.BoardChip) -> Int? {
        let count: Int? = switch chip {
        case .all: nil
        case .mine: myIssues?.count
        case .drafts: draftCount
        }
        return count.flatMap { $0 > 0 ? $0 : nil }
    }

    /// #EXP-1836: THE session screen's tab strip (`SessionScreenTabs`' own
    /// `GlassSegmentedControl`, hugging, left-aligned, scrolling past the
    /// width), each count after its label as a muted number.
    private var boardChips: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            GlassSegmentedControl(
                options: chips,
                selection: shownChip,
                label: \.label,
                identifier: { "\(Chats.TestIds.boardChip)-\($0.rawValue)" },
                accessory: { option in
                    chipCount(option).map { count in
                        AnyView(
                            Text("\(count)")
                                .font(.subheadline.weight(.medium).monospacedDigit())
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                .lineLimit(1)
                        )
                    }
                },
                accessoryGap: 4,
                spokenLabel: { option in chipCount(option).map { "\(option.label) \($0)" } },
                hugging: true,
                onSelect: { chip = $0 }
            )
            .padding(.horizontal, 16)
        }
        .scrollBounceBehavior(.basedOnSize, axes: .horizontal)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.top, 6)
        .padding(.bottom, 8)
    }
}
