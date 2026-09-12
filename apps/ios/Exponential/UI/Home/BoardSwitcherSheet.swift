import ExpUI
import ExpCore
import SwiftUI

/// Bottom-sheet board picker for the Issues tab's inline switcher: the
/// server → team → board tree that used to be the Boards overview
/// screen, now presented modally. Selecting a board swaps the Issues tab's
/// list in place (the caller writes last-used and dismisses).
/// EXP-778: which account/team's pins the switcher heads with.
struct PinScope: Hashable {
    let accountId: String
    let teamId: String
}

struct BoardSwitcherSheet: View {
    let boardLoader: MultiAccountBoardLoader?
    let currentBoard: CurrentBoardRef?
    let onSelect: (_ accountId: String, _ boardId: String) -> Void
    /// EXP-698 r5 (Android parity): creating a board and creating a team are
    /// both reachable FROM the switcher — the two things a switcher with
    /// nothing worth switching to should offer.
    let onCreateBoard: () -> Void
    let onCreateTeam: () -> Void
    /// EXP-778: the account + team whose personal pins head the sheet (nil =
    /// no team in view, no section).
    var pinScope: PinScope? = nil
    /// A pinned row was tapped — the caller parks the route and pushes it
    /// once the sheet is gone (the create-entries hand-off).
    var onOpenPinned: (PinnedItem) -> Void = { _ in }

    @Environment(AppDependencies.self) private var deps
    @State private var pinned = PinnedItemsModel()

    var body: some View {
        GlassSheetChrome(title: "Switch board") {
            let groups = boardLoader?.groups ?? []
            LazyVStack(alignment: .leading, spacing: 18) {
                // EXP-778: the sidebar's Pinned section — on top, hidden
                // when the team has no resolvable pin.
                if !pinned.items.isEmpty {
                    pinnedSection
                }
                if groups.isEmpty {
                    emptyHint
                } else {
                    ForEach(groups) { group in
                        serverSection(group)
                    }
                    // At the very bottom, under every server's teams.
                    plainActionRow(
                        icon: AppIcons.uiAdd,
                        title: "New team",
                        identifier: "board-switcher-new-team",
                        action: onCreateTeam
                    )
                }
            }
            .padding(.horizontal, 16)
            .padding(.top, 4)
            .padding(.bottom, 24)
        }
        .task(id: pinScope) {
            guard let pinScope else {
                pinned.stop()
                return
            }
            pinned.observe(accountId: pinScope.accountId, teamId: pinScope.teamId, db: deps.db)
        }
    }

    // MARK: - Pinned (EXP-778)

