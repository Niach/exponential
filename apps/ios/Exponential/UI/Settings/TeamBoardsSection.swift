import ExpUI
import ExpCore
import SwiftUI

struct TeamBoardsSection: View {
    let boards: [BoardEntity]
    let accountId: String
    let teamId: String
    let isOwner: Bool
    let boardsApi: BoardsApi
    let issuesApi: IssuesApi
    let repositoriesApi: RepositoriesApi
    let onDelete: (BoardEntity) -> Void

    @State private var repoTarget: BoardEntity?
    @State private var showCreate = false

    /// EXP-525: THE board order ×4 (web `compareBoards`, desktop
    /// `collections.rs`): `sortOrder` ascending (missing = last), then
    /// `createdAt` ascending (missing = first; both wire forms, `T` or a
    /// space, compare as UTC timestamps), then `id` byte-wise.
    static func canonicalOrder(_ boards: [BoardEntity]) -> [BoardEntity] {
        func created(_ board: BoardEntity) -> String? {
            let raw = board.createdAt
            return raw.isEmpty ? nil : raw.replacingOccurrences(of: "T", with: " ")
        }
        return boards.sorted { left, right in
            let leftOrder = left.sortOrder ?? .infinity
            let rightOrder = right.sortOrder ?? .infinity
            if leftOrder != rightOrder { return leftOrder < rightOrder }
            let leftCreated = created(left)
            let rightCreated = created(right)
            if leftCreated != rightCreated {
                guard let leftCreated else { return true }
                guard let rightCreated else { return false }
                return leftCreated < rightCreated
            }
            return Array(left.id.utf8).lexicographicallyPrecedes(Array(right.id.utf8))
        }
    }

