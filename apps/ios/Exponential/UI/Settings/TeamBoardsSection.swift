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
                    GlassPillButton("New board", icon: AppIcons.uiAdd) {
                        showCreate = true
                    }
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

                        // Member-level retarget → boards.setRepository
                        // (mutate_resources server-side; EXP-557 web parity).
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
            .presentationBackground(.ultraThinMaterial)
        }
        .sheet(isPresented: $showCreate) {
            CreateBoardSheet(accountId: accountId, teamId: teamId)
                .presentationBackground(.ultraThinMaterial)
        }
    }
}

/// Retarget a board's backing repo to another already-connected registry repo
/// (`boards.setRepository`). Connecting a brand-new repo stays a create-board
/// / web-side flow — this picker only offers connected repos.
private struct ChangeRepositorySheet: View {
    let accountId: String
    let board: BoardEntity
    let boardsApi: BoardsApi
    let repositoriesApi: RepositoriesApi

    @Environment(\.dismiss) private var dismiss
    @State private var repos: [TeamRepo] = []
    @State private var loading = true
    @State private var errorText: String?
    @State private var saving = false

    var body: some View {
        NavigationStack {
            ZStack {
                AppBackground()
                ScrollView {
                    VStack(alignment: .leading, spacing: 12) {
                        Text(board.name)
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))

                        if loading {
                            HStack { Spacer(); ProgressView().tint(.white); Spacer() }
                                .padding(.vertical, 24)
                        } else if repos.isEmpty {
                            Text("No repositories connected. Add one in team settings → Repositories first.")
                                .font(.subheadline)
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        } else {
                            ForEach(repos) { repo in
                                Button {
                                    Task { await setRepo(repo) }
                                } label: {
                                    HStack(spacing: 10) {
                                        AppIcon(
                                            repo.id == board.repositoryId
                                                ? AppIcons.uiSelected : AppIcons.uiUnselected,
                                            size: AppIcon.Size.small
                                        )
                                            .foregroundStyle(repo.id == board.repositoryId
                                                ? DesignTokens.Semantic.blue
                                                : .white.opacity(TextOpacity.tertiary))
                                        Text(repo.fullName)
                                            .font(.subheadline.monospaced())
                                            .foregroundStyle(.white)
                                            .lineLimit(1)
                                            .truncationMode(.middle)
                                        Spacer()
                                        if repo.isPrivate {
                                            AppIcon(AppIcons.uiPrivate, size: 11)
                                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                        }
                                    }
                                    .padding(.horizontal, 12)
                                    .padding(.vertical, 10)
                                    .glassRow()
                                }
                                .buttonStyle(.plain)
                                .disabled(saving)
                            }
                        }

                        if let errorText {
                            Text(errorText).font(.caption).foregroundStyle(.red)
                        }
                    }
                    .padding(16)
                }
            }
            .navigationTitle("Change repository")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) { Button("Cancel") { dismiss() } }
            }
            .task { await load() }
        }
    }

    private func load() async {
        loading = true
        defer { loading = false }
        do {
            repos = try await repositoriesApi.list(accountId: accountId, teamId: board.teamId)
            errorText = nil
        } catch {
            errorText = error.trpcUserMessage
        }
    }

    private func setRepo(_ repo: TeamRepo) async {
        guard repo.id != board.repositoryId else { dismiss(); return }
        saving = true
        defer { saving = false }
        do {
            try await boardsApi.setRepository(
                accountId: accountId,
                boardId: board.id,
                repositoryId: repo.id
            )
            RepositoryDirectory.invalidate(accountId: accountId, teamId: board.teamId)
            dismiss()
        } catch {
            errorText = error.trpcUserMessage
        }
    }
}
