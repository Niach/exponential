import ExpCore
import ExpUI
import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

// EXP-1295: THE session composer box — the reply box EVERYWHERE (the session
// view, a thread's Conversation, a team agent's channel). One `GlassComposer`
// holding the markdown field, the pending picture/file strip, the footer
// (`SessionComposerFooter`: the `+`, the composer's ONLY tool, and the usage
// ring) and the round send glyph; a send is ONE `sessions.send({sessionId,
// text, attachmentIds})` after every pick uploaded to the session once.
// S-3 (composer-menu.json, surface session | agent): with `controls` the `+`
// opens THE "+" sheet (`ThreadComposerPlusMenu`): Photo or file (THE Photo
// | File sub-choice), Issue › (a pick inserts `#IDENT` at the caret),
// Model › (`/model <alias>`). A bare box's `+` is Photo | File.

/// The draft one composer edits: the session view's model (it outlives the
/// screen, EXP-621) or a thread's own (`ThreadComposerDraft`).
@MainActor
protocol SessionComposerDraft: AnyObject {
    var draftEditor: IssueEditorModel { get }
    var pendingImages: [PendingSteerImage] { get set }
    var pendingFiles: [PendingSteerFile] { get set }
    /// A pick refused for its size or type — inline under the field (FEED-79).
    var steerImageError: String? { get set }
    /// A send (uploads included) is out: the composer dims.
    var steerSending: Bool { get set }
    /// Upload one pick to the session (`/api/sessions/{id}/files`).
    func uploadAttachment(data: Data, filename: String, contentType: String) async throws -> UploadedAttachment
    /// Send one message; true once it is out (the caller clears the draft
    /// only then). A refusal surfaces through the owner's own toast.
    func sendMessage(_ text: String, attachmentIds: [String]?) async -> Bool
}

extension SessionComposerDraft {
    /// The draft as text — what a message sends (`plainText`, never the
    /// markdown serializer: a message goes out exactly as typed).
    var draftText: String {
        get { draftEditor.plainText }
        set { draftEditor.setPlainText(newValue) }
    }

