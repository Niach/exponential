import SwiftUI
import GRDB

// Mirror of apps/web/src/components/comment-thread.tsx. Reads live comments
// from the local GRDB store (populated by Electric sync) and routes
// create/update/delete through tRPC.
struct CommentThreadView: View {
    let issueId: String

    @Environment(AppDependencies.self) private var deps
    @State private var comments: [CommentEntity] = []
    @State private var users: [String: UserEntity] = [:]
    @State private var draft: String = ""
    @State private var submitting = false
    @State private var editingCommentId: String?
    @State private var observationTask: Task<Void, Never>?

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(comments.isEmpty ? "Comments" : "Comments (\(comments.count))")
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Spacer()
            }

            if comments.isEmpty {
                Text("No comments yet. Be the first to add one.")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }

            ForEach(comments, id: \.id) { comment in
                CommentRow(
                    comment: comment,
                    author: users[comment.authorId],
                    isAuthor: comment.authorId == deps.auth.userId,
                    isAdmin: deps.auth.isAdmin,
                    isEditing: editingCommentId == comment.id,
                    onEdit: { editingCommentId = comment.id },
                    onCancelEdit: { editingCommentId = nil },
                    onSaveEdit: { newText in
                        do {
                            try await deps.commentsApi.update(id: comment.id, text: newText)
                            editingCommentId = nil
                        } catch {
                            // Surfacing this inline would be nicer; for now,
                            // the rejected mutation just leaves the editor open.
                        }
                    },
                    onDelete: {
                        Task { try? await deps.commentsApi.delete(id: comment.id) }
                    }
                )
            }

            HStack(alignment: .bottom, spacing: 8) {
                TextField("Write a comment…", text: $draft, axis: .vertical)
                    .lineLimit(1...5)
                    .textFieldStyle(.plain)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .background(Color.white.opacity(0.04))
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                    .foregroundStyle(.white)

                Button {
                    Task { await submit() }
                } label: {
                    Image(systemName: "paperplane.fill")
                        .padding(8)
                        .background(.blue, in: RoundedRectangle(cornerRadius: 8))
                        .foregroundStyle(.white)
                }
                .disabled(submitting || draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
        .padding(.vertical, 8)
        .onAppear { startObserving() }
        .onDisappear { observationTask?.cancel() }
    }

    private func submit() async {
        let trimmed = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        submitting = true
        defer { submitting = false }
        do {
            try await deps.commentsApi.create(issueId: issueId, text: trimmed)
            draft = ""
        } catch {
            // Surface inline later. For now the draft is preserved.
        }
    }

    private func startObserving() {
        observationTask?.cancel()
        observationTask = Task {
            let commentObs = ValueObservation.tracking { db in
                try CommentEntity
                    .filter(Column("issue_id") == issueId)
                    .order(Column("created_at").asc)
                    .fetchAll(db)
            }
            Task {
                for try await rows in commentObs.values(in: deps.db.dbPool) {
                    self.comments = rows
                }
            }

            let userObs = ValueObservation.tracking { db in
                try UserEntity.fetchAll(db)
            }
            Task {
                for try await rows in userObs.values(in: deps.db.dbPool) {
                    self.users = Dictionary(uniqueKeysWithValues: rows.map { ($0.id, $0) })
                }
            }
        }
    }
}

private struct CommentRow: View {
    let comment: CommentEntity
    let author: UserEntity?
    let isAuthor: Bool
    let isAdmin: Bool
    let isEditing: Bool
    let onEdit: () -> Void
    let onCancelEdit: () -> Void
    let onSaveEdit: (String) async -> Void
    let onDelete: () -> Void

    @State private var draft: String = ""

    private var canModify: Bool { isAuthor || isAdmin }

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Circle()
                .fill(Color.white.opacity(0.08))
                .frame(width: 28, height: 28)
                .overlay(
                    Text(initials)
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                )

            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 6) {
                    Text(displayName)
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
                            Button("Edit") {
                                draft = getCommentBodyText(comment.body)
                                onEdit()
                            }
                            Button("Delete", role: .destructive, action: onDelete)
                        } label: {
                            Image(systemName: "ellipsis")
                                .font(.caption)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        }
                    }
                }

                if isEditing {
                    TextField("Edit comment", text: $draft, axis: .vertical)
                        .lineLimit(1...5)
                        .textFieldStyle(.plain)
                        .padding(.horizontal, 8)
                        .padding(.vertical, 6)
                        .background(Color.white.opacity(0.04))
                        .clipShape(RoundedRectangle(cornerRadius: 6))
                        .foregroundStyle(.white)
                    HStack {
                        Button("Save") {
                            let trimmed = draft.trimmingCharacters(in: .whitespacesAndNewlines)
                            guard !trimmed.isEmpty else {
                                onCancelEdit()
                                return
                            }
                            Task { await onSaveEdit(trimmed) }
                        }
                        .buttonStyle(.borderedProminent)
                        .controlSize(.small)
                        Button("Cancel", action: onCancelEdit)
                            .controlSize(.small)
                    }
                } else {
                    Text(getCommentBodyText(comment.body))
                        .font(.callout)
                        .foregroundStyle(.white)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
        .padding(.vertical, 4)
    }

    private var displayName: String {
        author?.name ?? author?.email ?? "Someone"
    }

    private var initials: String {
        let source = displayName
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
}
