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
/// MobileTabBar treatment exactly (ultraThinMaterial capsule/circles,
/// white-12% 0.5pt stroke, black-35% shadow r16 y6, 5pt inner padding).
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
                        expandedComposer
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
                .background(.ultraThinMaterial, in: Capsule())
                .overlay(
                    Capsule().stroke(Color.white.opacity(0.12), lineWidth: 0.5)
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
                    sessionDot(state)
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
        case .merged:
            // The PR merged but the session lives on (EXP-358) — same blue as
            // the legacy PR-derived done state.
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
            .background(.ultraThinMaterial, in: Circle())
            .overlay(
                Circle().stroke(Color.white.opacity(0.12), lineWidth: 0.5)
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
        VStack(spacing: 0) {
            MarkdownEditor(
                model: composerEditor,
                placeholder: "Write a comment…",
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

            // EXP-554: queued attachments live INSIDE the card, above the action
            // row — never inlined into the body markdown.
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

            HStack(spacing: 2) {
                Button {
                    showPhotoPicker = true
                } label: {
                    AppIcon(AppIcons.editorImage, size: AppIcon.Size.medium)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .frame(width: 36, height: 36)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .disabled(attachmentsFull)
                .opacity(attachmentsFull ? 0.4 : 1)
                .accessibilityLabel("Add photo")

                Button {
                    showFileImporter = true
                } label: {
                    AppIcon(AppIcons.uiAttach, size: AppIcon.Size.medium)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .frame(width: 36, height: 36)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .disabled(attachmentsFull)
                .opacity(attachmentsFull ? 0.4 : 1)
                .accessibilityLabel("Attach a file")

                if !singleMemberTeam {
                    Button {
                        composerEditor.insertTextAtCaret("@")
                    } label: {
                        AppIcon(AppIcons.editorMention, size: AppIcon.Size.medium)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .frame(width: 36, height: 36)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Mention a member")
                }

                Button {
                    composerEditor.insertTextAtCaret("#")
                } label: {
                    AppIcon(AppIcons.editorIssueRef, size: AppIcon.Size.medium)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .frame(width: 36, height: 36)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Reference an issue")

                // EXP-551 — same picker sheet as the formatting toolbar's
                // emoji button; inserts unicode at the composer's caret.
                Button {
                    emojiRefocusTarget = composerEditor.insertionTargetBlockId
                    showEmojiPicker = true
                } label: {
                    AppIcon(AppIcons.editorEmoji, size: AppIcon.Size.medium)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .frame(width: 36, height: 36)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Insert emoji")

                Spacer(minLength: 0)

                Button {
                    Task { await submit() }
                } label: {
                    AppIcon(AppIcons.uiSend, size: 28)
                        .foregroundStyle(
                            submitting || !canSend
                                ? Color.white.opacity(0.3)
                                : Accent.indigo
                        )
                }
                .buttonStyle(.plain)
                .disabled(submitting || !canSend)
                .accessibilityLabel("Send comment")
            }
            .padding(.horizontal, 8)
            .padding(.bottom, 6)
        }
        .background(.ultraThinMaterial)
        .clipShape(RoundedRectangle(cornerRadius: 20))
        .overlay(
            RoundedRectangle(cornerRadius: 20).stroke(Color.white.opacity(0.12), lineWidth: 0.5)
        )
        .shadow(color: .black.opacity(0.35), radius: 16, y: 6)
    }

    // MARK: - Expand / collapse

    /// Hand first responder back to the composer after the emoji sheet closes
    /// (EXP-551) — otherwise the keyboard stays down and the blur-collapse
    /// guard above would have to keep the composer open forever.
    private func refocusAfterEmojiPicker() {
        // A tone change in the sheet has to reach the composer's typeahead too.
        composerEditor.emojiSkinTone = EmojiPreferences().skinTone
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
                issueImagesApi: deps.issueImagesApi,
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
                attachmentIds: attachmentIds
            )
            resetComposer()
            collapse()
        } catch {
            attachmentError = error.localizedDescription
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