    var body: some View {
        // EXP-818: the filled group band over flat rows (Labels/Members
        // parity) — the boards read as a table, not as a stack of cards.
        VStack(alignment: .leading, spacing: 0) {
            // EXP-721: the ONE shared header — the hand-rolled `.headline` +
            // count line is gone, counts having been retired from section
            // headers by EXP-698.
            GlassSectionBand("Boards") {
                // New boards require repo-connect rights — owner-gated, matching
                // the server's create policy.
                if isOwner {
                    GlassPill("New board", icon: AppIcons.uiAdd, mode: .action {
                        showCreate = true
                    })
                }
            }

            if boards.isEmpty {
                EmptyState(.teamBoards)
            } else {
                ForEach(Self.canonicalOrder(boards), id: \.id) { board in
                    HStack(spacing: 10) {
                        // Board glyph tinted with the board color — same idiom
                        // as the board switcher sheet (EXP-449).
                        AppIcon(BoardTypeDisplay.iconName(for: board), size: 16)
                            .foregroundStyle(Color(hex: board.color ?? "#888888") ?? .gray)

                        // EXP-698 (Android parity): the NAME owns the first
                        // line and the backing repo (one board = one repo)
                        // sits under it — side by side, an `owner/repo` chip
                        // ellipsized the board it belongs to.
                        VStack(alignment: .leading, spacing: 4) {
                            Text(board.name)
                                .font(.subheadline)
                                .foregroundStyle(.white)
                                .lineLimit(1)
                            if let repositoryId = board.repositoryId, !repositoryId.isEmpty {
                                BoardRepoLine(
                                    accountId: accountId,
                                    teamId: board.teamId,
                                    repositoryId: repositoryId,
                                    repositoriesApi: repositoriesApi
                                )
                            }
                        }

                        Spacer(minLength: 8)

                        // Member-level repo + branch editing → boards.setRepository
                        // / boards.update (mutate_resources server-side).
                        // EXP-721: the shared chromed circle (Labels parity) —
                        // a bare glyph read as a stray mark in the row.
                        GhostIconButton(AppIcons.uiSwap, accessibilityLabel: "Change repository") {
                            repoTarget = board
                        }

                        // Delete (→ trash) — owner-only (the server refuses for
                        // everyone else). Hidden for non-owners entirely (full
                        // web parity); the tap still routes through the parent's
                        // destructive confirmation alert.
                        if isOwner {
                            GhostIconButton(
                                AppIcons.uiDelete,
                                accessibilityLabel: "Delete board",
                                tint: DesignTokens.Palette.destructive.opacity(0.7)
                            ) {
                                onDelete(board)
                            }
                        }
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 10)
                    .flatRow()
                }
            }

            if isOwner {
                PutAwayBoardsBands(
                    accountId: accountId,
                    teamId: teamId,
                    liveKey: boards.map(\.id).sorted().joined(separator: ","),
                    boardsApi: boardsApi
                )
            }

            // #EXP-1797: the issue trash, for EVERY member (the board bands
            // above stay owner-only).
            DeletedIssuesBand(
                accountId: accountId,
                teamId: teamId,
                boards: boards,
                isOwner: isOwner,
                issuesApi: issuesApi
            )
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

/// #NEW-37 (settings-nav.json `boards`): the archived boards and the 48h
/// trash, each band drawn only while non-empty, under the live list (web
/// `PutAwayBoards`, desktop `all_boards.rs`). Both states are out of every
/// synced shape (EXP-500): the owner-only tRPC reads load on open, when the
/// live list changes and after every Unarchive / Restore; a failed read is
/// quiet (FEED-79), a failed action a toast.
private struct PutAwayBoardsBands: View {
    let accountId: String
    let teamId: String
    let liveKey: String
    let boardsApi: BoardsApi

    @State private var archived: [PutAwayBoard] = []
    @State private var trashed: [PutAwayBoard] = []
    @State private var pendingId: String?
    @Environment(\.toaster) private var toaster

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if !archived.isEmpty {
                band("Archived", rows: archived, caption: { PutAwayBoardCaption.archived($0.archivedAt) }, action: "Unarchive") { board in
                    await run(board, verb: "unarchive") {
                        try await boardsApi.unarchive(accountId: accountId, boardId: board.id)
                    }
                }
            }
            if !trashed.isEmpty {
                // The purge countdown re-renders every minute.
                TimelineView(.periodic(from: .now, by: 60)) { context in
                    band("Trash", rows: trashed, caption: { PutAwayBoardCaption.purge($0.purgeAt, now: context.date) }, action: "Restore") { board in
                        await run(board, verb: "restore") {
                            try await boardsApi.restore(accountId: accountId, boardId: board.id)
                        }
                    }
                }
            }
        }
        .task(id: "\(accountId)|\(teamId)|\(liveKey)") { await refresh() }
    }

    private func band(
        _ title: String,
        rows: [PutAwayBoard],
        caption: @escaping (PutAwayBoard) -> String,
        action: String,
        perform: @escaping (PutAwayBoard) async -> Void
    ) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            GlassSectionBand(title)
            ForEach(rows) { board in
                PutAwayRow(
                    icon: BoardTypeDisplay.iconName(for: board.icon) ?? (board.repositoryId != nil ? "code" : "square-kanban"),
                    color: board.color,
                    text: board.name,
                    caption: caption(board)
                ) {
                    GlassPill(action, mode: .action { Task { await perform(board) } }, enabled: pendingId != board.id)
                }
            }
        }
        .padding(.top, 24)
    }

    private func refresh() async {
        async let nextArchived = try? boardsApi.listArchived(accountId: accountId, teamId: teamId)
        async let nextTrashed = try? boardsApi.listDeleted(accountId: accountId, teamId: teamId)
        if let rows = await nextArchived { archived = rows }
        if let rows = await nextTrashed { trashed = rows }
    }

    /// Refresh on success AND failure: an unarchive can fail because the
    /// board was trashed meanwhile, a restore because it was purged.
    private func run(_ board: PutAwayBoard, verb: String, _ action: () async throws -> Void) async {
        pendingId = board.id
        do {
            try await action()
        } catch {
            toaster.error(
                FeedbackStates.Errors.title(verb: verb, thing: board.name),
                description: error.trpcUserMessage
            )
        }
        pendingId = nil
        await refresh()
    }
}