    var trimmedDraft: String {
        draftText.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    var hasPendingAttachments: Bool { !pendingImages.isEmpty || !pendingFiles.isEmpty }

    var attachFull: Bool {
        pendingImages.count >= SteerImageMessage.maxImages
            && pendingFiles.count >= SteerImageMessage.maxFiles
    }

    var hasContent: Bool { !trimmedDraft.isEmpty || hasPendingAttachments }

    /// Send what is in the box. Text alone clears ONLY once the message is
    /// out (EXP-621) and only if nobody typed on meanwhile; with picks, every
    /// one uploads first (a failure keeps the strip with the ids already
    /// stamped, so a retry uploads only the rest), then ONE message carries
    /// the text plus a markdown embed per attachment (EXP-511, wave D).
    func sendDraft() async {
        guard !steerSending else { return }
        guard hasPendingAttachments else {
            guard !trimmedDraft.isEmpty else { return }
            let text = draftText
            if await sendMessage(text, attachmentIds: nil), draftText == text {
                draftText = ""
            }
            return
        }
        steerSending = true
        steerImageError = nil
        defer { steerSending = false }
        var images = pendingImages
        var files = pendingFiles
        do {
            for index in images.indices where images[index].uploadedId == nil {
                images[index].uploadedId = try await uploadAttachment(
                    data: images[index].data,
                    filename: images[index].filename,
                    contentType: images[index].contentType
                ).id
            }
            for index in files.indices where files[index].uploadedId == nil {
                files[index].stamp(try await uploadAttachment(
                    data: files[index].data,
                    filename: files[index].filename,
                    contentType: files[index].contentType
                ))
            }
        } catch {
            steerImageError = error.userFacingMessage
            pendingImages = images
            pendingFiles = files
            return
        }
        let imageIds = images.compactMap(\.uploadedId)
        let sent = await sendMessage(
            SteerImageMessage.buildMessage(
                text: draftText, imageIds: imageIds, files: files.compactMap(\.wireFile)
            ),
            attachmentIds: imageIds + files.compactMap(\.uploadedId)
        )
        guard sent else {
            pendingImages = images
            pendingFiles = files
            return
        }
        draftText = ""
        pendingImages = []
        pendingFiles = []
    }

    /// Wave D: Files-app picks, ANY type — an inline image queues as an image
    /// (marker and all), the rest as file tiles capped at four.
    func ingestFiles(_ urls: [URL]) async {
        steerImageError = nil
        for url in urls {
            let outcome = await AttachmentPicks.readPickedSteerFile(at: url)
            guard let picked = outcome.attachment else {
                steerImageError = outcome.failure
                continue
            }
            if AttachmentFiles.isInlineImage(contentType: picked.contentType) {
                guard pendingImages.count < SteerImageMessage.maxImages else { continue }
                queuePendingImage(picked)
            } else {
                guard pendingFiles.count < SteerImageMessage.maxFiles else {
                    steerImageError = SteerImageMessage.filesFullMessage
                    continue
                }
                pendingFiles.append(PendingSteerFile(
                    data: picked.data, filename: picked.filename, contentType: picked.contentType
                ))
            }
        }
    }

    /// Photo picks → pending images: anything the server's inline-image
    /// pipeline does not accept (HEIC) transcodes to JPEG through the ONE
    /// normalizer every composer shares (EXP-554), capped at four.
    func ingestPhotos(_ items: [PhotosPickerItem]) async {
        steerImageError = nil
        for item in items {
            guard pendingImages.count < SteerImageMessage.maxImages else { break }
            guard let data = try? await item.loadTransferable(type: Data.self) else { continue }
            let type = item.supportedContentTypes.first
            let outcome = AttachmentPicks.normalizedPhoto(
                data: data,
                contentTypeHint: type?.preferredMIMEType,
                filenameExtensionHint: type?.preferredFilenameExtension
            )
            guard let normalized = outcome.attachment else {
                steerImageError = outcome.failure
                continue
            }
            queuePendingImage(normalized)
        }
    }

    /// EXP-802: a PASTED image joins the strip like a picked one.
    func ingestPastedImage(_ image: UIImage) {
        guard pendingImages.count < SteerImageMessage.maxImages else { return }
        guard let data = image.jpegData(compressionQuality: 0.85) else { return }
        steerImageError = nil
        let outcome = AttachmentPicks.normalizedPhoto(
            data: data, contentTypeHint: "image/jpeg", filenameExtensionHint: "jpg"
        )
        guard let normalized = outcome.attachment else {
            steerImageError = outcome.failure
            return
        }
        queuePendingImage(normalized)
    }

    /// Queue one normalized image and drop its positional `[Image #k]`
    /// marker at the END of the draft (EXP-698), so a sentence can name the
    /// picture it means. Not at the caret: that is an offset into the
    /// DECORATED text, where a resolved `#EXP-1` chip carries a title the
    /// sent text does not.
    func queuePendingImage(_ normalized: PendingCommentAttachment) {
        pendingImages.append(PendingSteerImage(
            data: normalized.data,
            filename: normalized.filename,
            contentType: normalized.contentType,
            uploadedId: nil
        ))
        let inserted = SteerImageMessage.insertImageMarker(
            text: draftText,
            caret: (draftText as NSString).length,
            index: pendingImages.count
        )
        draftText = inserted.text
    }

    /// Drop one pick; an image's removal renumbers the draft's markers, so
    /// `[Image #N]` keeps naming the N-th image that will actually be sent.
    func removePending(id: UUID) {
        if let position = pendingImages.firstIndex(where: { $0.id == id }) {
            pendingImages.remove(at: position)
            draftText = SteerImageMessage.renumberImageMarkers(draftText, removedIndex: position + 1)
        } else {
            pendingFiles.removeAll { $0.id == id }
        }
    }
}

/// The box's attach UI, owned by the HOST so an open picker or menu (each
/// resigns the field's focus) never reads as an idle composer to fold away.
struct SessionComposerAttach: Equatable {
    var photoPicker = false
    var fileImporter = false
    var menu = false
    /// S-3: THE "+" sheet (composer-menu.json) is up.
    var plusMenu = false
    var photoItems: [PhotosPickerItem] = []

    /// A picker, the menu or a photo load is up.
    var busy: Bool { photoPicker || fileImporter || menu || plusMenu || !photoItems.isEmpty }
}

struct SessionComposerCard<Draft: SessionComposerDraft>: View {
    let draft: Draft
    let placeholder: String
    /// The session view's own footer controls; nil = a thread's bare box.
    var controls: SessionComposerFooter.Controls?
    @Binding var attach: SessionComposerAttach
    /// Whether Send may go (the owner's gates: a viewer who may not post, a
    /// send in flight).
    var sendEnabled: Bool
    /// EXP-790: an EMPTY field while the agent works offers Stop in the send
    /// slot (the session view only).
    var showsStop = false
    var canStop = false
    var onStop: () -> Void = {}
    /// The return key; nil = send.
    var onReturn: (() -> Void)?
    /// The send glyph; nil = `draft.sendDraft()`.
    var onSend: (() -> Void)?
    var onIssueRefTap: ((String) -> Void)?
    /// EXP-724: hardware ↑/↓/Escape for the session view's `/` menu; each
    /// answers whether it handled the key.
    var onArrow: ((Int) -> Bool)?
    var onEscape: (() -> Bool)?
    var fieldIdentifier: String?

    /// The "+" row the sheet closed on, run once it has dismissed (a sheet
    /// cannot present while its sibling is still animating away).
    @State private var plusPick: ComposerMenu.RowId?

