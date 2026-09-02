import ExpUI
import ExpCore
import PhotosUI
import SwiftUI
import GRDB
import UniformTypeIdentifiers

// The activity timeline (EXP-240 redesign). Reads live comments + issue_events
// from the local GRDB store (populated by Electric sync) and routes comment
// edit/delete through tRPC. Renders a synthesized "created the issue" first
// item, comments as glass cards with the author avatar in a leading gutter,
// events as dot rows on a connecting vertical rail, and folds runs of >2
// consecutive events behind a "Show N activity items" expander. Composing NEW
// comments moved to the docked bottom-bar composer (IssueDetailBottomBar).
struct CommentThreadView: View {
    let issue: IssueEntity
    /// Solo teams hide the comment editors' @ affordance (EXP-246) — same
    /// gate as the assignee chip, threaded from the detail view model.
    let singleMemberTeam: Bool
    /// The editor backing the comment currently being edited (re-seeded on each
    /// Edit tap; only one comment edits at a time). Owned by the DETAIL view,
    /// not by this timeline: its `@`/`#`/`:` menu is mounted screen-level, in
    /// the bottom safe-area inset that rides above the keyboard (EXP-592).
    @Binding var editEditor: IssueEditorModel
    /// Horizontal padding of the hosting column, escaped by the top rule so the
    /// line runs edge to edge (EXP-327).
    var hostPadding: CGFloat = 20

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.motion) private var motion
    @State private var comments: [CommentEntity] = []
    @State private var events: [IssueEventEntity] = []
    @State private var users: [String: UserEntity] = [:]
    @State private var labels: [String: LabelEntity] = [:]
    @State private var boards: [String: BoardEntity] = [:]
    /// EXP-595: every synced team's `issue_statuses` rows — the status-change
    /// rows' glyphs resolve against this issue's team subset, so the timeline
    /// shows the same colored status icon the list and the picker do.
    @State private var statusRows: [IssueStatusEntity] = []
    /// EXP-554 — this issue's comment-linked attachment rows, grouped by
    /// `comment_id` and ordered (created_at, id) like every other timeline list.
    @State private var attachmentsByComment: [String: [AttachmentEntity]] = [:]
    @State private var editingCommentId: String?
    // Opened event runs, keyed by the run's first event id (survives sync
    // re-emits — see collapseTimeline).
    @State private var expandedRuns: Set<String> = []
    // Each observation loop is stored and cancelled individually — a single
    // wrapper task would NOT propagate cancellation into unstructured inner
    // `Task {}` loops, and the view re-arms on every appear, so leaked loops
    // would accumulate per push/pop.
    @State private var observationTasks: [Task<Void, Never>] = []

    private var humanComments: [CommentEntity] {
        comments.filter { $0.commentKind == .regular }
    }

    /// This issue's team statuses in render order (EXP-314). Empty while the
    /// boards/statuses shapes are still syncing — `IssueStatusResolver.resolve`
    /// degrades to the constructed builtin defaults, so rendering never fails.
    private var teamStatuses: [ResolvedIssueStatus] {
        guard let teamId = boards[issue.boardId]?.teamId else { return [] }
        return IssueStatusResolver.teamStatuses(statusRows.filter { $0.teamId == teamId })
    }

    private var timeline: [TimelineItem] {
        let created = TimelineItem.created(
            actorId: issue.creatorId,
            createdAt: issue.createdAt,
            source: issue.source
        )
        // (createdAt, id) — the deterministic tie-break Android uses, so
        // same-timestamp items order identically on both platforms.
        // EXP-530: server `created` events are suppressed (nil phrase) — drop
        // them HERE too, so collapsed-run counts never include hidden rows.
        let visibleEvents = events.filter {
            eventPhrase($0, users: users, labels: labels, boards: boards) != nil
        }
        let rest = (humanComments.map { TimelineItem.comment($0) }
            + visibleEvents.map { TimelineItem.event($0) })
            .sorted { ($0.createdAt, $0.id) < ($1.createdAt, $1.id) }
        return [created] + rest
    }

    var body: some View {
        let rows = collapseTimeline(timeline, expandedRuns: expandedRuns)
        VStack(alignment: .leading, spacing: 8) {
            // A rule the full width of the screen, not just the reading column:
            // activity reads as its own region below the issue (EXP-327).
            Rectangle()
                .fill(Color.white.opacity(0.08))
                .frame(height: 0.5)
                .padding(.horizontal, -hostPadding)
                .padding(.bottom, 4)
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
        .onDisappear { stopObserving() }
    }

    // MARK: - Rows

    @ViewBuilder
    private func displayRow(_ row: TimelineDisplayRow, showTop: Bool, showBottom: Bool) -> some View {
        switch row {
        case .item(let item):
            switch item {
            case let .created(actorId, createdAt, source):
                createdRow(actorId: actorId, createdAt: createdAt, source: source, showTop: showTop, showBottom: showBottom)
            case .comment(let comment):
                commentRow(comment, showTop: showTop, showBottom: showBottom)
            case .event(let event):
                eventRow(event, showTop: showTop, showBottom: showBottom)
            }
        case let .collapsedRun(key, runEvents):
            collapsedRunRow(key: key, events: runEvents, showTop: showTop, showBottom: showBottom)
        }
    }

    /// Synthesized first item: "«creator» created the issue" (widget/agent
    /// issues have no user creator — their origin is the actor).
    @ViewBuilder
    private func createdRow(actorId: String?, createdAt: String, source: String?, showTop: Bool, showBottom: Bool) -> some View {
        let who = source == DomainContract.issueSourceWidget
            ? "Feedback widget"
            : source == DomainContract.issueSourceAgent
                ? "Agent"
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
        // Nil phrase = a suppressed event type (the timeline filter already
        // drops them; this guard keeps a stray one from rendering munged).
        if let phrase = eventPhrase(event, users: users, labels: labels, boards: boards) {
            // Append a relative timestamp (EXP-169) — only when it parses, so
            // an unparseable created_at never leaves a dangling " · ".
            let time = relativeDate(event.createdAt)
            TimelineRow(
                showTop: showTop,
                showBottom: showBottom,
                markerSize: 16,
                topPadding: 5,
                bottomPadding: 5,
                marker: { eventMarker(event) }
            ) {
                Text(time.isEmpty ? "\(who) \(phrase)" : "\(who) \(phrase) · \(time)")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            }
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
                AppIcon(AppIcons.uiMore, size: 11)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        ) {
            GlassPillButton("Show \(events.count) activity items") {
                // EXP-523: `motion.standard` is nil under Reduce Motion and
                // `withAnimation(nil)` applies the change instantly, so the
                // explicit branch this used to carry is gone.
                withAnimation(motion.standard) {
                    _ = expandedRuns.insert(key)
                }
            }
        }
    }

    private var eventDot: some View {
        Circle()
            .fill(.white.opacity(0.25))
            .frame(width: 6, height: 6)
    }

    /// The leading glyph of one event row (EXP-595 — web/desktop parity): the
    /// event type's shared-registry concept icon in the muted text color, a
    /// status change's resolved status icon in its color, and the plain dot
    /// only for unknown event types.
    @ViewBuilder
    private func eventMarker(_ event: IssueEventEntity) -> some View {
        switch eventGlyph(event, statuses: teamStatuses) {
        case .status(let status)?:
            AppIcon(status.iconName, size: 12)
                .foregroundStyle(status.color)
        case .plain(let name)?:
            AppIcon(name, size: 12)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
        case nil:
            eventDot
        }
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
                attachments: attachmentsByComment[comment.id] ?? [],
                issueId: issue.id,
                author: users[comment.authorId],
                authorId: comment.authorId,
                isAuthor: comment.authorId == deps.auth.userId,
                isEditing: editingCommentId == comment.id,
                editEditor: editEditor,
                singleMemberTeam: singleMemberTeam,
                baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
                accountId: accountId,
                httpClient: deps.httpClient,
                mentionMembers: users.values.map { MentionMember(name: $0.name ?? $0.email, email: $0.email) },
                resolveIssueRef: { identifier in resolveIssueRef(identifier) },
                resolveIssueRefTitle: { identifier in resolveIssueRefTitle(identifier) },
                resolveIssueRefStatus: { identifier in resolveIssueRefStatus(identifier) },
                onOpenIssue: { issueId in deps.deepLinkBus.navigateToIssue(issueId) },
                onEdit: {
                    // Fresh model per edit, seeded from the comment's markdown — the
                    // same rich block editor as the composer (images, mentions,
                    // lists, #issue-ref pills). Resolver/search set BEFORE load so
                    // existing refs decorate on seed.
                    let editor = IssueEditorModel()
                    editor.issueRefResolver = { resolveIssueRef($0) }
                    editor.issueRefTitleResolver = { resolveIssueRefTitle($0) }
                    editor.issueRefStatusResolver = { resolveIssueRefStatus($0) }
                    editor.issueRefSearch = { searchIssueRefs($0) }
                    editor.load(
                        markdown: getCommentBodyText(comment.body),
                        baseURL: deps.auth.instanceBaseURL(forAccountId: accountId)
                    )
                    editEditor = editor
                    editingCommentId = comment.id
                },
                onCancelEdit: { editingCommentId = nil },
                onSaveEdit: { attachmentIds in
                    // The editor's own keyboard toolbar can still inline an
                    // image into the BODY (that path predates EXP-554 and stays
                    // as it was); `attachmentIds` is the new, separate list of
                    // linked rows — a FULL desired set, so anything the user
                    // removed is hard-deleted server-side.
                    let ok = await editEditor.commitPendingImages(uploader: makeCommentImageUploader())
                    guard ok, !editEditor.hasUncommittedDrafts else { return false }
                    let md = editEditor.currentMarkdown().trimmingCharacters(in: .whitespacesAndNewlines)
                    guard !md.isEmpty || !attachmentIds.isEmpty else { return false }
                    do {
                        try await deps.commentsApi.update(
                            accountId: accountId,
                            id: comment.id,
                            text: md,
                            attachmentIds: attachmentIds
                        )
                        editingCommentId = nil
                        return true
                    } catch {
                        return false
                    }
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
        let api = deps.attachmentsApi
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
        IssueRefChipCache.chip(identifier, scope: .issue(id: issue.id), db: deps.db, accountId: accountId)?
            .issueId
    }

    /// identifier → issue title for the read-only chips (`#ID <title>`,
    /// EXP-307); same team scoping as the id resolver.
    private func resolveIssueRefTitle(_ identifier: String) -> String? {
        IssueRefChipCache.chip(identifier, scope: .issue(id: issue.id), db: deps.db, accountId: accountId)?
            .title
    }

    /// identifier → the status glyph the chip paints over its `#` (EXP-423);
    /// same team scoping as the resolvers above.
    private func resolveIssueRefStatus(_ identifier: String) -> IssueRefStatusInfo? {
        IssueRefChipCache.statusInfo(
            identifier, scope: .issue(id: issue.id), db: deps.db, accountId: accountId)
    }

    /// Issues offered by the comment editors' #-autocomplete (team-scoped;
    /// identifier + title substring match).
    private func searchIssueRefs(_ query: String) -> [IssueRefCandidate] {
        IssueRefLookup.search(query, scope: .issue(id: issue.id), db: deps.db, accountId: accountId)
    }

    private func startObserving() {
        stopObserving() // restartable: the view re-arms on every appear
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let issueId = issue.id

        let commentObs = ValueObservation.tracking { db in
            try CommentEntity
                .filter(Column("issue_id") == issueId)
                .order(Column("created_at").asc)
                .fetchAll(db)
        }
        observationTasks.append(Task {
            do {
                for try await rows in commentObs.values(in: pool) {
                    self.comments = rows
                }
            } catch {}
        })

        let userObs = ValueObservation.tracking { db in
            try UserEntity.fetchAll(db)
        }
        observationTasks.append(Task {
            do {
                for try await rows in userObs.values(in: pool) {
                    self.users = Dictionary(uniqueKeysWithValues: rows.map { ($0.id, $0) })
                }
            } catch {}
        })

        let labelObs = ValueObservation.tracking { db in
            try LabelEntity.fetchAll(db)
        }
        observationTasks.append(Task {
            do {
                for try await rows in labelObs.values(in: pool) {
                    self.labels = Dictionary(uniqueKeysWithValues: rows.map { ($0.id, $0) })
                }
            } catch {}
        })

        // Board names for board_moved events (EXP-57).
        let boardObs = ValueObservation.tracking { db in
            try BoardEntity.fetchAll(db)
        }
        observationTasks.append(Task {
            do {
                for try await rows in boardObs.values(in: pool) {
                    self.boards = Dictionary(uniqueKeysWithValues: rows.map { ($0.id, $0) })
                }
            } catch {}
        })

        // EXP-554: attachments linked to a COMMENT on this issue. Issue-level
        // rows (comment_id NULL) belong to the description/Files rail and are
        // filtered out in SQL so the grouping below never has to guess.
        let attachmentObs = ValueObservation.tracking { db in
            try AttachmentEntity
                .filter(Column("issue_id") == issueId)
                .filter(Column("comment_id") != nil)
                .fetchAll(db)
        }
        observationTasks.append(Task {
            do {
                for try await rows in attachmentObs.values(in: pool) {
                    let ordered = rows.sorted { ($0.createdAt, $0.id) < ($1.createdAt, $1.id) }
                    self.attachmentsByComment = Dictionary(
                        grouping: ordered,
                        by: { $0.commentId ?? "" }
                    )
                }
            } catch {}
        })

        // Team statuses (EXP-595) — resolve the status-change rows' glyph +
        // color against the same vocabulary every other status surface uses.
        let statusObs = ValueObservation.tracking { db in
            try IssueStatusEntity.fetchAll(db)
        }
        observationTasks.append(Task {
            do {
                for try await rows in statusObs.values(in: pool) {
                    self.statusRows = rows
                }
            } catch {}
        })

        let eventObs = ValueObservation.tracking { db in
            try IssueEventEntity
                .filter(Column("issue_id") == issueId)
                .order(Column("created_at").asc)
                .fetchAll(db)
        }
        observationTasks.append(Task {
            do {
                for try await rows in eventObs.values(in: pool) {
                    self.events = rows
                }
            } catch {}
        })
    }

    private func stopObserving() {
        for task in observationTasks { task.cancel() }
        observationTasks = []
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
    /// EXP-554 — the rows whose `comment_id` is this comment, already ordered.
    let attachments: [AttachmentEntity]
    /// Uploads target the ISSUE (the attachment rows are issue-scoped; the
    /// comment link is stamped by `comments.create`/`update`).
    let issueId: String
    let author: UserEntity?
    // The author's user id, so a not-synced author still gets a stable pseudonym
    // instead of the generic fallback.
    let authorId: String
    let isAuthor: Bool
    let isEditing: Bool
    let editEditor: IssueEditorModel
    let singleMemberTeam: Bool
    let baseURL: URL?
    let accountId: String
    let httpClient: HTTPClient?
    let mentionMembers: [MentionMember]
    let resolveIssueRef: (String) -> String?
    let resolveIssueRefTitle: (String) -> String?
    let resolveIssueRefStatus: (String) -> IssueRefStatusInfo?
    let onOpenIssue: (String) -> Void
    let onEdit: () -> Void
    let onCancelEdit: () -> Void
    /// Saves the edit with the FULL desired attachment id list; returns whether
    /// the mutation went through (a failure keeps the composer open).
    let onSaveEdit: ([String]) async -> Bool
    let onDelete: () -> Void

    @Environment(AppDependencies.self) private var deps

    @State private var saving = false
    // Read-only display model for the comment body (same block stack as the
    // editors); rebuilt only when the body text actually changes.
    @State private var displayModel = IssueEditorModel()
    @State private var displayedBody: String?

    // EXP-554 edit state: which already-linked rows survive the save, plus
    // anything newly picked. Removals are permanent once saved (the server hard-
    // deletes rows missing from the list), so nothing is uploaded or dropped
    // until Save is pressed.
    @State private var keptAttachmentIds: Set<String> = []
    @State private var pendingAttachments: [PendingCommentAttachment] = []
    @State private var attachmentError: String?
    @State private var showPhotoPicker = false
    @State private var photoItems: [PhotosPickerItem] = []
    @State private var showFileImporter = false

    // Author-only, no global-admin bypass (EXP-398) — the server refuses the
    // mutation for anyone else, so the menu would only ever be a dead end.
    private var canModify: Bool { isAuthor }

    private var keptAttachments: [AttachmentEntity] {
        attachments.filter { keptAttachmentIds.contains($0.id) }
    }

    private var attachmentsFull: Bool {
        keptAttachments.count + pendingAttachments.count >= AttachmentFiles.maxCommentAttachments
    }

    private var bodyText: String {
        getCommentBodyText(comment.body)
    }

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
                    GlassMenu {
                        GlassMenuItem("Edit", icon: AppIcons.uiEdit, action: onEdit)
                        GlassMenuItem("Delete", icon: AppIcons.uiDelete, destructive: true, action: onDelete)
                    } label: {
                        CircleIconLabel(AppIcons.uiMore)
                            .accessibilityLabel("Comment actions")
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
                // Bounded scroller, not a bare frame clamp — overflow content
                // rendered outside the clamp with the caret detached (EXP-246).
                .boundedEditorHeight(minHeight: 60, maxHeight: 220)
                .padding(.vertical, 2)
                .background(Color.white.opacity(0.04))
                .clipShape(RoundedRectangle(cornerRadius: 6))
                // The `@`/`#`/`:` menu is NOT mounted here: a row deep in the
                // timeline is as likely to sit behind the keyboard as under it.
                // IssueDetailView renders it in its bottom safe-area inset for
                // whichever of its editors holds the keyboard (EXP-592).

                // Already-linked rows, each removable. Removals only take effect
                // on Save — and then they are permanent.
                if !keptAttachments.isEmpty {
                    CommentAttachmentsStrip(attachments: keptAttachments) { id in
                        keptAttachmentIds.remove(id)
                        attachmentError = nil
                    }
                }
                if !pendingAttachments.isEmpty {
                    PendingAttachmentStrip(items: pendingAttachments) { id in
                        pendingAttachments.removeAll { $0.id == id }
                        attachmentError = nil
                    }
                }
                if let attachmentError {
                    Text(attachmentError)
                        .font(.caption2)
                        .foregroundStyle(DesignTokens.Semantic.red)
                }

                HStack(spacing: 2) {
                    Button {
                        showPhotoPicker = true
                    } label: {
                        AppIcon(AppIcons.editorImage, size: AppIcon.Size.medium)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .frame(width: 32, height: 32)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .disabled(saving || attachmentsFull)
                    .opacity(saving || attachmentsFull ? 0.4 : 1)
                    .accessibilityLabel("Add photo")

                    Button {
                        showFileImporter = true
                    } label: {
                        AppIcon(AppIcons.uiAttach, size: AppIcon.Size.medium)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .frame(width: 32, height: 32)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .disabled(saving || attachmentsFull)
                    .opacity(saving || attachmentsFull ? 0.4 : 1)
                    .accessibilityLabel("Attach a file")

                    Spacer(minLength: 8)

                    Button {
                        saving = true
                        Task {
                            await performSave()
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
                //
                // EXP-554: an attachment-only comment has a blank body — skip
                // the body view entirely rather than rendering an empty block.
                if !bodyText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
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
                    .task(id: bodyText) {
                        let text = bodyText
                        guard displayedBody != text else { return }
                        displayedBody = text
                        let model = IssueEditorModel()
                        // Display-only model: chips render as `#ID <title>` with
                        // the title spliced in as real characters (EXP-307), which
                        // is safe here because this model never serializes — the
                        // edit path builds its own model from the raw stored
                        // markdown. Editable models get the same chip via a
                        // serialization-invisible attachment instead (EXP-322).
                        model.isDisplayOnly = true
                        model.mentionMembers = mentionMembers
                        model.issueRefResolver = resolveIssueRef
                        model.issueRefTitleResolver = resolveIssueRefTitle
                        model.issueRefStatusResolver = resolveIssueRefStatus
                        model.load(markdown: text, baseURL: baseURL)
                        displayModel = model
                    }
                }

                // EXP-554: linked attachments render BELOW the body as squared
                // thumbs + file chips — never inlined into the markdown. Old
                // comments that inlined `![](…)` keep rendering through the body
                // above exactly as before.
                CommentAttachmentsStrip(attachments: attachments)
            }
        }
        // Glass comment card (EXP-240) — the avatar lives in the timeline
        // gutter, not inside the card.
        .padding(.horizontal, 12)
        .padding(.top, 8)
        .padding(.bottom, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .glassCard()
        .photosPicker(
            isPresented: $showPhotoPicker,
            selection: $photoItems,
            maxSelectionCount: AttachmentFiles.maxCommentAttachments,
            matching: .images
        )
        .onChange(of: photoItems) { _, newItems in
            guard !newItems.isEmpty else { return }
            Task { await ingestPhotos(newItems) }
        }
        .fileImporter(
            isPresented: $showFileImporter,
            allowedContentTypes: [.item],
            allowsMultipleSelection: true
        ) { result in
            guard case let .success(urls) = result else { return }
            Task { await ingestFiles(urls) }
        }
        // Entering edit mode seeds the "kept" set from what is linked today; the
        // pending list starts empty on every fresh edit.
        .onChange(of: isEditing) { _, editing in
            if editing { seedEditAttachments() }
        }
        .onAppear {
            if isEditing { seedEditAttachments() }
        }
    }

    // MARK: - Edit-mode attachments (EXP-554)

    private func seedEditAttachments() {
        keptAttachmentIds = Set(attachments.map(\.id))
        pendingAttachments = []
        attachmentError = nil
    }

    /// Upload anything newly picked, then hand the FULL desired id list to the
    /// save closure. Uploads stamp `uploadedId`, so a retry after a failure only
    /// uploads what is left.
    private func performSave() async {
        attachmentError = nil
        var attachmentIds = keptAttachments.map(\.id)
        if !pendingAttachments.isEmpty {
            let outcome = await CommentAttachmentUploads.uploadAll(
                pendingAttachments,
                accountId: accountId,
                issueId: issueId,
                attachmentsApi: deps.attachmentsApi
            )
            pendingAttachments = outcome.items
            if let failure = outcome.failure {
                attachmentError = failure
                return
            }
            attachmentIds += outcome.items.compactMap(\.uploadedId)
        }
        if await onSaveEdit(attachmentIds) {
            pendingAttachments = []
        }
    }

    private func ingestPhotos(_ items: [PhotosPickerItem]) async {
        defer { photoItems = [] }
        attachmentError = nil
        for item in items {
            guard !attachmentsFull else {
                attachmentError = "A comment can carry \(AttachmentFiles.maxCommentAttachments) attachments."
                break
            }
            guard let data = try? await item.loadTransferable(type: Data.self) else { continue }
            let type = item.supportedContentTypes.first
            let outcome = AttachmentPicks.normalizedPhoto(
                data: data,
                contentTypeHint: type?.preferredMIMEType,
                filenameExtensionHint: type?.preferredFilenameExtension
            )
            if let attachment = outcome.attachment {
                pendingAttachments.append(attachment)
            } else {
                attachmentError = outcome.failure
            }
        }
    }

    private func ingestFiles(_ urls: [URL]) async {
        attachmentError = nil
        for url in urls {
            guard !attachmentsFull else {
                attachmentError = "A comment can carry \(AttachmentFiles.maxCommentAttachments) attachments."
                break
            }
            let outcome = await AttachmentPicks.readPickedFile(at: url)
            if let attachment = outcome.attachment {
                pendingAttachments.append(attachment)
            } else {
                attachmentError = outcome.failure
            }
        }
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
