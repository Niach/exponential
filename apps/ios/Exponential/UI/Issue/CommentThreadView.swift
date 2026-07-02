import ExpUI
import ExpCore
import SwiftUI
import GRDB
import MarkdownUI

// The activity timeline. Reads live comments + issue_events from the local GRDB
// store (populated by Electric sync) and routes create/update/delete through
// tRPC. Renders regular comments and issue events (status/assignee/label/PR
// changes) inline, merged by created_at.
struct CommentThreadView: View {
    let issue: IssueEntity

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var comments: [CommentEntity] = []
    @State private var events: [IssueEventEntity] = []
    @State private var users: [String: UserEntity] = [:]
    @State private var labels: [String: LabelEntity] = [:]
    @State private var composerEditor = IssueEditorModel()
    @State private var composerHasText = false
    @State private var submitting = false
    @State private var editingCommentId: String?
    // The rich editor backing the comment currently being edited (re-seeded on
    // each Edit tap; only one comment edits at a time).
    @State private var editEditor = IssueEditorModel()
    @State private var observationTask: Task<Void, Never>?

    // Linear-style activity timeline: regular comments + non-agent events.
    private enum TimelineItem: Identifiable {
        case comment(CommentEntity)
        case event(IssueEventEntity)
        var id: String {
            switch self {
            case .comment(let c): return "c-\(c.id)"
            case .event(let e): return "e-\(e.id)"
            }
        }
        var createdAt: String {
            switch self {
            case .comment(let c): return c.createdAt
            case .event(let e): return e.createdAt
            }
        }
    }

    private var humanComments: [CommentEntity] {
        comments.filter { $0.commentKind == .regular }
    }

    private var timeline: [TimelineItem] {
        return (humanComments.map { TimelineItem.comment($0) }
            + events.map { TimelineItem.event($0) })
            .sorted { $0.createdAt < $1.createdAt }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(humanComments.isEmpty ? "Comments" : "Comments (\(humanComments.count))")
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Spacer()
            }

            if timeline.isEmpty {
                Text("No comments yet. Be the first to add one.")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }

            ForEach(timeline) { item in
                switch item {
                case .comment(let comment): commentRow(comment)
                case .event(let event): eventRow(event)
                }
            }

