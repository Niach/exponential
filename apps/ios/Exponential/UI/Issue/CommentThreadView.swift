import ExpUI
import ExpCore
import SwiftUI
import GRDB
import MarkdownUI

// Mirror of apps/web/src/components/issue-timeline.tsx. Reads live comments
// from the local GRDB store (populated by Electric sync) and routes
// create/update/delete through tRPC. Renders four comment kinds:
//
// - regular: a normal human (or agent terminal) message. Error-shaped
//   regular messages get a Retry button.
// - question: agent question awaiting a human answer.
// - activity: compact, muted single-line tool-call updates.
// - plan: a bordered card with markdown body; the latest plan gets
//   Approve / Request changes buttons when state == awaiting_approval.
struct CommentThreadView: View {
    let issue: IssueEntity
    let canApprovePlan: Bool

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var comments: [CommentEntity] = []
    @State private var events: [IssueEventEntity] = []
    @State private var users: [String: UserEntity] = [:]
    @State private var draft: String = ""
    @State private var submitting = false
    @State private var editingCommentId: String?
    @State private var pendingPlanAction = false
    @State private var pendingRetry = false
    @State private var observationTask: Task<Void, Never>?

    // Linear-style activity timeline: comments + issue_events merged by time.
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

    private var timeline: [TimelineItem] {
        (comments.map { TimelineItem.comment($0) } + events.map { TimelineItem.event($0) })
            .sorted { $0.createdAt < $1.createdAt }
    }

    private var latestPlanCommentId: String? {
        comments.last(where: { $0.commentKind == .plan })?.id
    }

    // The Retry CTA attaches to the most recent error-shaped terminal
    // comment so it stays anchored when newer comments roll in.
    private var retryAnchorCommentId: String? {
        comments.last(where: { isErrorComment($0) })?.id
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text(comments.isEmpty ? "Comments" : "Comments (\(comments.count))")
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
                case .comment(let comment): rowFor(comment)
                case .event(let event): eventRow(event)
                }
            }

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
    private func rowFor(_ comment: CommentEntity) -> some View {
        switch comment.commentKind {
        case .regular:
            RegularCommentRow(
                comment: comment,
                author: users[comment.authorId],
                isAuthor: comment.authorId == deps.auth.userId,
                isAdmin: deps.auth.isAdmin,
                isEditing: editingCommentId == comment.id,
                showRetry: comment.id == retryAnchorCommentId,
                retrying: pendingRetry && comment.id == retryAnchorCommentId,
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
                },
                onRetry: { Task { await retry() } }
            )
        case .question:
            QuestionCommentRow(comment: comment, author: users[comment.authorId])
        case .plan:
            PlanCommentRow(
                comment: comment,
                isLatestPlan: comment.id == latestPlanCommentId,
                issueState: issue.agentPlanState,
                approvedAt: issue.agentPlanApprovedAt,
                approvedBy: users[issue.agentPlanApprovedBy ?? ""],
                canApprovePlan: canApprovePlan,
                isApproving: pendingPlanAction,
                onApprove: { Task { await approvePlan() } },
                onRequestChanges: { Task { await requestChanges() } }
            )
        }
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

    private func approvePlan() async {
        pendingPlanAction = true
        defer { pendingPlanAction = false }
        try? await deps.agentPlanApi.approvePlan(accountId: accountId, issueId: issue.id)
    }

    private func requestChanges() async {
        pendingPlanAction = true
        defer { pendingPlanAction = false }
        try? await deps.agentPlanApi.requestChanges(accountId: accountId, issueId: issue.id)
    }

    private func retry() async {
        pendingRetry = true
        defer { pendingRetry = false }
        try? await deps.agentPlanApi.retry(accountId: accountId, issueId: issue.id)
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

    // Compact Linear-style activity line (status/assignee/label/PR/plan/error).
    @ViewBuilder
    private func eventRow(_ event: IssueEventEntity) -> some View {
        let who = event.actorUserId.flatMap { users[$0] }.map { $0.name ?? $0.email } ?? "Someone"
        let verb: String = {
            switch event.type {
            case "status_changed": return "changed the status"
            case "assignee_changed": return "changed the assignee"
            case "label_added": return "added a label"
            case "label_removed": return "removed a label"
            case "pr_opened": return "opened a pull request"
            case "pr_merged": return "merged the pull request"
            case "plan_ready": return "posted a plan for review"
            case "agent_error": return "hit an error"
            case "agent_started": return "started working"
            case "agent_question": return "asked a question"
            case "agent_answer": return "answered the agent"
            default: return event.type.replacingOccurrences(of: "_", with: " ")
            }
        }()
        HStack(spacing: 8) {
            Circle()
                .fill(.white.opacity(TextOpacity.tertiary))
                .frame(width: 6, height: 6)
            Text("\(who) \(verb)")
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
    let showRetry: Bool
    let retrying: Bool
    let onEdit: () -> Void
    let onCancelEdit: () -> Void
    let onSaveEdit: (String) async -> Void
    let onDelete: () -> Void
    let onRetry: () -> Void

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

                if showRetry {
                    Button {
                        onRetry()
                    } label: {
                        HStack(spacing: 6) {
                            if retrying {
                                ProgressView().controlSize(.mini).tint(.white)
                            } else {
                                Image(systemName: "arrow.clockwise")
                            }
                            Text("Retry")
                        }
                        .font(.caption.weight(.medium))
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                        .glassButton()
                    }
                    .buttonStyle(.plain)
                    .disabled(retrying)
                    .padding(.top, 4)
                }
            }
        }
        .padding(.vertical, 4)
    }
}