/// #NEW-37: one put-away row (an archived or trashed board, #EXP-1797 a
/// trashed issue): the glyph tinted with the board color, the text (truncates),
/// the caption, then the pills.
private struct PutAwayRow<Pills: View>: View {
    /// nil = no glyph (an issue whose board is not in the live list).
    let icon: String?
    let color: String?
    let text: String
    let caption: String
    @ViewBuilder let pills: () -> Pills

    var body: some View {
        HStack(spacing: 10) {
            if let icon {
                AppIcon(icon, size: 16)
                    .foregroundStyle(Color(hex: color ?? "#888888") ?? .gray)
            }
            Text(text)
                .font(.subheadline)
                .foregroundStyle(.white)
                .lineLimit(1)
            Spacer(minLength: 8)
            // Web's put-away row: the caption and the pills keep their
            // natural width (`shrink-0`); only the name truncates.
            Text(caption)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)
            HStack(spacing: 8) { pills() }
                .fixedSize(horizontal: true, vertical: false)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .flatRow()
    }
}

/// #EXP-1797 (issue-lifecycle.json `trash`): the team's deleted issues, a
/// band under the board bands drawn only while non-empty, for EVERY member
/// (web `DeletedIssues`, desktop + Android twins). Each row = the issue's
/// board glyph, `{identifier} {title}`, the purge countdown (re-rendered every
/// minute), Restore and, for owners, Delete forever behind the `purge-issue`
/// prompt. Trashed issues are out of every synced shape: `issues.listDeleted`
/// loads on open and after every Restore / Delete forever; a failed read is
/// quiet (the band keeps its rows), a failed action a toast.
private struct DeletedIssuesBand: View {
    let accountId: String
    let teamId: String
    let boards: [BoardEntity]
    let isOwner: Bool
    let issuesApi: IssuesApi

    @State private var trashed: [TrashedIssue] = []
    @State private var pendingId: String?
    @State private var purgeTarget: TrashedIssue?
    @Environment(\.toaster) private var toaster

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if !trashed.isEmpty {
                let actions = IssueTrash.actions(role: isOwner ? "owner" : "member")
                let boardById = Dictionary(boards.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
                TimelineView(.periodic(from: .now, by: 60)) { context in
                    VStack(alignment: .leading, spacing: 0) {
                        GlassSectionBand(IssueTrash.Copy.band)
                        ForEach(trashed) { issue in
                            let board = boardById[issue.boardId]
                            PutAwayRow(
                                icon: board.map { BoardTypeDisplay.iconName(for: $0) },
                                color: board?.color,
                                text: IssueTrash.rowText(identifier: issue.identifier, title: issue.title),
                                caption: IssueTrash.caption(purgeAt: issue.purgeAt, now: context.date)
                            ) {
                                GlassPill(
                                    IssueTrash.Copy.restore,
                                    mode: .action { Task { await restore(issue) } },
                                    enabled: pendingId != issue.id
                                )
                                if actions.contains(.purge) {
                                    GlassPill(
                                        IssueTrash.Copy.purge,
                                        mode: .action { purgeTarget = issue },
                                        enabled: pendingId != issue.id
                                    )
                                }
                            }
                        }
                    }
                    .padding(.top, 24)
                }
            }
        }
        .task(id: "\(accountId)|\(teamId)") { await refresh() }
        .background {
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .glassAlert(item: $purgeTarget) { issue in
                    GlassAlert(
                        prompt: Prompts.PurgeIssue.copy(identifier: issue.identifier),
                        enabled: ["delete": pendingId != issue.id],
                        handlers: ["delete": { Task { await purge(issue) } }]
                    )
                }
        }
    }

    private func refresh() async {
        if let rows = try? await issuesApi.listDeleted(accountId: accountId, teamId: teamId) {
            trashed = rows
        }
    }

    private func restore(_ issue: TrashedIssue) async {
        await run(issue, verb: "restore") {
            try await issuesApi.restore(accountId: accountId, id: issue.id)
        }
    }

    private func purge(_ issue: TrashedIssue) async {
        await run(issue, verb: "delete") {
            try await issuesApi.purge(accountId: accountId, id: issue.id)
        }
    }

    /// Refresh on success AND failure: a restore can fail because the sweep
    /// purged the issue meanwhile.
    private func run(_ issue: TrashedIssue, verb: String, _ action: () async throws -> Void) async {
        pendingId = issue.id
        do {
            try await action()
        } catch {
            toaster.error(
                FeedbackStates.Errors.title(verb: verb, thing: issue.identifier),
                description: error.trpcUserMessage
            )
        }
        pendingId = nil
        await refresh()
    }
}

