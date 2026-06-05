import ExpUI
import ExpCore
import SwiftUI
import GRDB
import MarkdownUI

// The human conversation timeline. Reads live comments + issue_events from the
// local GRDB store (populated by Electric sync) and routes create/update/delete
// through tRPC. Renders:
//
// - regular comments (human or agent terminal messages)
// - non-agent events (status/assignee/label changes) inline
// - a collapsible "Agent activity" feed for agent lifecycle events
//
// Plan/question comments and the plan approval / retry affordances now live in
// the AgentPlanPanel (a sibling above this view), so this view stays a plain
// human thread.
struct CommentThreadView: View {
    let issue: IssueEntity

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var comments: [CommentEntity] = []
    @State private var events: [IssueEventEntity] = []
    @State private var users: [String: UserEntity] = [:]
    @State private var draft: String = ""
    @State private var submitting = false
    @State private var editingCommentId: String?
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
        let nonAgentEvents = events.filter { !agentEventTypes.contains($0.type) }
        return (humanComments.map { TimelineItem.comment($0) }
            + nonAgentEvents.map { TimelineItem.event($0) })
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

            AgentActivityFeed(events: events, users: users)

            // Plain-text composer for now. The rich-markdown composer lands
            // in a follow-up so the draft body shape can be coordinated with
            // the web/Android readers.
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

    @ViewBuilder
    private func commentRow(_ comment: CommentEntity) -> some View {
        RegularCommentRow(
            comment: comment,
            author: users[comment.authorId],
            isAuthor: comment.authorId == deps.auth.userId,
            isAdmin: deps.auth.isAdmin,
            isEditing: editingCommentId == comment.id,
            onEdit: { editingCommentId = comment.id },
            onCancelEdit: { editingCommentId = nil },
            onSaveEdit: { newText in
                do {
                    try await deps.commentsApi.update(accountId: accountId, id: comment.id, text: newText)
                    editingCommentId = nil
                } catch {}
            },
            onDelete: {
                Task { try? await deps.commentsApi.delete(accountId: accountId, id: comment.id) }
            }
        )
    }

    private func submit() async {
        let trimmed = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        submitting = true
        defer { submitting = false }
        do {
            try await deps.commentsApi.create(accountId: accountId, issueId: issue.id, text: trimmed)
            draft = ""
        } catch {}
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

    // Compact Linear-style activity line for non-agent events (status/assignee/
    // label). Agent lifecycle events go to the AgentActivityFeed instead.
    @ViewBuilder
    private func eventRow(_ event: IssueEventEntity) -> some View {
        let who = event.actorUserId.flatMap { users[$0] }.map { $0.name ?? $0.email } ?? "Someone"
        HStack(spacing: 8) {
            Circle()
                .fill(.white.opacity(TextOpacity.tertiary))
                .frame(width: 6, height: 6)
            Text("\(who) \(agentEventVerb(event.type))")
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
    let onEdit: () -> Void
    let onCancelEdit: () -> Void
    let onSaveEdit: (String) async -> Void
    let onDelete: () -> Void

    @State private var draft: String = ""

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
                            Button("Edit") {
                                draft = getCommentBodyText(comment.body)
                                onEdit()
                            }
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
                    Markdown(getCommentBodyText(comment.body))
                        .markdownTheme(.gitHub)
                        .frame(maxWidth: .infinity, alignment: .leading)
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