// MARK: - Question comment (agent)

private struct QuestionCommentRow: View {
    let comment: CommentEntity
    let author: UserEntity?

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            ZStack {
                Circle()
                    .fill(Color.purple.opacity(0.18))
                    .frame(width: 28, height: 28)
                Image(systemName: "questionmark.circle.fill")
                    .font(.caption)
                    .foregroundStyle(.purple)
            }

            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 6) {
                    Text(displayName(for: author, fallback: "Agent"))
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.white)
                    Text("asks")
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    Text(relativeDate(comment.createdAt))
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    Spacer()
                }
                Markdown(getCommentBodyText(comment.body))
                    .markdownTheme(.gitHub)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .padding(10)
        .background(Color.purple.opacity(0.06))
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .stroke(Color.purple.opacity(0.25), lineWidth: 0.5)
        )
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .padding(.vertical, 2)
    }
}

// MARK: - Plan comment (agent plan revision)

private struct PlanCommentRow: View {
    let comment: CommentEntity
    let isLatestPlan: Bool
    let issueState: String?
    let approvedAt: String?
    let approvedBy: UserEntity?
    let canApprovePlan: Bool
    let isApproving: Bool
    let onApprove: () -> Void
    let onRequestChanges: () -> Void

    private var awaitingApproval: Bool {
        isLatestPlan && issueState == "awaiting_approval"
    }

    private var isApproved: Bool {
        approvedAt != nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 6) {
                Image(systemName: "doc.text.fill")
                    .font(.caption)
                    .foregroundStyle(.blue)
                Text("Plan")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.white)
                Text(relativeDate(comment.createdAt))
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                Spacer()
                if isApproved && isLatestPlan {
                    HStack(spacing: 4) {
                        Image(systemName: "checkmark.circle.fill")
                            .font(.caption2)
                            .foregroundStyle(.green)
                        Text("Approved")
                            .font(.caption2.weight(.medium))
                            .foregroundStyle(.green)
                    }
                }
            }

            Markdown(getCommentBodyText(comment.body))
                .markdownTheme(.gitHub)
                .frame(maxWidth: .infinity, alignment: .leading)

            if awaitingApproval && canApprovePlan {
                HStack(spacing: 8) {
                    Button {
                        onApprove()
                    } label: {
                        HStack(spacing: 6) {
                            if isApproving {
                                ProgressView().controlSize(.mini).tint(.white)
                            } else {
                                Image(systemName: "checkmark")
                            }
                            Text("Approve")
                        }
                        .font(.caption.weight(.semibold))
                        .padding(.horizontal, 12)
                        .padding(.vertical, 6)
                        .background(.green.opacity(0.18), in: RoundedRectangle(cornerRadius: 6))
                        .foregroundStyle(.green)
                    }
                    .buttonStyle(.plain)
                    .disabled(isApproving)

                    Button {
                        onRequestChanges()
                    } label: {
                        Text("Request changes")
                            .font(.caption.weight(.medium))
                            .padding(.horizontal, 12)
                            .padding(.vertical, 6)
                            .glassButton()
                    }
                    .buttonStyle(.plain)
                    .disabled(isApproving)
                }
            }
        }
        .padding(12)
        .background(Color.blue.opacity(0.06))
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .stroke(Color.blue.opacity(0.25), lineWidth: 0.5)
        )
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .padding(.vertical, 2)
    }
}

// MARK: - Shared helpers

// Detect agent "terminal error" comments — same patterns the web timeline
// uses. Tests-failed/agent-error/no-repo/no-auth show a Retry CTA;
// "PR opened" is terminal but not an error.
private let errorBodyPatterns: [String] = [
    "^Tests failed after retry",
    "^Agent encountered an error",
    "^No GitHub repo linked",
    "Companion is not authenticated to GitHub",
]

fileprivate func isErrorComment(_ comment: CommentEntity) -> Bool {
    guard comment.commentKind == .regular else { return false }
    let body = getCommentBodyText(comment.body)
    for pattern in errorBodyPatterns {
        if body.range(of: pattern, options: .regularExpression) != nil {
            return true
        }
    }
    return false
}

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