            // Rich block-markdown composer (parity with the description editor):
            // reuses MarkdownEditor + IssueEditorModel; images route through the
            // issue image-upload path on submit.
            VStack(alignment: .trailing, spacing: 6) {
                MarkdownEditor(
                    model: composerEditor,
                    placeholder: "Write a comment…",
                    baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
                    accountId: accountId,
                    httpClient: deps.httpClient,
                    mentionMembers: users.values.filter { !$0.isAgent }.map { MentionMember(name: $0.name ?? $0.email, email: $0.email) }
                )
                .frame(minHeight: 44, maxHeight: 140)
                .background(Color.white.opacity(0.04))
                .clipShape(RoundedRectangle(cornerRadius: 8))

                Button {
                    Task { await submit() }
                } label: {
                    Image(systemName: "paperplane.fill")
                        .padding(8)
                        .background(.blue, in: RoundedRectangle(cornerRadius: 8))
                        .foregroundStyle(.white)
                }
                .disabled(submitting || !composerHasText)
            }
        }
        .padding(.vertical, 8)
        .onAppear {
            startObserving()
            composerEditor.onEdit = { composerHasText = !composerEditor.currentMarkdown().trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
        }
        .onDisappear { observationTask?.cancel() }
    }

    @ViewBuilder
    private func commentRow(_ comment: CommentEntity) -> some View {
        RegularCommentRow(
            comment: comment,
            author: users[comment.authorId],
            isAuthor: comment.authorId == deps.auth.userId,
            isAdmin: deps.auth.isAdmin,
            isEditing: editingCommentId == comment.id,
            editEditor: editEditor,
            baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
            accountId: accountId,
            httpClient: deps.httpClient,
            mentionMembers: users.values.filter { !$0.isAgent }.map { MentionMember(name: $0.name ?? $0.email, email: $0.email) },
            resolveIssueRef: { identifier in resolveIssueRef(identifier) },
            onOpenIssue: { issueId in deps.deepLinkBus.navigateToIssue(issueId) },
            onEdit: {
                // Fresh model per edit, seeded from the comment's markdown — the
                // same rich block editor as the composer (images, mentions, lists).
                let editor = IssueEditorModel()
                editor.load(
                    markdown: getCommentBodyText(comment.body),
                    baseURL: deps.auth.instanceBaseURL(forAccountId: accountId)
                )
                editEditor = editor
                editingCommentId = comment.id
            },
            onCancelEdit: { editingCommentId = nil },
            onSaveEdit: {
                let ok = await editEditor.commitPendingImages(uploader: makeCommentImageUploader())
                guard ok, !editEditor.hasUncommittedDrafts else { return }
                let md = editEditor.currentMarkdown().trimmingCharacters(in: .whitespacesAndNewlines)
                guard !md.isEmpty else { return }
                do {
                    try await deps.commentsApi.update(accountId: accountId, id: comment.id, text: md)
                    editingCommentId = nil
                } catch {}
            },
            onDelete: {
                Task { try? await deps.commentsApi.delete(accountId: accountId, id: comment.id) }
            }
        )
    }

    private func submit() async {
        submitting = true
        defer { submitting = false }
        // All-or-nothing image commit before deriving markdown (mirrors the
        // description save path).
        let ok = await composerEditor.commitPendingImages(uploader: makeCommentImageUploader())
        guard ok, !composerEditor.hasUncommittedDrafts else { return }
        let md = composerEditor.currentMarkdown().trimmingCharacters(in: .whitespacesAndNewlines)
        guard !md.isEmpty else { return }
        do {
            try await deps.commentsApi.create(accountId: accountId, issueId: issue.id, text: md)
            resetComposer()
        } catch {}
    }

    private func makeCommentImageUploader() -> @Sendable (PendingImage) async throws -> String {
        let api = deps.issueImagesApi
        let acc = accountId
        let issueId = issue.id
        return { image in
            let uploaded = try await api.upload(
                accountId: acc, issueId: issueId,
                data: image.data, filename: image.filename, contentType: image.contentType
            )
            return uploaded.url
        }
    }

    /// identifier (e.g. `VER-12`) → local issue id for inline `#IDENTIFIER`
    /// pills in comment bodies (render-only; unresolved refs stay plain text).
    private func resolveIssueRef(_ identifier: String) -> String? {
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return nil }
        return (try? pool.read { db in
            try String.fetchOne(
                db,
                sql: "SELECT id FROM issues WHERE upper(identifier) = ? AND archived_at IS NULL",
                arguments: [identifier]
            )
        }) ?? nil
    }

    private func resetComposer() {
        composerEditor = IssueEditorModel()
        composerHasText = false
        composerEditor.onEdit = { composerHasText = !composerEditor.currentMarkdown().trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
    }

    private func startObserving() {
        observationTask?.cancel()
        observationTask = Task {
            guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }

            let commentObs = ValueObservation.tracking { db in
                try CommentEntity
                    .filter(Column("issue_id") == issue.id)
                    .order(Column("created_at").asc)
                    .fetchAll(db)
            }
            Task {
                for try await rows in commentObs.values(in: pool) {
                    self.comments = rows
                }
            }

            let userObs = ValueObservation.tracking { db in
                try UserEntity.fetchAll(db)
            }
            Task {
                for try await rows in userObs.values(in: pool) {
                    self.users = Dictionary(uniqueKeysWithValues: rows.map { ($0.id, $0) })
                }
            }

            let labelObs = ValueObservation.tracking { db in
                try LabelEntity.fetchAll(db)
            }
            Task {
                for try await rows in labelObs.values(in: pool) {
                    self.labels = Dictionary(uniqueKeysWithValues: rows.map { ($0.id, $0) })
                }
            }

            let eventObs = ValueObservation.tracking { db in
                try IssueEventEntity
                    .filter(Column("issue_id") == issue.id)
                    .order(Column("created_at").asc)
                    .fetchAll(db)
            }
            Task {
                for try await rows in eventObs.values(in: pool) {
                    self.events = rows
                }
            }
        }
    }

    // Compact Linear-style activity line for issue events (status/assignee/
    // label/PR changes).
    @ViewBuilder
    private func eventRow(_ event: IssueEventEntity) -> some View {
        let who = event.actorUserId.flatMap { users[$0] }.map { $0.name ?? $0.email } ?? "Someone"
        HStack(spacing: 8) {
            Circle()
                .fill(.white.opacity(TextOpacity.tertiary))
                .frame(width: 6, height: 6)
            Text("\(who) \(eventPhrase(event, users: users, labels: labels))")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Spacer()
        }
        .padding(.vertical, 4)
    }
}

