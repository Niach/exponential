import ExpUI
import ExpCore
import SwiftUI
import GRDB

// The activity timeline (EXP-240 redesign). Reads live comments + issue_events
// from the local GRDB store (populated by Electric sync) and routes comment
// edit/delete through tRPC. Renders a synthesized "created the issue" first
// item, comments as glass cards with the author avatar in a leading gutter,
// events as dot rows on a connecting vertical rail, and folds runs of >2
// consecutive events behind a "Show N activity items" expander. Composing NEW
// comments moved to the docked bottom-bar composer (IssueDetailBottomBar).
struct CommentThreadView: View {
    let issue: IssueEntity

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var comments: [CommentEntity] = []
    @State private var events: [IssueEventEntity] = []
    @State private var users: [String: UserEntity] = [:]
    @State private var labels: [String: LabelEntity] = [:]
    @State private var boards: [String: BoardEntity] = [:]
    @State private var editingCommentId: String?
    // The rich editor backing the comment currently being edited (re-seeded on
    // each Edit tap; only one comment edits at a time).
    @State private var editEditor = IssueEditorModel()
    // Opened event runs, keyed by the run's first event id (survives sync
    // re-emits — see collapseTimeline).
    @State private var expandedRuns: Set<String> = []
    @State private var observationTask: Task<Void, Never>?

    private var humanComments: [CommentEntity] {
        comments.filter { $0.commentKind == .regular }
    }

    private var timeline: [TimelineItem] {
        let created = TimelineItem.created(
            actorId: issue.creatorId,
            createdAt: issue.createdAt,
            isWidget: issue.source == DomainContract.issueSourceWidget
        )
        // (createdAt, id) — the deterministic tie-break Android uses, so
        // same-timestamp items order identically on both platforms.
        let rest = (humanComments.map { TimelineItem.comment($0) }
            + events.map { TimelineItem.event($0) })
            .sorted { ($0.createdAt, $0.id) < ($1.createdAt, $1.id) }
        return [created] + rest
    }

    var body: some View {
        let rows = collapseTimeline(timeline, expandedRuns: expandedRuns)
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("Activity")
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .accessibilityIdentifier("comment-thread-header")
                Spacer()
            }