    @ViewBuilder
    private var pinnedSection: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                AppIcon(AppIcons.uiPin, size: 14)
                Text("Pinned")
                    .font(.caption.weight(.semibold))
                Spacer()
            }
            .foregroundStyle(.white.opacity(0.85))
            .padding(.horizontal, 4)

            VStack(spacing: 6) {
                ForEach(pinned.items) { item in
                    Button {
                        onOpenPinned(item)
                    } label: {
                        pinnedRow(item)
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("pinned-row")
                }
            }
        }
    }

    @ViewBuilder
    private func pinnedRow(_ item: PinnedItem) -> some View {
        HStack(spacing: 12) {
            switch item {
            case let .issue(_, issue):
                AppIcon(AppIcons.uiIssue, size: 16)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                HStack(spacing: 6) {
                    if let identifier = issue.identifier, !identifier.isEmpty {
                        Text(identifier)
                            .font(.caption.monospaced())
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .lineLimit(1)
                    }
                    Text(issue.title.isEmpty ? "Untitled issue" : issue.title)
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
            case let .session(_, session, issue):
                // The sessions list's first line, so the two cannot drift;
                // an ended run wears the static neutral dot.
                let ended = session.status == DomainContract.codingSessionStatusEnded
                SessionRowTitle(
                    identifier: issue?.identifier,
                    title: sessionRowTitle(issue: issue, session: session),
                    state: CodingSessionDisplayState.of(
                        session: session, prState: issue?.prState ?? session.prState
                    ),
                    paused: ended,
                    // EXP-848: pulses only while a turn is open.
                    busy: session.agentBusy
                )
            case let .action(_, action):
                AppIcon(action.icon ?? AppIcons.actionDefault, size: 16)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text(action.name)
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white)
                    .lineLimit(1)
            }

            Spacer(minLength: 0)

            AppIcon(AppIcons.uiChevronRight, size: 16)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
        .glassRow()
    }

    private var emptyHint: some View {
        VStack(spacing: 10) {
            AppIcon(AppIcons.navBoards, size: AppIcon.Size.large)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text("Create your first board on the web or desktop app.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, 40)
        .padding(.vertical, 32)
    }

    @ViewBuilder
    private func serverSection(_ group: ServerBoardGroup) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            // The hostname/email header only disambiguates when several
            // accounts are signed in — with a single account it's noise.
            if (boardLoader?.groups.count ?? 0) > 1 {
                HStack(alignment: .top) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(group.hostname)
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(.white.opacity(0.9))
                        if let email = group.userEmail, !email.isEmpty {
                            Text(email)
                                .font(.caption2)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        }
                    }
                    Spacer()
                }
                .padding(.horizontal, 4)
            }

            ForEach(group.teamBlocks) { block in
                teamBlock(accountId: group.accountId, block: block)
            }
        }
    }

    @ViewBuilder
    private func teamBlock(accountId: String, block: TeamBlock) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            // Just the mark and the name (EXP-698 r5): the board COUNT was a
            // number nobody switches on, and it is right there as the row
            // count underneath.
            HStack(spacing: 8) {
                TeamAvatar(team: block.team, size: 18)
                Text(block.team.name)
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.white.opacity(0.85))
                Spacer()
            }
            .padding(.horizontal, 4)

            VStack(spacing: 6) {
                ForEach(block.boards) { board in
                    Button {
                        onSelect(accountId, board.id)
                    } label: {
                        boardRow(board, isCurrent: isCurrent(accountId: accountId, boardId: board.id))
                    }
                    .buttonStyle(.plain)
                }

                plainActionRow(
                    icon: AppIcons.uiAdd,
                    title: "Create board",
                    identifier: "board-switcher-create-board",
                    action: onCreateBoard
                )
            }
        }
    }

    private func isCurrent(accountId: String, boardId: String) -> Bool {
        currentBoard?.accountId == accountId && currentBoard?.boardId == boardId
    }

    @ViewBuilder
    private func boardRow(_ board: BoardEntity, isCurrent: Bool) -> some View {
        HStack(spacing: 12) {
            // Board glyph (stored icon, else a shape-derived fallback) tinted
            // with the board color (replaces the plain color dot).
            AppIcon(BoardTypeDisplay.iconName(for: board), size: 16)
                .foregroundStyle(Color(hex: board.color ?? "#888888") ?? .gray)

            Text(board.name)
                .font(.body)
                .foregroundStyle(.white)

            Spacer()

            Text(board.prefix)
                .font(.caption.monospaced())
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))

            // EXP-698 r5: a chevron, not a tick. The ACTIVE row already says
            // so with its brighter fill (`glassRow(isActive:)`), and every row
            // here goes somewhere — which is what a chevron means everywhere
            // else in the app.
            AppIcon(AppIcons.uiChevronRight, size: 16)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
        .glassRow(isActive: isCurrent)
    }

    /// A plain (non-carded) muted row — the two creation entries. Deliberately
    /// card-less: they are not boards, and a glass row here would read as one.
    /// The identifier is what the styleguide lane taps: both titles are also
    /// button LABELS elsewhere on the screen behind the sheet, so a label query
    /// would be ambiguous.
    @ViewBuilder
    private func plainActionRow(
        icon: String,
        title: String,
        identifier: String,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            HStack(spacing: 12) {
                AppIcon(icon, size: 16)
                Text(title)
                    .font(.body)
                Spacer()
            }
            .foregroundStyle(.white.opacity(TextOpacity.secondary))
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier(identifier)
    }
}
