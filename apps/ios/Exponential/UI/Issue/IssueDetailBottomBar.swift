import Combine
import ExpUI
import ExpCore
import PhotosUI
import SwiftUI
import UIKit
import UniformTypeIdentifiers

/// How the bar's right-hand Start-coding circle renders (EXP-240). Computed by
/// IssueDetailView from the view model's steer state so the bar stays dumb.
enum StartCircleUi: Equatable {
    case hidden
    /// A live session on this issue — state dot, tap navigates to the viewer.
    case session(CodingSessionDisplayState, sessionId: String)
    /// Startable: relay on, member, repo-backed board, a desktop online.
    case start
    /// Same gates but no desktop online — dimmed, tap explains.
    case noDevices
    /// A start was sent; waiting for the desktop's session row (30s grace).
    case sending
}

/// The issue-detail floating bottom bar (EXP-240): properties circle +
/// expanding comment pill + start-coding circle, cloning the main
/// MobileTabBar treatment exactly (EXP-698: the OPAQUE card fill on the
/// capsule and circles — they float over the scrolling issue, so a low-alpha
/// tint would let it through — `strokeStrong` hairline, black-35% shadow
/// r16 y6, 5pt inner padding).
/// Tapping the pill expands it into the docked comment composer — a
/// full-width glass card that rides the keyboard (the bar lives in a bottom
/// `safeAreaInset`). Collapse on blur only when the draft is empty (drafts
/// are never lost) and after a successful submit. While another editor owns
/// the keyboard (title / description / comment edit) the collapsed bar hides
/// itself — but stays mounted at zero height so the draft state survives.
struct IssueDetailBottomBar: View {
    let issue: IssueEntity
    let mentionMembers: [MentionMember]
    /// Solo teams hide the composer's @ button (nobody to mention but
    /// yourself, EXP-246) — same gate as the assignee chip.
    let singleMemberTeam: Bool
    let isModerator: Bool
    let startUi: StartCircleUi
    let onOpenProperties: () -> Void
    let onStartCoding: () -> Void
    /// EXP-741: the comment this composer replies to. Setting it expands the
    /// composer in reply mode ("Replying to …" + `parentId` on send); the ✕,
    /// a send and a collapse clear it.
    @Binding var replyTarget: CommentReplyTarget?

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.motion) private var motion

    @State private var composerEditor = IssueEditorModel()
    @State private var expanded = false
    @State private var submitting = false
    @State private var composerHasText = false
    @State private var showPhotoPicker = false
    @State private var photoItems: [PhotosPickerItem] = []
    @State private var showFileImporter = false
    /// EXP-551: the emoji picker sheet resigns the composer's first responder,
    /// so the block to re-focus on dismiss is captured before it opens.
    @State private var showEmojiPicker = false
    @State private var emojiRefocusTarget: UUID?
    /// EXP-554: photos and files picked for THIS comment. They are never inlined
    /// into the markdown body — they upload on send and ride `attachmentIds`.
    @State private var pendingAttachments: [PendingCommentAttachment] = []
    @State private var attachmentError: String?
    @State private var showNoDeviceAlert = false
    // True while ANY keyboard is up (title, description, or a comment-edit
    // editor included — they all install the markdown toolbar as the keyboard
    // accessory). The bar hides then, unless its own composer is expanded.
    @State private var keyboardVisible = false

    /// Android-parity visibility: `composerExpanded || !imeVisible`. Hiding
    /// renders a zero-height placeholder INSTEAD of unmounting so the bar's
    /// @State (the composer draft + pending images) always survives.
    private var barVisible: Bool { expanded || !keyboardVisible }

    var body: some View {
        Group {
            if barVisible {
                Group {
                    if expanded {
                        VStack(spacing: 8) {
                            // EXP-581: the `@`/`#`/`:` menu lives ABOVE the
                            // card — inside it, the height-bounded editor
                            // clipped the menu and it covered the typed line.
                            if composerEditor.showsAutocompleteMenu {
                                EditorAutocompleteMenu(model: composerEditor)
                            }
                            expandedComposer
                        }
                        .padding(.horizontal, 12)
                    } else {
                        collapsedBar
                            .padding(.horizontal, 20)
                    }
                }
                .padding(.top, 8)
                .padding(.bottom, 4)
            } else {
                Color.clear.frame(height: 0)
            }
        }
        .animation(motion.standard, value: expanded)
        .onChange(of: replyTarget) { _, target in
            if target != nil, !expanded { expand() }
        }
        .onReceive(NotificationCenter.default.publisher(for: UIResponder.keyboardWillShowNotification)) { _ in
            keyboardVisible = true
        }
        .onReceive(NotificationCenter.default.publisher(for: UIResponder.keyboardWillHideNotification)) { _ in
            keyboardVisible = false
        }
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
        .sheet(isPresented: $showEmojiPicker, onDismiss: refocusAfterEmojiPicker) {
            EmojiPickerSheet { unicode in
                composerEditor.insertTextAtCaret(unicode)
            }
        }
        // Blur collapses the composer ONLY when nothing would be lost: empty
        // draft, no pending attachments, and no picker mid-flight (presenting a
        // picker resigns first responder).
        .onChange(of: composerEditor.focusedBlockId) { _, focused in
            guard expanded, focused == nil, !submitting else { return }
            guard !showPhotoPicker, photoItems.isEmpty, !showFileImporter, !showEmojiPicker else { return }
            let draft = composerEditor.currentMarkdown().trimmingCharacters(in: .whitespacesAndNewlines)
            guard draft.isEmpty, pendingAttachments.isEmpty, composerEditor.pendingImages.isEmpty else { return }
            collapse()
        }
        .alert("No desktop online", isPresented: $showNoDeviceAlert) {
            Button("OK", role: .cancel) {}
        } message: {
            Text("Open the Exponential desktop app to run here.")
        }
        .onAppear { configureComposer() }
    }

    // MARK: - Collapsed bar

    private var collapsedBar: some View {
        HStack(spacing: 12) {
            if isModerator {
                circleButton(action: onOpenProperties, accessibilityLabel: "Properties") {
                    AppIcon(AppIcons.uiProperties, size: AppIcon.Size.medium, weight: .medium)
                        .foregroundStyle(.white)
                }
                // The sheet it opens carries the SAME "Properties" title, so
                // the styleguide capture addresses the trigger by identifier
                // (EXP-566; Android's twin uses the "Issue properties"
                // contentDescription).
                .accessibilityIdentifier("issue-properties-button")
            }

            Button {
                expand()
            } label: {
                HStack(spacing: 6) {
                    AppIcon(AppIcons.uiAdd, size: AppIcon.Size.medium, weight: .medium)
                    Text("Comment")
                        .font(.subheadline)
                    Spacer(minLength: 0)
                }
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .padding(.horizontal, 14)
                .frame(height: 42)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(5)
                .background(GlassTokens.opaqueCardFill, in: Capsule())
                .overlay(
                    Capsule().stroke(GlassTokens.strokeStrong, lineWidth: GlassTokens.hairline)
                )
                .shadow(color: .black.opacity(0.35), radius: 16, y: 6)
                .contentShape(Capsule())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Comment")

            startCircle
        }
    }

    @ViewBuilder
    private var startCircle: some View {
        switch startUi {
        case .hidden:
            EmptyView()
        case let .session(state, sessionId):
            NavigationLink(value: AppRoute.agentSession(accountId: accountId, sessionId: sessionId)) {
                circleChrome {
                    sessionGlyph(state)
                }
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Coding session")
        case .start:
            circleButton(action: onStartCoding, accessibilityLabel: "Start coding") {
                AppIcon(AppIcons.actionRun, size: AppIcon.Size.medium, weight: .medium)
                    .foregroundStyle(.white)
            }
        case .noDevices:
            circleButton(action: { showNoDeviceAlert = true }, accessibilityLabel: "Start coding") {
                AppIcon(AppIcons.actionRun, size: AppIcon.Size.medium, weight: .medium)
                    .foregroundStyle(.white.opacity(TextOpacity.quaternary))
            }
        case .sending:
            circleChrome {
                ProgressView()
                    .controlSize(.small)
                    .tint(.white)
            }
        }
    }

    /// EXP-698: the circle NAMES the thing it opens — the machine glyph the
    /// Devices tab wears — and the state dot rides it as a badge. A bare dot
    /// in a glass circle said nothing about where the tap went, and read as a
    /// decoration next to the two labelled controls beside it.
    @ViewBuilder
    private func sessionGlyph(_ state: CodingSessionDisplayState) -> some View {
        AppIcon(AppIcons.navDevices, size: AppIcon.Size.medium, weight: .medium)
            .foregroundStyle(.white)
            .overlay(alignment: .topTrailing) {
                sessionDot(state)
                    // Clear of the glyph's own bounds, like a notification
                    // badge — the dot is state, not part of the mark.
                    .offset(x: 6, y: -5)
            }
    }

    @ViewBuilder
    private func sessionDot(_ state: CodingSessionDisplayState) -> some View {
        switch state {
        case .running:
            PulsingLiveDot()
        case .needsInput:
            Circle().fill(DesignTokens.Semantic.yellow).frame(width: 9, height: 9)
        case .review:
            Circle().fill(DesignTokens.Semantic.green).frame(width: 9, height: 9)
        case .done:
            Circle().fill(DesignTokens.Semantic.blue).frame(width: 9, height: 9)
        }
    }

    private func circleButton<Content: View>(
        action: @escaping () -> Void,
        accessibilityLabel: String,
        @ViewBuilder content: () -> Content
    ) -> some View {
        Button(action: action) {
            circleChrome(content: content)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(accessibilityLabel)
    }

    private func circleChrome<Content: View>(@ViewBuilder content: () -> Content) -> some View {
        content()
            .frame(width: 52, height: 52)
            .background(GlassTokens.opaqueCardFill, in: Circle())
            .overlay(
                Circle().stroke(GlassTokens.strokeStrong, lineWidth: GlassTokens.hairline)
            )
            .shadow(color: .black.opacity(0.35), radius: 16, y: 6)
            .contentShape(Circle())
    }

    // MARK: - Expanded composer

    /// EXP-554: an attachment-only comment is legal (the server accepts an empty
    /// body when `attachmentIds` is non-empty), so text is no longer required.
    private var canSend: Bool { composerHasText || !pendingAttachments.isEmpty }

    private var attachmentsFull: Bool {
        pendingAttachments.count >= AttachmentFiles.maxCommentAttachments
    }

    private var expandedComposer: some View {
        // EXP-698: the ONE composer card. Opaque — it floats over the feed.
        GlassComposer(isOpaque: true) {
            // EXP-741: the reply target rides the composer's leading row.
            if let target = replyTarget {
                HStack(spacing: 8) {
                    Text("Replying to \(target.authorName)")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .lineLimit(1)
                    Spacer(minLength: 0)
                    Button {
                        replyTarget = nil
                    } label: {
                        AppIcon(AppIcons.uiClose, size: 14)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .frame(width: 28, height: 28)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Stop replying")
                }
                .padding(.leading, 12)
                .padding(.trailing, 4)
                .padding(.top, 6)
            }
        } field: {
            MarkdownEditor(
                model: composerEditor,
                placeholder: replyTarget == nil ? "Write a comment…" : "Leave a reply…",
                baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
                accountId: accountId,
                httpClient: deps.httpClient,
                mentionMembers: mentionMembers,
                onIssueRefTap: { issueId in deps.deepLinkBus.navigateToIssue(issueId) },
                // The composer keeps only its own photo/@/# row — no
                // formatting strip (EXP-246).
                showsFormattingToolbar: false,
                imageMaxHeight: 120
            )
            .boundedEditorHeight(minHeight: 44, maxHeight: 140)
        } strip: {
            // EXP-554: queued attachments live INSIDE the card, between the
            // editor and the action row — never inlined into the body
            // markdown.
            if !pendingAttachments.isEmpty {
                PendingAttachmentStrip(items: pendingAttachments) { id in
                    pendingAttachments.removeAll { $0.id == id }
                    attachmentError = nil
                }
                .padding(.horizontal, 8)
                .padding(.bottom, 4)
            }

            if let attachmentError {
                Text(attachmentError)
                    .font(.caption2)
                    .foregroundStyle(DesignTokens.Semantic.red)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 12)
                    .padding(.bottom, 4)
            }
        } tools: {
            GlassComposerToolButton(
                AppIcons.editorImage,
                accessibilityLabel: "Add photo",
                enabled: !attachmentsFull
            ) {
                showPhotoPicker = true
            }

            GlassComposerToolButton(
                AppIcons.uiAttach,
                accessibilityLabel: "Attach a file",
                enabled: !attachmentsFull
            ) {
                showFileImporter = true
            }

            if !singleMemberTeam {
                GlassComposerToolButton(
                    AppIcons.editorMention,
                    accessibilityLabel: "Mention a member"
                ) {
                    composerEditor.insertTextAtCaret("@")
                }
            }

            GlassComposerToolButton(
                AppIcons.editorIssueRef,
                accessibilityLabel: "Reference an issue"
            ) {
                composerEditor.insertTextAtCaret("#")
            }

            // EXP-551 — same picker sheet as the formatting toolbar's
            // emoji button; inserts unicode at the composer's caret.
            GlassComposerToolButton(
                AppIcons.editorEmoji,
                accessibilityLabel: "Insert emoji"
            ) {
                emojiRefocusTarget = composerEditor.insertionTargetBlockId
                showEmojiPicker = true
            }
        } submit: {
            GlassComposerSubmitButton(
                AppIcons.uiSubmit,
                accessibilityLabel: "Send comment",
                enabled: !submitting && canSend
            ) {
                Task { await submit() }
            }
        }
    }

    // MARK: - Expand / collapse

    /// Hand first responder back to the composer after the emoji sheet closes
    /// (EXP-551) — otherwise the keyboard stays down and the blur-collapse
    /// guard above would have to keep the composer open forever.
    private func refocusAfterEmojiPicker() {
        guard let target = emojiRefocusTarget else { return }
        emojiRefocusTarget = nil
        DispatchQueue.main.async {
            composerEditor.setFocused(target)
        }
    }

    private func expand() {
        withAnimation(motion.standard) { expanded = true }
        // Programmatic focus needs the text view mounted — one runloop hop,
        // with a 150ms retry in case the first lands before layout.
        DispatchQueue.main.async {
            composerEditor.setFocused(composerEditor.blocks.first?.id)
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) {
            if expanded, composerEditor.focusedBlockId == nil {
                composerEditor.setFocused(composerEditor.blocks.first?.id)
            }
        }
    }

    private func collapse() {
        withAnimation(motion.standard) { expanded = false }
        // A folded composer replies to nothing (EXP-741).
        replyTarget = nil
    }

    // MARK: - Composer plumbing (ported from the old CommentThreadView composer)

    private func configureComposer() {
        composerEditor.onEdit = {
            composerHasText = !composerEditor.currentMarkdown()
                .trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        }
        composerEditor.issueRefResolver = { resolveIssueRef($0) }
        composerEditor.issueRefTitleResolver = { resolveIssueRefTitle($0) }
        composerEditor.issueRefStatusResolver = { resolveIssueRefStatus($0) }
        composerEditor.issueRefSearch = { searchIssueRefs($0) }
    }

    private func resetComposer() {
        composerEditor = IssueEditorModel()
        composerHasText = false
        pendingAttachments = []
        attachmentError = nil
        configureComposer()
    }

    /// EXP-554 — upload on send, then link. Pending items stamp their
    /// `uploadedId` as they land, so a failure half-way keeps the strip and a
    /// retry only uploads what is left. The comment is only cleared once
    /// `comments.create` succeeds.
    private func submit() async {
        guard !submitting else { return }
        submitting = true
        defer { submitting = false }
        let md = composerEditor.currentMarkdown().trimmingCharacters(in: .whitespacesAndNewlines)
        guard !md.isEmpty || !pendingAttachments.isEmpty else { return }
        attachmentError = nil

        var attachmentIds: [String] = []
        if !pendingAttachments.isEmpty {
            let outcome = await CommentAttachmentUploads.uploadAll(
                pendingAttachments,
                accountId: accountId,
                issueId: issue.id,
                attachmentsApi: deps.attachmentsApi
            )
            pendingAttachments = outcome.items
            if let failure = outcome.failure {
                attachmentError = failure
                return
            }
            attachmentIds = outcome.items.compactMap(\.uploadedId)
        }

        do {
            try await deps.commentsApi.create(
                accountId: accountId,
                issueId: issue.id,
                text: md,
                attachmentIds: attachmentIds,
                parentId: replyTarget?.parentId
            )
            resetComposer()
            collapse()
        } catch {
            attachmentError = error.userFacingMessage
        }
    }

    private func resolveIssueRef(_ identifier: String) -> String? {
        IssueRefChipCache.chip(identifier, scope: .issue(id: issue.id), db: deps.db, accountId: accountId)?
            .issueId
    }

    private func resolveIssueRefTitle(_ identifier: String) -> String? {
        IssueRefChipCache.chip(identifier, scope: .issue(id: issue.id), db: deps.db, accountId: accountId)?
            .title
    }

    /// The status glyph a resolved chip paints over its `#` (EXP-423).
    private func resolveIssueRefStatus(_ identifier: String) -> IssueRefStatusInfo? {
        IssueRefChipCache.statusInfo(
            identifier, scope: .issue(id: issue.id), db: deps.db, accountId: accountId)
    }

    private func searchIssueRefs(_ query: String) -> [IssueRefCandidate] {
        IssueRefLookup.search(query, scope: .issue(id: issue.id), db: deps.db, accountId: accountId)
    }

    /// EXP-554: a photo pick becomes a PENDING attachment, not an inline
    /// `![](…)` in the body. Normalization (HEIC→JPEG) and the 10 MB cap are the
    /// shared ones the steer composer uses.
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