            VStack(alignment: .leading, spacing: 0) {
                ForEach(Array(rows.enumerated()), id: \.element.id) { index, row in
                    displayRow(
                        row,
                        showTop: index > 0,
                        showBottom: index < rows.count - 1
                    )
                }
            }
        }
        .padding(.vertical, 8)
        .onAppear { startObserving() }
        .onDisappear { observationTask?.cancel() }
    }

    // MARK: - Rows

    @ViewBuilder
    private func displayRow(_ row: TimelineDisplayRow, showTop: Bool, showBottom: Bool) -> some View {
        switch row {
        case .item(let item):
            switch item {
            case let .created(actorId, createdAt, isWidget):
                createdRow(actorId: actorId, createdAt: createdAt, isWidget: isWidget, showTop: showTop, showBottom: showBottom)
            case .comment(let comment):
                commentRow(comment, showTop: showTop, showBottom: showBottom)
            case .event(let event):
                eventRow(event, showTop: showTop, showBottom: showBottom)
            }
        case let .collapsedRun(key, runEvents):
            collapsedRunRow(key: key, events: runEvents, showTop: showTop, showBottom: showBottom)
        }
    }

    /// Synthesized first item: "«creator» created the issue" (widget issues
    /// have no user creator — the feedback widget is the actor).
    @ViewBuilder
    private func createdRow(actorId: String?, createdAt: String, isWidget: Bool, showTop: Bool, showBottom: Bool) -> some View {
        let who = isWidget
            ? "Feedback widget"
            : memberDisplayName(actorId.flatMap { users[$0] }, id: actorId)
        let time = relativeDate(createdAt)
        TimelineRow(
            showTop: showTop,
            showBottom: showBottom,
            markerSize: 16,
            topPadding: 5,
            bottomPadding: 5,
            marker: { eventDot }
        ) {
            Text(time.isEmpty ? "\(who) created the issue" : "\(who) created the issue · \(time)")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
        }
    }

    // Compact Linear-style activity line for issue events (status/assignee/
    // label/PR changes).
    @ViewBuilder
    private func eventRow(_ event: IssueEventEntity, showTop: Bool, showBottom: Bool) -> some View {
        let who = memberDisplayName(event.actorUserId.flatMap { users[$0] }, id: event.actorUserId)
        let phrase = eventPhrase(event, users: users, labels: labels, boards: boards)
        // Append a relative timestamp (EXP-169) — only when it parses, so an
        // unparseable created_at never leaves a dangling " · ".
        let time = relativeDate(event.createdAt)
        TimelineRow(
            showTop: showTop,
            showBottom: showBottom,
            markerSize: 16,
            topPadding: 5,
            bottomPadding: 5,
            marker: { eventDot }
        ) {
            Text(time.isEmpty ? "\(who) \(phrase)" : "\(who) \(phrase) · \(time)")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
        }
    }

    /// A folded run of consecutive events: one expander row; tapping opens the
    /// run in place (animated unless Reduce Motion).
    @ViewBuilder
    private func collapsedRunRow(key: String, events: [IssueEventEntity], showTop: Bool, showBottom: Bool) -> some View {
        TimelineRow(
            showTop: showTop,
            showBottom: showBottom,
            markerSize: 28,
            topPadding: 5,
            bottomPadding: 5,
            marker: {
                Image(systemName: "ellipsis")
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        ) {
            Button {
                if reduceMotion {
                    expandedRuns.insert(key)
                } else {
                    withAnimation(.easeInOut(duration: 0.2)) {
                        _ = expandedRuns.insert(key)
                    }
                }
            } label: {
                Text("Show \(events.count) activity items")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .padding(.horizontal, 12)
                    .padding(.vertical, 6)
                    .glassButton()
            }
            .buttonStyle(.plain)
        }
    }

    private var eventDot: some View {
        Circle()
            .fill(.white.opacity(0.25))
            .frame(width: 6, height: 6)
    }

    @ViewBuilder
    private func commentRow(_ comment: CommentEntity, showTop: Bool, showBottom: Bool) -> some View {
        TimelineRow(
            showTop: showTop,
            showBottom: showBottom,
            markerSize: 28,
            topPadding: 6,
            bottomPadding: 6,
            marker: { avatar(author: users[comment.authorId], id: comment.authorId) }
        ) {
            RegularCommentRow(
                comment: comment,
                author: users[comment.authorId],
                authorId: comment.authorId,
                isAuthor: comment.authorId == deps.auth.userId,
                isAdmin: deps.auth.isAdmin,
                isEditing: editingCommentId == comment.id,
                editEditor: editEditor,
                baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
                accountId: accountId,
                httpClient: deps.httpClient,
                mentionMembers: users.values.map { MentionMember(name: $0.name ?? $0.email, email: $0.email) },
                resolveIssueRef: { identifier in resolveIssueRef(identifier) },
                onOpenIssue: { issueId in deps.deepLinkBus.navigateToIssue(issueId) },
                onEdit: {
                    // Fresh model per edit, seeded from the comment's markdown — the
                    // same rich block editor as the composer (images, mentions,
                    // lists, #issue-ref pills). Resolver/search set BEFORE load so
                    // existing refs decorate on seed.
                    let editor = IssueEditorModel()
                    editor.issueRefResolver = { resolveIssueRef($0) }
                    editor.issueRefSearch = { searchIssueRefs($0) }
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
    }

    /// The comment-edit image uploader (the NEW-comment path lives in
    /// IssueDetailBottomBar with its own copy).
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
    /// pills in comment bodies (render-only, same team only; unresolved
    /// refs stay plain text).
    private func resolveIssueRef(_ identifier: String) -> String? {
        IssueRefLookup.resolve(identifier, scope: .issue(id: issue.id), db: deps.db, accountId: accountId)
    }

    /// Issues offered by the comment editors' #-autocomplete (team-scoped;
    /// identifier + title substring match).
    private func searchIssueRefs(_ query: String) -> [IssueRefCandidate] {
        IssueRefLookup.search(query, scope: .issue(id: issue.id), db: deps.db, accountId: accountId)
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

            // Board names for board_moved events (EXP-57).
            let boardObs = ValueObservation.tracking { db in
                try BoardEntity.fetchAll(db)
            }
            Task {
                for try await rows in boardObs.values(in: pool) {
                    self.boards = Dictionary(uniqueKeysWithValues: rows.map { ($0.id, $0) })
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
}

// MARK: - Gutter rail

/// One timeline row: a fixed-width leading gutter holding the marker (dot or
/// avatar) with a 1.5pt vertical rail connecting to the neighboring rows,
/// drawn as a background so it spans the row's full height. `showTop`/
/// `showBottom` trim the rail at the timeline's ends.
private struct TimelineRow<Marker: View, Content: View>: View {
    let showTop: Bool
    let showBottom: Bool
    /// Height of the marker slot; the marker centers in it and the rail
    /// breaks around it.
    let markerSize: CGFloat
    let topPadding: CGFloat
    let bottomPadding: CGFloat
    @ViewBuilder let marker: () -> Marker
    @ViewBuilder let content: () -> Content

    private let gutterWidth: CGFloat = 26
    private let railBreathing: CGFloat = 3

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            marker()
                .frame(width: gutterWidth, height: markerSize)
            content()
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.top, topPadding)
        .padding(.bottom, bottomPadding)
        .background(alignment: .leading) {
            VStack(spacing: 0) {
                Rectangle()
                    .fill(Color.white.opacity(showTop ? 0.09 : 0))
                    .frame(width: 1.5)
                    .frame(height: max(0, topPadding - railBreathing))
                Color.clear
                    .frame(height: markerSize + railBreathing * 2)
                Rectangle()
                    .fill(Color.white.opacity(showBottom ? 0.09 : 0))
                    .frame(width: 1.5)
                    .frame(maxHeight: .infinity)
            }
            .frame(width: gutterWidth)
        }
    }
}

// MARK: - Regular comment

private struct RegularCommentRow: View {
    let comment: CommentEntity
    let author: UserEntity?
    // The author's user id, so a not-synced author still gets a stable pseudonym
    // instead of the generic fallback.
    let authorId: String
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
    // Read-only display model for the comment body (same block stack as the
    // editors); rebuilt only when the body text actually changes.
    @State private var displayModel = IssueEditorModel()
    @State private var displayedBody: String?

    private var canModify: Bool { isAuthor || isAdmin }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                Text(displayName(for: author, id: authorId))
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
                // images, @mentions, and #issue-refs all work in edit mode too.
                MarkdownEditor(
                    model: editEditor,
                    placeholder: "Edit comment…",
                    baseURL: baseURL,
                    accountId: accountId,
                    httpClient: httpClient,
                    mentionMembers: mentionMembers,
                    onIssueRefTap: { issueId in onOpenIssue(issueId) }
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
                // Read-only render through the SAME block stack as the
                // description and the composer (no MarkdownUI — its
                // optimized opaque-Body metadata hard-crashed the iOS 27
                // runtime, and one dependency for one read-only view isn't
                // worth that class of bug). The model decorates @mentions
                // and resolved `#IDENTIFIER` refs as tappable pills; the
                // raw stored markdown stays untouched (the edit path
                // reseeds from it).
                MarkdownEditor(
                    model: displayModel,
                    placeholder: "",
                    baseURL: baseURL,
                    accountId: accountId,
                    httpClient: httpClient,
                    onIssueRefTap: { issueId in onOpenIssue(issueId) },
                    isReadOnly: true
                )
                .frame(maxWidth: .infinity, alignment: .leading)
                .task(id: getCommentBodyText(comment.body)) {
                    let text = getCommentBodyText(comment.body)
                    guard displayedBody != text else { return }
                    displayedBody = text
                    let model = IssueEditorModel()
                    model.mentionMembers = mentionMembers
                    model.issueRefResolver = resolveIssueRef
                    model.load(markdown: text, baseURL: baseURL)
                    displayModel = model
                }
            }
        }
        // Glass comment card (EXP-240) — the avatar lives in the timeline
        // gutter, not inside the card.
        .padding(.horizontal, 12)
        .padding(.top, 8)
        .padding(.bottom, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .glassSection()
    }
}

// MARK: - Shared helpers

private func avatar(author: UserEntity?, id: String?) -> some View {
    Circle()
        .fill(Color.white.opacity(0.08))
        .frame(width: 28, height: 28)
        .overlay(
            Text(initials(for: author, id: id))
                .font(.caption2.weight(.semibold))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
        )
}

private func displayName(for author: UserEntity?, id: String?, fallback: String = "Someone") -> String {
    memberDisplayName(author, id: id, generic: fallback)
}

private func initials(for author: UserEntity?, id: String?) -> String {
    let source = displayName(for: author, id: id)
    let parts = source.split(separator: " ").prefix(2)
    return parts.map { $0.first.map(String.init) ?? "" }.joined().uppercased()
}

private func relativeDate(_ s: String) -> String {
    // Electric syncs created_at as Postgres text (space separator, hour-only
    // offset), which ISO8601DateFormatter alone rejects — WireTimestamps
    // handles both wire forms (EXP-169).
    guard let date = WireTimestamps.parse(s) else { return "" }
    let formatter = RelativeDateTimeFormatter()
    formatter.unitsStyle = .short
    return formatter.localizedString(for: date, relativeTo: Date())
}
