import SwiftUI
import UIKit

/// Root SwiftUI surface for the extension: extracts the shared payload, then
/// shows the Share to flow (or a guidance message when not signed in / nothing
/// to share).
struct ShareRootView: View {
    let deps: ShareDependencies
    let extensionItems: [NSExtensionItem]
    /// EXP-1635: the `conversationIdentifier` of a direct share.
    let directShareId: String?
    let onComplete: () -> Void
    let onCancel: () -> Void

    @State private var payload: SharedPayload?
    @State private var loading = true

    var body: some View {
        Group {
            if loading {
                ProgressView().controlSize(.large)
            } else if let payload {
                ShareFlowView(
                    deps: deps, payload: payload, directShareId: directShareId,
                    onComplete: onComplete, onCancel: onCancel
                )
            } else {
                ShareMessageView(message: "Nothing to share here.", onCancel: onCancel)
            }
        }
        .preferredColorScheme(.dark)
        .task {
            payload = await ShareItemExtractor.extract(from: extensionItems)
            loading = false
        }
    }
}

/// EXP-1635 (share-targets.json): step 1 under one navigation stack, with
/// step 2 pushed over it (Back returns to Share to). A direct share opens with
/// step 2 already pushed; an id the viewer no longer sees stays on step 1.
struct ShareFlowView: View {
    let deps: ShareDependencies
    let payload: SharedPayload
    let onComplete: () -> Void
    let onCancel: () -> Void

    @State private var path: [ShareRoute]
    @State private var teamKey: String?
    /// Retained across Send attempts, like `ShareSubmitter`.
    @State private var submitter: ShareChatSubmitter?

    private let teams: [ShareTeam]
    private let mirrors: [String: MirroredShareTargets]

    init(
        deps: ShareDependencies,
        payload: SharedPayload,
        directShareId: String?,
        onComplete: @escaping () -> Void,
        onCancel: @escaping () -> Void
    ) {
        self.deps = deps
        self.payload = payload
        self.onComplete = onComplete
        self.onCancel = onCancel
        // Only accounts that still hold a token (see `ShareComposeView`).
        let signedIn = deps.auth.authenticatedAccountIds
        let boards = SharedBoardMirror.readBoards().filter { signedIn.contains($0.accountId) }
        var seen = Set<String>()
        let teams = boards
            .map { ShareTeam(accountId: $0.accountId, teamId: $0.teamId, name: $0.teamName) }
            .filter { seen.insert($0.id).inserted }
        self.teams = teams
        var mirrors: [String: MirroredShareTargets] = [:]
        for accountId in signedIn {
            mirrors[accountId] = SharedShareMirror.read(accountId: accountId)
        }
        self.mirrors = mirrors

        let lastUsed = SharedBoardMirror.readLastUsed()
            .flatMap { last in boards.first { $0.accountId == last.accountId && $0.boardId == last.boardId } }
        var team = lastUsed.flatMap { board in teams.first { $0.accountId == board.accountId && $0.teamId == board.teamId } }
            ?? teams.first
        var route: ShareRoute?
        switch directShareId.flatMap(ShareTargets.parseDirectShareId) {
        case let .agent(id)?:
            for (accountId, mirror) in mirrors {
                // #EXP-1714: the viewer's global Assistant has no synced
                // home team: it opens in the account's current team.
                guard let agent = mirror.agents.first(where: { $0.id == id }),
                      AgentOrdering.visibleAgent(agent, me: mirror.userId),
                      let home = teams.first(where: { $0.accountId == accountId && $0.teamId == agent.teamId })
                      ?? (team?.accountId == accountId ? team : nil)
                      ?? teams.first(where: { $0.accountId == accountId }) else { continue }
                team = home
                route = .compose(pick: id)
            }
        case let .chat(id)?:
            for (accountId, mirror) in mirrors {
                guard let chat = mirror.chats.first(where: { $0.id == id }),
                      let home = teams.first(where: { $0.accountId == accountId && $0.teamId == chat.root.teamId }) else { continue }
                team = home
                route = .chat(id: id)
            }
        case nil:
            break
        }
        _teamKey = State(initialValue: team?.id)
        _path = State(initialValue: route.map { [$0] } ?? [])
    }

    private var team: ShareTeam? { teams.first { $0.id == teamKey } }
    private var mirror: MirroredShareTargets? { team.flatMap { mirrors[$0.accountId] } }

