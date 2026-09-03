import ExpUI
import ExpCore
import SwiftUI

/// Bottom-sheet board picker for the Issues tab's inline switcher: the
/// server → team → board tree that used to be the Boards overview
/// screen, now presented modally. Selecting a board swaps the Issues tab's
/// list in place (the caller writes last-used and dismisses).
struct BoardSwitcherSheet: View {
    let boardLoader: MultiAccountBoardLoader?
    let currentBoard: CurrentBoardRef?
    let onSelect: (_ accountId: String, _ boardId: String) -> Void
    /// EXP-698 r5 (Android parity): creating a board and creating a team are
    /// both reachable FROM the switcher — the two things a switcher with
    /// nothing worth switching to should offer.
    let onCreateBoard: () -> Void
    let onCreateTeam: () -> Void

    var body: some View {
        GlassSheetChrome(title: "Switch board") {
            let groups = boardLoader?.groups ?? []
            if groups.isEmpty {
                emptyHint
            } else {
                LazyVStack(alignment: .leading, spacing: 18) {
                    ForEach(groups) { group in
                        serverSection(group)
                    }
                    // At the very bottom, under every server's teams.
                    plainActionRow(icon: AppIcons.uiAdd, title: "New team", action: onCreateTeam)
                }
                .padding(.horizontal, 16)
                .padding(.top, 4)
                .padding(.bottom, 24)
            }
        }
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

                plainActionRow(icon: AppIcons.uiAdd, title: "Create board", action: onCreateBoard)
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
    @ViewBuilder
    private func plainActionRow(
        icon: String,
        title: String,
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
    }
}