// MARK: - Regular comment

private struct RegularCommentRow: View {
    let comment: CommentEntity
    let author: UserEntity?
    let isAuthor: Bool
    let isAdmin: Bool
    let isEditing: Bool
    let editEditor: IssueEditorModel
    let baseURL: URL?
    let accountId: String
    let httpClient: HTTPClient?
    let mentionMembers: [MentionMember]
    let resolveIssueRef: (String) -> String?
    let onOpenIssue: (String) -> Void
    let onEdit: () -> Void
    let onCancelEdit: () -> Void
    let onSaveEdit: () async -> Void
    let onDelete: () -> Void

    @State private var saving = false

    private var canModify: Bool { isAuthor || isAdmin }

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            avatar(author: author)

            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 6) {
                    Text(displayName(for: author))
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.white)
                    Text(relativeDate(comment.createdAt))
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    if comment.editedAt != nil {
                        Text("· edited")
                            .font(.caption2)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                    Spacer()
                    if canModify && !isEditing {
                        Menu {
                            Button("Edit", action: onEdit)
                            Button("Delete", role: .destructive, action: onDelete)
                        } label: {
                            Image(systemName: "ellipsis")
                                .font(.caption)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                .frame(width: 32, height: 32)
                                .contentShape(Rectangle())
                        }
                    }
                }

                if isEditing {
                    // The same rich block editor as the composer — formatting,
                    // images, and @mentions all work in edit mode too.
                    MarkdownEditor(
                        model: editEditor,
                        placeholder: "Edit comment…",
                        baseURL: baseURL,
                        accountId: accountId,
                        httpClient: httpClient,
                        mentionMembers: mentionMembers
                    )
                    .frame(minHeight: 60, maxHeight: 220)
                    .padding(.vertical, 2)
                    .background(Color.white.opacity(0.04))
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                    HStack {
                        Button {
                            saving = true
                            Task {
                                await onSaveEdit()
                                saving = false
                            }
                        } label: {
                            if saving { ProgressView().controlSize(.small) } else { Text("Save") }
                        }
                        .buttonStyle(.borderedProminent)
                        .controlSize(.small)
                        .disabled(saving)
                        Button("Cancel", action: onCancelEdit)
                            .controlSize(.small)
                            .disabled(saving)
                    }
                } else {
                    // Display-only transform: resolved `#IDENTIFIER` refs become
                    // tappable links routed back to the app (never persisted —
                    // the edit path reseeds from the raw stored markdown).
                    Markdown(IssueRefs.linkifyForDisplay(getCommentBodyText(comment.body), resolver: resolveIssueRef))
                        .markdownTheme(.gitHub)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .environment(\.openURL, OpenURLAction { url in
                            if url.scheme == "exp-issue", let issueId = url.host, !issueId.isEmpty {
                                onOpenIssue(issueId)
                                return .handled
                            }
                            return .systemAction
                        })
                }
            }
        }
        .padding(.vertical, 4)
    }
}

// MARK: - Shared helpers

private func avatar(author: UserEntity?) -> some View {
    Circle()
        .fill(Color.white.opacity(0.08))
        .frame(width: 28, height: 28)
        .overlay(
            Text(initials(for: author))
                .font(.caption2.weight(.semibold))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
        )
}

private func displayName(for author: UserEntity?, fallback: String = "Someone") -> String {
    author?.name ?? author?.email ?? fallback
}

private func initials(for author: UserEntity?) -> String {
    let source = displayName(for: author)
    let parts = source.split(separator: " ").prefix(2)
    return parts.map { $0.first.map(String.init) ?? "" }.joined().uppercased()
}

private func relativeDate(_ s: String) -> String {
    let isoFormatter = ISO8601DateFormatter()
    isoFormatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    let date = isoFormatter.date(from: s) ?? ISO8601DateFormatter().date(from: s)
    guard let date else { return "" }
    let formatter = RelativeDateTimeFormatter()
    formatter.unitsStyle = .short
    return formatter.localizedString(for: date, relativeTo: Date())
}