    private var agents: [MirroredAgent] {
        guard let team, let mirror else { return [] }
        // #EXP-1714: the viewer's global Assistant (its team never syncs)
        // lists in every team.
        let inTeam = AgentOrdering.agentsInTeam(
            mirror.agents,
            teamId: team.teamId,
            teamIds: teams.filter { $0.accountId == team.accountId }.map(\.teamId),
            me: mirror.userId
        )
        return ShareTargets.shareTargets(agents: inTeam, me: mirror.userId).agents
    }

    var body: some View {
        // Any signed-in account can receive a share (the same rule the app's
        // nav gate uses); the ACTIVE account is routinely tokenless after a
        // per-server sign-out and says nothing about the others.
        if !deps.auth.hasAuthenticatedAccount {
            ShareMessageView(
                message: "Sign in to Exponential first, then try sharing again.",
                onCancel: onCancel
            )
        } else if teams.isEmpty {
            ShareMessageView(
                message: "Open Exponential and let it sync once, then try sharing again.",
                onCancel: onCancel
            )
        } else {
            NavigationStack(path: $path) {
                ShareToPage(teams: teams, teamKey: $teamKey, agents: agents, images: payload.images)
                    .navigationTitle(ShareTargets.Copy.title)
                    .navigationBarTitleDisplayMode(.inline)
                    .toolbar {
                        ToolbarItem(placement: .cancellationAction) {
                            Button("Cancel", action: onCancel)
                        }
                    }
                    .navigationDestination(for: ShareRoute.self, destination: page)
            }
        }
    }

    @ViewBuilder
    private func page(_ route: ShareRoute) -> some View {
        switch route {
        case .newIssue:
            ShareComposeView(deps: deps, payload: payload, team: team, onComplete: onComplete, onCancel: onCancel)
        case let .compose(pick):
            composer(pick: pick, chat: nil)
        case let .chat(id):
            composer(pick: ComposerAgentRow.temporaryPick, chat: mirror?.chats.first { $0.id == id })
        }
    }

    private func composer(pick: String, chat: MirroredChat?) -> some View {
        ShareChatComposerPage(
            context: ShareComposerContext(
                teamId: team?.teamId,
                agents: agents,
                me: mirror?.userId,
                devices: team.flatMap { mirror?.startDevices(teamId: $0.teamId) } ?? [],
                chat: chat
            ),
            pick: pick,
            payload: payload,
            onSend: { destination, text, attachments in
                guard let accountId = team?.accountId else { return }
                let submitter = submitter ?? ShareChatSubmitter(deps: deps)
                self.submitter = submitter
                try await submitter.send(
                    accountId: accountId, destination: destination, text: text, attachments: attachments
                )
            },
            onComplete: onComplete
        )
    }
}

/// Editable compose form: a "Share to" destination picker on top (EXP-60,
/// defaulting to the most recently used board), then title, description and
/// image thumbnails.
struct ShareComposeView: View {
    let deps: ShareDependencies
    let payload: SharedPayload
    let onComplete: () -> Void
    let onCancel: () -> Void

    @State private var title: String
    @State private var descriptionText: String
    /// The composite `MirroredBoard.id` (accountId + boardId) — a bare boardId
    /// is ambiguous once two accounts on the same server mirror a shared team's
    /// board, and it loses the account the issue must be created as.
    @State private var selectedBoardKey: String?
    @State private var submitting = false
    @State private var error: String?
    /// Retained across Post attempts so a retry resumes the submission it
    /// already started instead of creating a duplicate issue.
    @State private var submitter: ShareSubmitter?

    private let boards: [MirroredBoard]

    init(
        deps: ShareDependencies,
        payload: SharedPayload,
        team: ShareTeam?,
        onComplete: @escaping () -> Void,
        onCancel: @escaping () -> Void
    ) {
        self.deps = deps
        self.payload = payload
        self.onComplete = onComplete
        self.onCancel = onCancel
        // Only boards whose OWN account still holds a token: the extension
        // submits per account (HTTPClient resolves the bearer by accountId), so
        // signing out of one server must neither hide the other accounts' boards
        // nor offer a destination that can no longer authenticate.
        let signedIn = deps.auth.authenticatedAccountIds
        // #EXP-1823 (share-targets.json `shareIssueBoards`): the team picked
        // on Share to owns the page, its boards only, its last board first.
        let lastUsed = SharedBoardMirror.readLastUsed()
        let picked = ShareTargets.shareIssueBoards(
            teamId: team?.id,
            boards: SharedBoardMirror.readBoards().filter { signedIn.contains($0.accountId) },
            lastBoardId: lastUsed.map { $0.accountId + ":" + $0.boardId },
            id: \.id,
            team: { ShareTeam(accountId: $0.accountId, teamId: $0.teamId, name: $0.teamName).id }
        )
        self.boards = picked.boards
        _title = State(initialValue: payload.title)
        _descriptionText = State(initialValue: payload.descriptionText)
        _selectedBoardKey = State(initialValue: picked.board?.id)
    }

