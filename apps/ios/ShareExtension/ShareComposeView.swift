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
    @State private var selectedBoardId: String?
    @State private var submitting = false
    @State private var error: String?

    private let boards: [MirroredBoard]

    init(deps: ShareDependencies, payload: SharedPayload, onComplete: @escaping () -> Void, onCancel: @escaping () -> Void) {
        self.deps = deps
        self.payload = payload
        self.onComplete = onComplete
        self.onCancel = onCancel
        let boards = SharedBoardMirror.readBoards()
        self.boards = boards
        _title = State(initialValue: payload.title)
        _descriptionText = State(initialValue: payload.descriptionText)
        let lastUsed = SharedBoardMirror.readLastUsed()?.boardId
        _selectedBoardId = State(initialValue:
            lastUsed.flatMap { id in boards.first { $0.boardId == id }?.boardId } ?? boards.first?.boardId
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
        if !deps.auth.isAuthenticated {
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
                    Picker("Board", selection: $selectedBoardId) {
                        ForEach(boards) { board in
                            Text("\(board.teamName) / \(board.boardName)")
                                .tag(Optional(board.boardId))
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
        !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && selectedBoardId != nil && !submitting
    }

    private func post() {
        guard let boardId = selectedBoardId,
              let board = boards.first(where: { $0.boardId == boardId }) else { return }
        submitting = true
        error = nil
        var submitted = payload
        submitted.title = title
        submitted.descriptionText = descriptionText
        let submitter = ShareSubmitter(issuesApi: deps.issuesApi, issueImagesApi: deps.issueImagesApi)
        Task {
            do {
                try await submitter.submit(payload: submitted, accountId: board.accountId, boardId: boardId)
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
