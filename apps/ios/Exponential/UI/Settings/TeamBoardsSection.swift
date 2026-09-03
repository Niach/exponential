import ExpUI
import ExpCore
import SwiftUI

struct TeamBoardsSection: View {
    let boards: [BoardEntity]
    let accountId: String
    let teamId: String
    let isOwner: Bool
    let boardsApi: BoardsApi
    let repositoriesApi: RepositoriesApi
    let onDelete: (BoardEntity) -> Void

    @State private var repoTarget: BoardEntity?
    @State private var showCreate = false

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("Boards")
                    .font(.headline)
                    .foregroundStyle(.white)
                Text("\(boards.count)")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))

                Spacer()

                // New boards require repo-connect rights — owner-gated, matching
                // the server's create policy.
                if isOwner {
                    GlassPill("New board", icon: AppIcons.uiAdd, mode: .action {
                        showCreate = true
                    })
                }
            }

            if boards.isEmpty {
                Text("No boards in this team yet.")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
            } else {
                ForEach(boards, id: \.id) { board in
                    HStack(spacing: 10) {
                        // Board glyph tinted with the board color — same idiom
                        // as the board switcher sheet (EXP-449).
                        AppIcon(BoardTypeDisplay.iconName(for: board), size: 16)
                            .foregroundStyle(Color(hex: board.color ?? "#888888") ?? .gray)

                        // Board name
                        Text(board.name)
                            .font(.subheadline)
                            .foregroundStyle(.white)
                            .lineLimit(1)

                        Spacer()

                        // Backing repo (v4: one board = one repo). Read-only
                        // chip resolving the synced repositoryId to owner/name.
                        RepoNameChip(
                            accountId: accountId,
                            teamId: board.teamId,
                            repositoryId: board.repositoryId
                        )

                        // Member-level repo + branch editing → boards.setRepository
                        // / boards.update (mutate_resources server-side).
                        Button {
                            repoTarget = board
                        } label: {
                            AppIcon(AppIcons.uiSwap, size: AppIcon.Size.small)
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        }
                        .buttonStyle(.plain)

                        // Delete (→ trash) — owner-only (the server refuses for
                        // everyone else). Hidden for non-owners entirely (full
                        // web parity); the tap still routes through the parent's
                        // destructive confirmation alert.
                        if isOwner {
                            Button {
                                onDelete(board)
                            } label: {
                                AppIcon(AppIcons.uiDelete, size: AppIcon.Size.small)
                                    .foregroundStyle(.red.opacity(0.5))
                            }
                            .buttonStyle(.plain)
                        }
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .glassRow()
                }
            }
        }
        .sheet(item: $repoTarget) { board in
            ChangeRepositorySheet(
                accountId: accountId,
                board: board,
                boardsApi: boardsApi,
                repositoriesApi: repositoriesApi
            )
        }
        .sheet(isPresented: $showCreate) {
            CreateBoardSheet(accountId: accountId, teamId: teamId)
        }
    }
}

/// A board's repository + branch, edited in place (EXP-712). Retargeting goes
/// through `boards.setRepository` — which RESETS the branch server-side, a
/// branch belonging to the repo it was picked in — and the branch itself
/// through `boards.update`. Connecting a brand-new repo registers it first
/// (`repositories.add`, member-level since EXP-557) and then points the board
/// at the returned row.
private struct ChangeRepositorySheet: View {
    let accountId: String
    let board: BoardEntity
    let boardsApi: BoardsApi
    let repositoriesApi: RepositoriesApi

    @State private var repositoryId: String?
    @State private var branch: String?
    @State private var busy = false
    @State private var errorText: String?

    init(accountId: String, board: BoardEntity, boardsApi: BoardsApi, repositoriesApi: RepositoriesApi) {
        self.accountId = accountId
        self.board = board
        self.boardsApi = boardsApi
        self.repositoriesApi = repositoriesApi
        _repositoryId = State(initialValue: board.repositoryId)
        _branch = State(initialValue: board.defaultBranch)
    }

    var body: some View {
        GlassSheetChrome(title: "Repository") {
            VStack(alignment: .leading, spacing: 12) {
                Text(board.name)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))

                BoardRepoField(
                    accountId: accountId,
                    teamId: board.teamId,
                    repositoryId: repositoryId,
                    onSelectRegistry: { repo in Task { await applyRepo(repo?.id) } },
                    onConnectNew: { picked in Task { await connect(picked) } },
                    branch: branch,
                    onBranchChange: { next in Task { await applyBranch(next) } },
                    disabled: busy,
                    errorText: errorText
                )
            }
            .padding(16)
        }
    }

    private func applyRepo(_ nextId: String?) async {
        guard nextId != repositoryId else { return }
        busy = true
        errorText = nil
        defer { busy = false }
        do {
            try await boardsApi.setRepository(
                accountId: accountId,
                boardId: board.id,
                repositoryId: nextId
            )
            repositoryId = nextId
            // The retarget dropped the pin — mirror it locally.
            branch = nil
            RepositoryDirectory.invalidate(accountId: accountId, teamId: board.teamId)
        } catch {
            errorText = error.trpcUserMessage
        }
    }

    private func connect(_ picked: GithubPickerRepo) async {
        busy = true
        errorText = nil
        do {
            let id = try await repositoriesApi.add(
                accountId: accountId,
                teamId: board.teamId,
                fullName: picked.fullName,
                defaultBranch: picked.defaultBranch,
                isPrivate: picked.`private`
            )
            busy = false
            guard let id else {
                errorText = "Could not connect \(picked.fullName)."
                return
            }
            await applyRepo(id)
        } catch {
            busy = false
            errorText = error.trpcUserMessage
        }
    }

    private func applyBranch(_ next: String?) async {
        guard next != branch else { return }
        busy = true
        errorText = nil
        defer { busy = false }
        do {
            try await boardsApi.setDefaultBranch(
                accountId: accountId,
                boardId: board.id,
                defaultBranch: next
            )
            branch = next
        } catch {
            errorText = error.trpcUserMessage
        }
    }
}