    var body: some View {
        // Pushed over Share to: Back (no Cancel) leaves the page, and the
        // guidance is the stack's page, no second stack (EXP-405).
        if boards.isEmpty {
            ShareMessageView(
                message: "Open Exponential and let it sync once, then try sharing again.",
                onCancel: onCancel,
                embedded: true
            )
        } else {
            content
                .navigationTitle("New Issue")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) {
                        if submitting {
                            ProgressView()
                        } else {
                            Button("Post", action: post)
                                .disabled(!canPost)
                        }
                    }
                }
        }
    }

    @ViewBuilder
    private var content: some View {
        Form {
            // Destination first (EXP-60): choosing where the share lands
            // leads the form, matching the Android share composer.
            Section("Share to") {
                // Deliberately the system `Picker` (EXP-603): the glass
                // menu/sheet language lives in ExpUI, which the share
                // extension does not link.
                Picker("Board", selection: $selectedBoardKey) {
                    ForEach(boards) { board in
                        Text("\(board.teamName) / \(board.boardName)")
                            .tag(Optional(board.id))
                    }
                }
            }
            Section("Title") {
                TextField("Issue title", text: $title)
            }
            Section("Description") {
                // NOT a GlassTextField: the ShareExtension target links no
                // ExpUI (Project.swift `dependencies: []`), so the shared
                // control is out of reach here. EXP-698.
                TextField("Description", text: $descriptionText, axis: .vertical)
                    .lineLimit(2...8)
            }
            if !payload.images.isEmpty {
                Section("Images") {
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 8) {
                            ForEach(Array(payload.images.enumerated()), id: \.offset) { _, image in
                                if let uiImage = UIImage(data: image.data) {
                                    Image(uiImage: uiImage)
                                        .resizable()
                                        .scaledToFill()
                                        .frame(width: 64, height: 64)
                                        .clipShape(RoundedRectangle(cornerRadius: 8))
                                }
                            }
                        }
                        .padding(.vertical, 4)
                    }
                }
            }
            if let error {
                Section {
                    Text(error).foregroundStyle(.red).font(.footnote)
                }
            }
        }
    }

    private var canPost: Bool {
        !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && selectedBoardKey != nil && !submitting
    }

    private func post() {
        guard let key = selectedBoardKey,
              let board = boards.first(where: { $0.id == key }) else { return }
        submitting = true
        error = nil
        var submitted = payload
        submitted.title = title
        submitted.descriptionText = descriptionText
        let submitter = self.submitter ?? ShareSubmitter(issuesApi: deps.issuesApi, attachmentsApi: deps.attachmentsApi)
        self.submitter = submitter
        Task {
            do {
                try await submitter.submit(payload: submitted, accountId: board.accountId, boardId: board.boardId)
                onComplete()
            } catch {
                self.error = error.trpcUserMessage
                submitting = false
            }
        }
    }
}

/// Simple centered message with a single Cancel action (not-signed-in / empty).
struct ShareMessageView: View {
    let message: String
    let onCancel: () -> Void
    /// Pushed inside the flow's stack: no stack of its own.
    var embedded = false

    var body: some View {
        if embedded {
            page
        } else {
            NavigationStack {
                page
                    .toolbar {
                        ToolbarItem(placement: .cancellationAction) {
                            Button("Cancel", action: onCancel)
                        }
                    }
            }
        }
    }

    private var page: some View {
        VStack(spacing: 16) {
            Image(systemName: "tray")
                .font(.largeTitle)
                .foregroundStyle(.secondary)
            Text(message)
                .multilineTextAlignment(.center)
                .foregroundStyle(.secondary)
                .padding(.horizontal, 32)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .navigationTitle("Exponential")
        .navigationBarTitleDisplayMode(.inline)
    }
}