    var body: some View {
        GlassComposer(isOpaque: true) {
            // EXP-802: a caret-bearing field, so `@` members, `#` issues and
            // `:` emoji complete here exactly as in a comment. Return SENDS
            // (one message, never a document).
            MarkdownComposerField(
                model: draft.draftEditor,
                placeholder: placeholder,
                onReturn: { (onReturn ?? send)() },
                onPasteImage: { image in draft.ingestPastedImage(image) },
                onIssueRefTap: onIssueRefTap,
                // One line plus the field's own inset, growing to about five
                // before it scrolls inside itself.
                minHeight: 34,
                maxHeight: 120
            )
            .onKeyPress(.upArrow) { onArrow?(-1) == true ? .handled : .ignored }
            .onKeyPress(.downArrow) { onArrow?(1) == true ? .handled : .ignored }
            .onKeyPress(.escape) { onEscape?() == true ? .handled : .ignored }
            // The text view carries a 6pt inset of its own, so the card's
            // 12/12/6 band is spelled 6/6/0 here.
            .padding(.horizontal, 6)
            .padding(.top, 6)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(fieldIdentifier ?? "session-composer-field")
        } strip: {
            if draft.hasPendingAttachments {
                PendingAttachmentStrip(
                    items: PendingStripItem.steer(images: draft.pendingImages, files: draft.pendingFiles)
                ) { id in
                    draft.removePending(id: id)
                }
                .padding(.horizontal, 8)
                .padding(.bottom, 4)
            }
            if let imageError = draft.steerImageError {
                Text(imageError)
                    .font(.system(size: CGFloat(FeedbackStates.Errors.inlineSize)))
                    .foregroundStyle(DesignTokens.Semantic.red)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 12)
                    .padding(.bottom, 4)
            }
        } tools: {
            SessionComposerFooter(
                controls: controls,
                attachEnabled: !draft.attachFull && !draft.steerSending,
                onPhoto: { attach.photoPicker = true },
                onFile: { attach.fileImporter = true },
                attachMenuOpen: $attach.menu,
                onPlus: plusAction
            )
        } submit: {
            GlassComposerSubmitButton(
                showsStop ? AppIcons.uiStop : AppIcons.uiSubmit,
                accessibilityLabel: showsStop ? "Stop" : "Send",
                enabled: showsStop ? canStop : sendEnabled && draft.hasContent
            ) {
                if showsStop { onStop() } else { (onSend ?? send)() }
            }
        }
        .photosPicker(
            isPresented: $attach.photoPicker,
            selection: $attach.photoItems,
            maxSelectionCount: SteerImageMessage.maxImages,
            matching: .images
        )
        .onChange(of: attach.photoItems) { _, items in
            guard !items.isEmpty else { return }
            Task {
                await draft.ingestPhotos(items)
                attach.photoItems = []
            }
        }
        .background {
            Color.clear
                .sheet(isPresented: $attach.plusMenu, onDismiss: promotePlusPick) {
                    if let controls { plusMenu(controls) }
                }
        }
        .background {
            Color.clear
                .fileImporter(
                    isPresented: $attach.fileImporter,
                    allowedContentTypes: [.item],
                    allowsMultipleSelection: true
                ) { result in
                    guard case let .success(urls) = result else { return }
                    Task { await draft.ingestFiles(urls) }
                }
        }
    }

    /// THE "+" sheet (composer-menu.json, surface session | agent).
    private func plusMenu(_ controls: SessionComposerFooter.Controls) -> ThreadComposerPlusMenu {
        let editor = draft.draftEditor
        return ThreadComposerPlusMenu(
            conditions: controls.menuConditions,
            surface: controls.surface,
            agent: controls.agent,
            modelValue: controls.modelValue,
            onPickModel: controls.onPickModel,
            teamId: controls.teamId,
            onPickIssue: { issue in
                if let identifier = issue.identifier {
                    editor.insertTextAtCaret("#\(identifier) ")
                }
                attach.plusMenu = false
            },
            prompts: controls.prompts,
            onPickPrompt: { prompt in
                controls.onPickPrompt(prompt)
                attach.plusMenu = false
            },
            addFileEnabled: !draft.attachFull && !draft.steerSending,
            onPick: { row in
                plusPick = row
                attach.plusMenu = false
            }
        )
    }

    /// With controls the `+` opens THE "+" sheet; a bare box's is Photo | File.
    private var plusAction: (() -> Void)? {
        guard controls != nil else { return nil }
        return { openPlusMenu() }
    }

    private func openPlusMenu() {
        UIApplication.endEditing()
        attach.plusMenu = true
    }

    /// The row the "+" closed on opens its picker now the sheet is gone:
    /// Photo or file = THE Photo | File sub-choice.
    private func promotePlusPick() {
        guard let pick = plusPick else { return }
        plusPick = nil
        guard pick == .addFile else { return }
        var transaction = Transaction()
        transaction.disablesAnimations = true
        withTransaction(transaction) { attach.menu = true }
    }

    private func send() {
        guard sendEnabled else { return }
        Task { await draft.sendDraft() }
    }
}
