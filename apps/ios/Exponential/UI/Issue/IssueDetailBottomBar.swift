import Combine
import ExpUI
import ExpCore
import PhotosUI
import SwiftUI
import UIKit

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
    let isModerator: Bool
    let startUi: StartCircleUi
    let onOpenProperties: () -> Void
    let onStartCoding: () -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    @State private var composerEditor = IssueEditorModel()
    @State private var expanded = false
    @State private var submitting = false
    @State private var composerHasText = false
    @State private var showPhotoPicker = false
    @State private var photoItem: PhotosPickerItem?
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
        .animation(reduceMotion ? nil : .snappy(duration: 0.18), value: expanded)
        .onReceive(NotificationCenter.default.publisher(for: UIResponder.keyboardWillShowNotification)) { _ in
            keyboardVisible = true
        }
        .onReceive(NotificationCenter.default.publisher(for: UIResponder.keyboardWillHideNotification)) { _ in
            keyboardVisible = false
        }
        .photosPicker(isPresented: $showPhotoPicker, selection: $photoItem, matching: .images)
        .onChange(of: photoItem) { _, newItem in
            guard let newItem else { return }
            Task { await ingestPhoto(newItem) }
        }
        // Blur collapses the composer ONLY when nothing would be lost: empty
        // draft, no pending images, and no picker mid-flight (presenting the
        // photo picker resigns first responder).
        .onChange(of: composerEditor.focusedBlockId) { _, focused in
            guard expanded, focused == nil, !submitting else { return }
            guard !showPhotoPicker, photoItem == nil else { return }
            let draft = composerEditor.currentMarkdown().trimmingCharacters(in: .whitespacesAndNewlines)
            guard draft.isEmpty, composerEditor.pendingImages.isEmpty else { return }
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
                    Image(systemName: "slider.horizontal.3")
                        .font(.body.weight(.medium))
                        .foregroundStyle(.white)
                }
            }

            Button {
                expand()
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: "plus")
                        .font(.body.weight(.medium))
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
                Image(systemName: "play.fill")
                    .font(.body.weight(.medium))
                    .foregroundStyle(.white)
            }
        case .noDevices:
            circleButton(action: { showNoDeviceAlert = true }, accessibilityLabel: "Start coding") {
                Image(systemName: "play.fill")
                    .font(.body.weight(.medium))
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

    private var expandedComposer: some View {
        VStack(spacing: 0) {
            MarkdownEditor(
                model: composerEditor,
                placeholder: "Write a comment…",
                baseURL: deps.auth.instanceBaseURL(forAccountId: accountId),
                accountId: accountId,
                httpClient: deps.httpClient,
                mentionMembers: mentionMembers,
                onIssueRefTap: { issueId in deps.deepLinkBus.navigateToIssue(issueId) }
            )
            .frame(minHeight: 44, maxHeight: 140)

            HStack(spacing: 2) {
                Button {
                    showPhotoPicker = true
                } label: {
                    Image(systemName: "photo")
                        .font(.body)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .frame(width: 36, height: 36)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Add photo")

                Button {
                    composerEditor.insertTextAtCaret("@")
                } label: {
                    Image(systemName: "at")
                        .font(.body)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .frame(width: 36, height: 36)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Mention a member")

                Spacer(minLength: 0)

                Button {
                    Task { await submit() }
                } label: {
                    Image(systemName: "arrow.up.circle.fill")
                        .font(.system(size: 28))
                        .foregroundStyle(
                            submitting || !composerHasText
                                ? Color.white.opacity(0.3)
                                : Accent.indigo
                        )
                }
                .buttonStyle(.plain)
                .disabled(submitting || !composerHasText)
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

    private func expand() {
        withAnimation(reduceMotion ? nil : .snappy(duration: 0.18)) { expanded = true }
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
        withAnimation(reduceMotion ? nil : .snappy(duration: 0.18)) { expanded = false }
    }

    // MARK: - Composer plumbing (ported from the old CommentThreadView composer)

    private func configureComposer() {
        composerEditor.onEdit = {
            composerHasText = !composerEditor.currentMarkdown()
                .trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        }
        composerEditor.issueRefResolver = { resolveIssueRef($0) }
        composerEditor.issueRefSearch = { searchIssueRefs($0) }
    }

    private func resetComposer() {
        composerEditor = IssueEditorModel()
        composerHasText = false
        configureComposer()
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
            collapse()
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

    private func resolveIssueRef(_ identifier: String) -> String? {
        IssueRefLookup.resolve(identifier, scope: .issue(id: issue.id), db: deps.db, accountId: accountId)
    }

    private func searchIssueRefs(_ query: String) -> [IssueRefCandidate] {
        IssueRefLookup.search(query, scope: .issue(id: issue.id), db: deps.db, accountId: accountId)
    }

    private func ingestPhoto(_ item: PhotosPickerItem) async {
        defer { photoItem = nil }
        guard let data = try? await item.loadTransferable(type: Data.self) else { return }
        let contentType = item.supportedContentTypes.first?.preferredMIMEType ?? "image/jpeg"
        let ext = item.supportedContentTypes.first?.preferredFilenameExtension ?? "jpg"
        let filename = "image-\(Int(Date().timeIntervalSince1970)).\(ext)"
        let (width, height) = pixelSize(of: data)
        composerEditor.insertImage(
            data: data, filename: filename, contentType: contentType,
            width: width, height: height
        )
    }

    private func pixelSize(of data: Data) -> (Int?, Int?) {
        guard let image = UIImage(data: data) else { return (nil, nil) }
        let w = Int(image.size.width * image.scale)
        let h = Int(image.size.height * image.scale)
        return (w > 0 ? w : nil, h > 0 ? h : nil)
    }
}
