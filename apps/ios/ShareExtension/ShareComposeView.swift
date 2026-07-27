import SwiftUI
import UIKit

/// Root SwiftUI surface for the extension: extracts the shared payload, then
/// shows the compose form (or a guidance message when not signed in / nothing
/// to share).
struct ShareRootView: View {
    let deps: ShareDependencies
    let extensionItems: [NSExtensionItem]
    let onComplete: () -> Void
    let onCancel: () -> Void

    @State private var payload: SharedPayload?
    @State private var loading = true

    var body: some View {
        Group {
            if loading {
                ProgressView().controlSize(.large)
            } else if let payload {
                ShareComposeView(deps: deps, payload: payload, onComplete: onComplete, onCancel: onCancel)
            } else {
                ShareMessageView(message: "Nothing to share here.", onCancel: onCancel)
            }
        }
        .task {
            payload = await ShareItemExtractor.extract(from: extensionItems)
            loading = false
        }
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

    init(deps: ShareDependencies, payload: SharedPayload, onComplete: @escaping () -> Void, onCancel: @escaping () -> Void) {
        self.deps = deps
        self.payload = payload
        self.onComplete = onComplete
        self.onCancel = onCancel
        // Only boards whose OWN account still holds a token: the extension
        // submits per account (HTTPClient resolves the bearer by accountId), so
        // signing out of one server must neither hide the other accounts' boards
        // nor offer a destination that can no longer authenticate.
        let signedIn = deps.auth.authenticatedAccountIds
        let boards = SharedBoardMirror.readBoards().filter { signedIn.contains($0.accountId) }
        self.boards = boards
        _title = State(initialValue: payload.title)
        _descriptionText = State(initialValue: payload.descriptionText)
        let lastUsed = SharedBoardMirror.readLastUsed()
        _selectedBoardKey = State(initialValue:
            lastUsed.flatMap { last in
                boards.first { $0.accountId == last.accountId && $0.boardId == last.boardId }?.id
            } ?? boards.first?.id
        )
    }

    var body: some View {
        NavigationStack {
            content
                .navigationTitle("New Issue")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Cancel", action: onCancel)
                    }
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
        // Any signed-in account can receive a share (the same rule the app's nav
        // gate uses); the ACTIVE account is routinely tokenless after a
        // per-server sign-out and says nothing about the others.
        if !deps.auth.hasAuthenticatedAccount {
            ShareMessageView(
                message: "Sign in to Exponential first, then try sharing again.",
                onCancel: onCancel
            )
        } else if boards.isEmpty {
            ShareMessageView(
                message: "Open Exponential and let it sync once, then try sharing again.",
                onCancel: onCancel
            )
        } else {
            Form {
                // Destination first (EXP-60): choosing where the share lands
                // leads the form, matching the Android share composer.
                Section("Share to") {
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
        let submitter = self.submitter ?? ShareSubmitter(issuesApi: deps.issuesApi, issueImagesApi: deps.issueImagesApi)
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

    var body: some View {
        NavigationStack {
            VStack(spacing: 16) {
                // Stays an SF Symbol: the extension links neither ExpUI nor the
                // app's asset catalog, so the shared Lucide registry (EXP-273)
                // isn't reachable from here.
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
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", action: onCancel)
                }
            }
        }
    }
}
