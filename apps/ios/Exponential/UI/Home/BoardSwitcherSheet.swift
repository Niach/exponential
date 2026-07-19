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

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Switch board")
                .font(.headline)
                .foregroundStyle(.white)
                .padding(.horizontal, 20)
                .padding(.top, 18)
                .padding(.bottom, 10)

            let groups = boardLoader?.groups ?? []
            if groups.isEmpty {
                emptyHint
            } else {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 18) {
                        ForEach(groups) { group in
                            serverSection(group)
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.top, 4)
                    .padding(.bottom, 24)
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    private var emptyHint: some View {
        VStack(spacing: 10) {
            Spacer()
            Image(systemName: "tray")
                .font(.title3)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text("Create your first board on the web or desktop app.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .multilineTextAlignment(.center)
            Spacer()
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, 40)
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
            HStack(spacing: 8) {
                TeamAvatar(team: block.team, size: 18)
                Text(block.team.name)
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.white.opacity(0.85))
                Spacer()
                Text("\(block.boards.count)")
                    .font(.caption2.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
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
            Image(systemName: BoardTypeDisplay.symbol(for: board))
                .font(.caption)
                .foregroundStyle(Color(hex: board.color ?? "#888888") ?? .gray)
                .frame(width: 16, height: 16)

            Text(board.name)
                .font(.body)
                .foregroundStyle(.white)

            Spacer()

            Text(board.prefix)
                .font(.caption.monospaced())
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))

            if isCurrent {
                Image(systemName: "checkmark")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(Accent.indigo)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
        .glassRow()
    }
}