/// A linked board's repo under its name: the `owner/name` chip once the
/// tRPC registry resolves the synced id, and the FEED-32 `BoardRepoLabel`
/// fallback ("Loading repository…" / "Repository unavailable") until then —
/// never NOTHING. A miss re-lists the registry ONCE (a repo connected on
/// another client a moment ago), Android's `ensureRepoKnown` twin.
private struct BoardRepoLine: View {
    let accountId: String
    let teamId: String
    let repositoryId: String
    let repositoriesApi: RepositoriesApi

    @State private var repo: TeamRepo?
    @State private var loaded = false
    @State private var resolving = false

    var body: some View {
        Group {
            if let repo {
                GlassPill(
                    repo.fullName,
                    mode: .action {
                        if let url = URL(string: "https://github.com/\(repo.fullName)") {
                            Platform.open(url)
                        }
                    }
                ) {
                    AppIcon(AppIcons.uiRepository, size: GlassPillTokens.glyphSm)
                } trailing: {
                    AppIcon(AppIcons.uiExternalLink, size: GlassPillTokens.glyphSm)
                }
            } else {
                Text(
                    BoardRepoLabel.trigger(
                        selectedName: nil,
                        repositoryId: repositoryId,
                        loading: !loaded,
                        resolving: resolving
                    )
                )
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .lineLimit(1)
            }
        }
        .task(id: "\(accountId)|\(teamId)|\(repositoryId)") { await resolve() }
    }

    private func resolve() async {
        repo = await RepositoryDirectory.repo(
            accountId: accountId, teamId: teamId, repositoryId: repositoryId, api: repositoriesApi
        )
        loaded = true
        guard repo == nil else { return }
        resolving = true
        RepositoryDirectory.invalidate(accountId: accountId, teamId: teamId)
        repo = await RepositoryDirectory.repo(
            accountId: accountId, teamId: teamId, repositoryId: repositoryId, api: repositoriesApi
        )
        resolving = false
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
    // FEED-79: a failed retarget/connect/branch save is a toast.
    @Environment(\.toaster) private var toaster

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
                    disabled: busy
                )
            }
            .padding(16)
        }
    }

    private func applyRepo(_ nextId: String?) async {
        guard nextId != repositoryId else { return }
        busy = true
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
            toaster.error(
                FeedbackStates.Errors.title(verb: "change", thing: "the repository"),
                description: error.trpcUserMessage
            )
        }
    }

    private func connect(_ picked: GithubPickerRepo) async {
        busy = true
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
                toaster.error(FeedbackStates.Errors.title(verb: "connect", thing: picked.fullName))
                return
            }
            await applyRepo(id)
        } catch {
            busy = false
            toaster.error(
                FeedbackStates.Errors.title(verb: "connect", thing: picked.fullName),
                description: error.trpcUserMessage
            )
        }
    }

    private func applyBranch(_ next: String?) async {
        guard next != branch else { return }
        busy = true
        defer { busy = false }
        do {
            try await boardsApi.setDefaultBranch(
                accountId: accountId,
                boardId: board.id,
                defaultBranch: next
            )
            branch = next
        } catch {
            toaster.error(
                FeedbackStates.Errors.title(verb: "save", thing: "the branch"),
                description: error.trpcUserMessage
            )
        }
    }
}
