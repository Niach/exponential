import ExpUI
import ExpCore
import os
import PhotosUI
import SwiftUI
import UIKit
import UniformTypeIdentifiers

private let log = Logger(subsystem: "com.exponential", category: "MarkdownEditor")

// MARK: - Public SwiftUI View

/// Block-based markdown editor. Renders the blocks owned by `IssueEditorModel`
/// and routes every edit back through it; the model is the single source of
/// truth and derives markdown only at save points.
struct MarkdownEditor: View {
    let model: IssueEditorModel
    var placeholder: String = "Add description..."
    var baseURL: URL?
    var accountId: String = ""
    var httpClient: HTTPClient?
    var mentionMembers: [MentionMember] = []
    /// Tap on a rendered `#IDENTIFIER` issue-ref pill (value = resolved issue
    /// id). Pills only render when the host set `model.issueRefResolver`.
    var onIssueRefTap: ((String) -> Void)?
    /// Display-only rendering (comment bodies): text views are non-editable
    /// (link taps open URLs), image blocks lose their delete affordance, and
    /// the editing chrome (toolbar, pickers, autocomplete bars) never mounts.
    var isReadOnly: Bool = false
    /// False suppresses the keyboard-accessory formatting strip entirely —
    /// the bottom-bar comment composer keeps only its own photo/@/# row
    /// (EXP-246).
    var showsFormattingToolbar: Bool = true
    /// Caps embedded image blocks — compact contexts (the comment composer)
    /// would otherwise be dominated by a single image (EXP-246).
    var imageMaxHeight: CGFloat?
    /// Text blocks report their own IDEAL width (capped at the proposal)
    /// instead of filling it — a chat bubble must hug a one-line message
    /// rather than stretch across the feed (EXP-440).
    var hugsContentWidth: Bool = false
    /// EXP-655: issue detail passes 200 (Android's `minHeight = 200.dp`), and
    /// the empty band below the content focuses the end of the description.
    /// The create screen passes 120, Android's `CreateIssueScreen` height
    /// (EXP-698 r4): a band short enough that the auto-focused title's keyboard
    /// still leaves the properties card, Labels and "Create more" on screen —
    /// the 200pt one hid them on an iPhone (EXP-659). Comment composers keep
    /// their own bounded scroller, so they leave this nil and stay hugging.
    var minHeight: CGFloat? = nil
    /// EXP-327: non-nil adds a "Files" entry to the toolbar's image button and
    /// receives the NON-image picks. Images picked there are appended to the
    /// description here instead, so the host never sees them — which is why
    /// attaching a screenshot through the file picker no longer dead-ends in
    /// "images go in the description". Nil keeps the plain image button, for
    /// editors whose host has nowhere to put an attachment.
    var onAttachFile: ((URL) -> Void)?
    // The `@`/`#`/`:` candidate menu is NOT mounted here: every host mounts
    // `EditorAutocompleteMenu(model:)` itself, pinned above the keyboard (a
    // bottom `safeAreaInset`), gated on `model.showsAutocompleteMenu`. An
    // in-editor anchor cannot be made to work — a whole description is ONE
    // text block on iOS, so anchoring to the typed block put the menu under
    // the end of the description, off-screen inside the enclosing scroller
    // and behind the keyboard (EXP-592).

    @State private var photoItem: PhotosPickerItem?
    @State private var showPhotoPicker = false
    @State private var showFileImporter = false
    @State private var toolbar = MarkdownToolbar()
    /// EXP-551 — the toolbar's emoji button opens a sheet, which resigns the
    /// text view's first responder; `emojiRefocusTarget` is the block to hand
    /// focus back to once it dismisses.
    @State private var showEmojiPicker = false
    @State private var emojiRefocusTarget: UUID?
    private let emojiPreferences = EmojiPreferences()

    // NOTE: deliberately no internal ScrollView. Every usage embeds this
    // editor inside an outer ScrollView (issue detail, create sheet, comment
    // composer); a nested vertical ScrollView proposed an unbounded height
    // reports its content's IDEAL size in both axes, so one long unwrappable
    // line (e.g. a code span) blew the whole column out to ~3× screen width
    // and embedded images rendered at native pixel size.
    var body: some View {
        Group {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(model.blocks) { block in
                        switch block {
                        case .text(let id, let content):
                            BlockTextEditor(
                                model: model,
                                blockId: id,
                                content: content,
                                revision: model.revision(for: id),
                                isFocused: model.focusedBlockId == id,
                                placeholder: isSolePlaceholderBlock(id) ? placeholder : nil,
                                toolbar: showsFormattingToolbar ? toolbar : nil,
                                isReadOnly: isReadOnly,
                                hugsContentWidth: hugsContentWidth,
                                onPasteImage: { image in insert(uiImage: image) },
                                onIssueRefTap: onIssueRefTap
                            )
                            .id(id)

                        case .image(let id, let url, let alt):
                            BlockImageView(
                                model: model,
                                blockId: id,
                                url: url,
                                alt: alt,
                                baseURL: baseURL,
                                accountId: accountId,
                                httpClient: httpClient,
                                pendingImages: model.pendingImages,
                                isReadOnly: isReadOnly,
                                maxHeight: imageMaxHeight,
                                onDelete: { model.deleteImageBlock(id: id) },
                                onTapBelow: { focusBlock(after: id) },
                                onRetry: { Task { await model.retryImage(blockId: id) } }
                            )
                            .id(id)

                        case .table(let id, let table):
                            BlockTableView(
                                model: model,
                                blockId: id,
                                table: table,
                                isReadOnly: isReadOnly,
                                onIssueRefTap: onIssueRefTap
                            )
                            .id(id)
                        }
                    }
                }
                .padding(.horizontal, isReadOnly ? 0 : 8)
                .padding(.top, isReadOnly ? 0 : 12)
                // EXP-655: the tap catcher sits IN FRONT of the host's own
                // endEditing background (IssueDetailView puts one on the outer
                // content) and BEHIND the UIKit text views, so only dead space
                // below the last block ever reaches it. Kept conditional so
                // read-only / hugging callers are untouched.
                .frame(
                    maxWidth: minHeight == nil ? nil : .infinity,
                    minHeight: minHeight,
                    alignment: .top
                )
                .background {
                    if minHeight != nil, !isReadOnly {
                        Color.clear
                            .contentShape(Rectangle())
                            .onTapGesture { model.focusEnd() }
                    }
                }
        }
        .sheet(isPresented: $showEmojiPicker, onDismiss: refocusAfterEmojiPicker) {
            EmojiPickerSheet(preferences: emojiPreferences) { unicode in
                model.insertTextAtCaret(unicode)
            }
        }
        .onChange(of: mentionMembers) { _, newValue in model.mentionMembers = newValue }
        .photosPicker(isPresented: $showPhotoPicker, selection: $photoItem, matching: .images)
        .onChange(of: photoItem) { _, newItem in
            guard let newItem else { return }
            Task { await ingestPhoto(newItem) }
        }
        .fileImporter(
            isPresented: $showFileImporter,
            allowedContentTypes: [.item],
            allowsMultipleSelection: true
        ) { result in
            guard case let .success(urls) = result else { return }
            for url in urls { ingestPickedFile(url) }
        }
        .onAppear {
            guard !isReadOnly else { return }
            toolbar.onImagePick = { showPhotoPicker = true }
            if onAttachFile != nil {
                toolbar.onFilePick = { showFileImporter = true }
            } else {
                toolbar.onFilePick = nil
            }
            toolbar.onEmoji = {
                // Captured while the text view is still first responder.
                emojiRefocusTarget = model.insertionTargetBlockId
                showEmojiPicker = true
            }
            model.mentionMembers = mentionMembers
            // EXP-551: decode the bundled dataset off-main once, then wire the
            // `:shortcode` typeahead into the model.
            EmojiCatalog.shared.preload()
            model.emojiSearch = { query in
                EmojiCatalog.shared.search(query, limit: EmojiCatalog.typeaheadLimit)
            }
            model.onEmojiInserted = { record in
                EmojiPreferences().recordRecent(record.unicode)
            }
        }
        // Membership can sync in after mount and flip the attach gate:
        // `onAttachFile` is derived from membership, and on a cold start the
        // editor mounts before the team_members rows land. Without this the
        // "Files" entry would stay missing for the life of the view — and
        // EXP-327 removed the Files section's own paperclip, so there is no
        // other way in.
        .onChange(of: onAttachFile == nil) { _, isNil in
            toolbar.onFilePick = isNil ? nil : { showFileImporter = true }
        }
    }

    /// The picker sheet took first responder; hand it back so the keyboard
    /// returns with the caret where the emoji landed (EXP-551).
    private func refocusAfterEmojiPicker() {
        guard let target = emojiRefocusTarget else { return }
        emojiRefocusTarget = nil
        DispatchQueue.main.async {
            model.setFocused(target)
        }
    }

    private func isSolePlaceholderBlock(_ id: UUID) -> Bool {
        model.blocks.count == 1 && model.blocks.first?.id == id
    }

    private func focusBlock(after id: UUID) {
        guard let idx = model.blocks.firstIndex(where: { $0.id == id }), idx + 1 < model.blocks.count else { return }
        model.setFocused(model.blocks[idx + 1].id)
    }

    // MARK: - Image insertion

    private func ingestPhoto(_ item: PhotosPickerItem) async {
        defer { photoItem = nil }
        guard let data = try? await item.loadTransferable(type: Data.self) else { return }
        let contentType = item.supportedContentTypes.first?.preferredMIMEType ?? "image/jpeg"
        let ext = item.supportedContentTypes.first?.preferredFilenameExtension ?? "jpg"
        let filename = "image-\(Int(Date().timeIntervalSince1970)).\(ext)"
        let (width, height) = pixelSize(of: data)
        model.insertImage(data: data, filename: filename, contentType: contentType, width: width, height: height)
    }

    /// Sort a "Files" pick (EXP-327): an inline-image type is APPENDED to the
    /// description — same draft/upload lifecycle as the photo picker, just at
    /// the end rather than at the caret, because the user was attaching rather
    /// than typing. Everything else goes to the host as a real attachment.
    ///
    /// The read runs off-main inside the security scope: a 50 MB pick from a
    /// cloud-backed provider streams over the network and would freeze the UI.
    private func ingestPickedFile(_ url: URL) {
        let contentType = AttachmentFiles.canonicalContentType(
            UTType(filenameExtension: url.pathExtension)?.preferredMIMEType
        )
        guard AttachmentFiles.isInlineImage(contentType: contentType) else {
            onAttachFile?(url)
            return
        }
        // Size check BEFORE buffering, like the sibling attachment path: the
        // pick is classified from its extension alone, so a 300 MB `.png` would
        // otherwise be read whole into memory only to be rejected on upload —
        // leaving an uncommittable draft that blocks every later description
        // save. Over the cap it goes to the host as an ordinary attachment
        // pick, which re-checks the size and renders a real failure row; the
        // editor has no error surface for a block it never inserted.
        // Cheap metadata read, so it stays inline rather than off-main.
        let scoped = url.startAccessingSecurityScopedResource()
        let declaredSize = (try? url.resourceValues(forKeys: [.fileSizeKey]))?.fileSize
        if scoped { url.stopAccessingSecurityScopedResource() }
        if let declaredSize, declaredSize > AttachmentFiles.maxFileUploadBytes {
            onAttachFile?(url)
            return
        }

        let filename = AttachmentFiles.sanitizedFilename(url.lastPathComponent)
        let editorModel = model
        Task.detached {
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            guard let data = try? Data(contentsOf: url) else { return }
            // Backstop for a provider that reports no file size up front.
            guard data.count <= AttachmentFiles.maxFileUploadBytes else {
                log.error("Picked image exceeds the upload cap: \(data.count, privacy: .public) bytes")
                return
            }
            let decoded = UIImage(data: data)
            let width = decoded.map { Int($0.size.width * $0.scale) }
            let height = decoded.map { Int($0.size.height * $0.scale) }
            await editorModel.appendImage(
                data: data,
                filename: filename,
                contentType: contentType,
                width: (width ?? 0) > 0 ? width : nil,
                height: (height ?? 0) > 0 ? height : nil
            )
        }
    }

    private func insert(uiImage image: UIImage) {
        guard let data = image.jpegData(compressionQuality: 0.85) else { return }
        let scale = image.scale
        let width = Int(image.size.width * scale)
        let height = Int(image.size.height * scale)
        model.insertImage(
            data: data,
            filename: "pasted-\(Int(Date().timeIntervalSince1970)).jpg",
            contentType: "image/jpeg",
            width: width > 0 ? width : nil,
            height: height > 0 ? height : nil
        )
    }

    private func pixelSize(of data: Data) -> (Int?, Int?) {
        guard let image = UIImage(data: data) else { return (nil, nil) }
        let w = Int(image.size.width * image.scale)
        let h = Int(image.size.height * image.scale)
        return (w > 0 ? w : nil, h > 0 ? h : nil)
    }
}

// MARK: - Editor Text View (UITextView subclass)

// Internal, not private: `BlockTableView` builds cell editors from another
// file in this module (EXP-726).
final class EditorTextView: UITextView {
    var onDeleteBackwardAtStart: (() -> Void)?
    var onPasteImage: ((UIImage) -> Void)?
    var onIssueRefTap: ((String) -> Void)?
    /// Display-only rendering: issue-ref taps still navigate, but checkbox
    /// glyph taps must not mutate the (never-persisted) text.
    var isReadOnlyRendering = false
    /// Strong ref to the manually-built TextKit 1 stack's storage. TextKit
    /// ownership is strictly downward (storage → layoutManager → container);
    /// the text view only retains its container, so without this the storage
    /// would deallocate out from under the view.
    var ownedTextStorage: NSTextStorage?

    override init(frame: CGRect, textContainer: NSTextContainer?) {
        super.init(frame: frame, textContainer: textContainer)
        let tap = UITapGestureRecognizer(target: self, action: #selector(handleTap(_:)))
        tap.delegate = self
        addGestureRecognizer(tap)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError() }

    // Detaching from the window (navigation pop, sheet teardown, unmount)
    // must tear the keyboard + accessory strip down with it. Nothing else
    // resigns UIKit first responders — a stale one left the formatting strip
    // floating over unrelated screens (EXP-246).
    // Deferred by one runloop turn on purpose: resigning synchronously fires
    // textViewDidEndEditing -> model.clearFocusIfMatches, which mutates
    // @Observable state in the middle of SwiftUI's hierarchy teardown
    // ("Modifying state during view update"). The keyboard still goes away.
    override func willMove(toWindow newWindow: UIWindow?) {
        super.willMove(toWindow: newWindow)
        if newWindow == nil, isFirstResponder {
            DispatchQueue.main.async { [weak self] in
                self?.resignFirstResponder()
            }
        }
    }

    @objc private func handleTap(_ gesture: UITapGestureRecognizer) {
        // Container coordinates: every layout-manager geometry call is relative
        // to the text container's origin, which the raw view point ignores.
        let raw = gesture.location(in: self)
        let point = CGPoint(x: raw.x - textContainerInset.left, y: raw.y - textContainerInset.top)
        let glyphIndex = layoutManager.glyphIndex(for: point, in: textContainer)
        guard glyphIndex < layoutManager.numberOfGlyphs else { return }
        let charIndex = layoutManager.characterIndexForGlyph(at: glyphIndex)
        guard charIndex < textStorage.length else { return }
        // Issue-ref pill: navigate to the referenced issue (render-only
        // decoration applied by IssueEditorModel.load). `glyphIndex(for:)` is
        // NEAREST-glyph, so a tap in the empty space right of a line ending in
        // a chip resolved to the chip and navigated instead of placing the
        // caret (EXP-655) — only navigate when the tap really lands on the
        // pill. Returning instead lets UITextView's own recognizer (they run
        // simultaneously) place the caret.
        var refRange = NSRange(location: 0, length: 0)
        if let issueId = textStorage.attribute(
            .markdownIssueRef,
            at: charIndex,
            longestEffectiveRange: &refRange,
            in: NSRange(location: 0, length: textStorage.length)
        ) as? String {
            guard hitTestGlyphs(point, charRange: refRange, glyphIndex: glyphIndex, slack: 2) else {
                return
            }
            onIssueRefTap?(issueId)
            return
        }
        guard !isReadOnlyRendering else { return }
        let char = (textStorage.string as NSString).substring(with: NSRange(location: charIndex, length: 1))
        if char == "\u{2610}" || char == "\u{2611}" {
            let box = NSRange(location: charIndex, length: 1)
            guard hitTestGlyphs(point, charRange: box, glyphIndex: glyphIndex, slack: 4) else { return }
            let replacement = char == "\u{2610}" ? "\u{2611}" : "\u{2610}"
            let attrs = textStorage.attributes(at: charIndex, effectiveRange: nil)
            textStorage.replaceCharacters(in: box, with: NSAttributedString(string: replacement, attributes: attrs))
            delegate?.textViewDidChange?(self)
        }
    }

    /// True when `point` (container coordinates) lies within `slack` of the
    /// glyphs `charRange` paints on the line fragment that holds `glyphIndex` —
    /// a wrapped run must not be hit-tested against its other lines' art.
    private func hitTestGlyphs(
        _ point: CGPoint,
        charRange: NSRange,
        glyphIndex: Int,
        slack: CGFloat
    ) -> Bool {
        var lineGlyphs = NSRange(location: 0, length: 0)
        _ = layoutManager.lineFragmentRect(forGlyphAt: glyphIndex, effectiveRange: &lineGlyphs)
        let glyphs = layoutManager.glyphRange(forCharacterRange: charRange, actualCharacterRange: nil)
        let onThisLine = NSIntersectionRange(glyphs, lineGlyphs)
        guard onThisLine.length > 0 else { return false }
        let rect = layoutManager.boundingRect(forGlyphRange: onThisLine, in: textContainer)
        return rect.insetBy(dx: -slack, dy: -slack).contains(point)
    }

    override func deleteBackward() {
        if selectedRange.location == 0, selectedRange.length == 0 {
            onDeleteBackwardAtStart?()
            return
        }
        super.deleteBackward()
    }

    override func paste(_ sender: Any?) {
        let pb = UIPasteboard.general
        if pb.hasImages, let image = pb.image {
            onPasteImage?(image)
            return
        }
        super.paste(sender)
    }

    override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
        if action == #selector(paste(_:)) && UIPasteboard.general.hasImages { return true }
        return super.canPerformAction(action, withSender: sender)
    }
}

extension EditorTextView: UIGestureRecognizerDelegate {
    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool {
        true
    }
}

// MARK: - Block Text Editor (UIViewRepresentable)

struct BlockTextEditor: UIViewRepresentable {
    let model: IssueEditorModel
    let blockId: UUID
    let content: NSAttributedString
    let revision: Int
    let isFocused: Bool
    let placeholder: String?
    /// nil = no formatting accessory strip (the comment composer, EXP-246).
    let toolbar: MarkdownToolbar?
    var isReadOnly = false
    /// See `MarkdownEditor.hugsContentWidth`.
    var hugsContentWidth = false
    /// EXP-726 — table-cell mode: a cell is ONE inline paragraph, so Return
    /// leaves the cell instead of splitting it, pasted newlines collapse to
    /// spaces, the return key reads "next", and the representable measures
    /// itself against its own width bounds when the proposal is unspecified
    /// (a horizontally scrolling table row proposes none).
    var singleLine = false
    /// Column alignment from the table's delimiter row.
    var textAlignment: NSTextAlignment = .natural
    /// Return in `singleLine` mode. Nil swallows the newline.
    var onReturn: (() -> Void)?
    /// EXP-727 — set only on an editable table cell: adds "Delete table" to
    /// the cell's edit menu, the one a long-press already opens.
    var onDeleteTable: (() -> Void)?
    var onPasteImage: (UIImage) -> Void = { _ in }
    var onIssueRefTap: ((String) -> Void)?

    func makeUIView(context: Context) -> EditorTextView {
        // Explicit TextKit 1 stack: installs MarkdownLayoutManager (quote bar
        // + one-box code fences) and makes the TextKit version deterministic
        // — handleTap's `layoutManager` access was already forcing the lazy
        // TextKit 1 fallback (EXP-246).
        let storage = NSTextStorage()
        let layoutManager = MarkdownLayoutManager()
        // `CGFloat.` spelled out: a bare `.greatestFiniteMagnitude` next to the
        // untyped `0` leaves the CGSize overload unpinned and the literal is
        // ambiguous between CGFloat and Double, which fails to compile.
        let container = NSTextContainer(
            size: CGSize(width: 0, height: CGFloat.greatestFiniteMagnitude)
        )
        container.widthTracksTextView = true
        layoutManager.addTextContainer(container)
        storage.addLayoutManager(layoutManager)
        let tv = EditorTextView(frame: .zero, textContainer: container)
        tv.ownedTextStorage = storage
        tv.backgroundColor = .clear
        tv.textColor = MarkdownStyle.textColor
        tv.tintColor = MarkdownStyle.linkColor
        tv.font = MarkdownStyle.bodyFont
        // Read-only display (comment bodies): non-editable but selectable —
        // UITextView then opens .link attributes natively on tap.
        tv.isEditable = !isReadOnly
        tv.isReadOnlyRendering = isReadOnly
        tv.isScrollEnabled = false
        tv.alwaysBounceVertical = false
        tv.textContainerInset = UIEdgeInsets(top: 4, left: 0, bottom: 4, right: 0)
        tv.keyboardAppearance = .dark // app chrome is forced-dark
        tv.autocorrectionType = .default
        tv.autocapitalizationType = .sentences
        tv.typingAttributes = MarkdownStyle.baseAttributes
        if singleLine {
            // The cell is one line of prose, so the keyboard offers "next" and
            // autocapitalisation stays sentence-shaped like every other block.
            tv.returnKeyType = .next
            tv.textContainerInset = UIEdgeInsets(top: 6, left: 6, bottom: 6, right: 6)
        }
        if !isReadOnly, let toolbar {
            tv.inputAccessoryView = toolbar
        }
        tv.delegate = context.coordinator

        let coord = context.coordinator
        coord.textView = tv
        coord.model = model
        coord.blockId = blockId
        coord.onPasteImage = onPasteImage
        coord.singleLine = singleLine
        coord.onReturn = onReturn
        coord.onDeleteTable = onDeleteTable
        coord.appliedRevision = revision

        tv.onDeleteBackwardAtStart = { [weak coord] in coord?.handleDeleteBackwardAtStart() }
        tv.onPasteImage = { [weak coord] image in coord?.onPasteImage?(image) }
        tv.onIssueRefTap = onIssueRefTap

        coord.beginProgrammaticChange()
        tv.attributedText = content
        if textAlignment != .natural { tv.textAlignment = textAlignment }
        coord.endProgrammaticChange()
        if content.length == 0, let placeholder {
            coord.showPlaceholder(in: tv, text: placeholder)
        }
        return tv
    }

    // Without this, SwiftUI sizes the representable from UITextView's
    // intrinsicContentSize, whose width is the longest paragraph UNWRAPPED —
    // one long code span (e.g. a user-agent string) then widens the whole
    // block column far beyond the screen. Adopt the proposed width and report
    // the wrapped text height for it instead.
    //
    // `hugsContentWidth` reports the unwrapped ideal width instead — but still
    // CAPPED at the proposal, which is what keeps the runaway-width hazard
    // above fixed: past the cap the text wraps, so the height has to be
    // measured at the capped width either way.
    func sizeThatFits(
        _ proposal: ProposedViewSize,
        uiView tv: EditorTextView,
        context: Context
    ) -> CGSize? {
        guard let width = proposal.width, width.isFinite, width > 0 else {
            // A table row scrolls horizontally, so SwiftUI proposes no width to
            // its cells (EXP-726). Hug the content between the cell bounds and
            // measure the height AT that width, or the text wraps into a height
            // nobody reserved.
            guard singleLine else { return nil }
            let ideal = tv.sizeThatFits(
                CGSize(
                    width: CGFloat.greatestFiniteMagnitude,
                    height: CGFloat.greatestFiniteMagnitude
                )
            )
            let clamped = min(
                max(ideal.width.isFinite ? ceil(ideal.width) : MarkdownStyle.tableCellMinWidth,
                    MarkdownStyle.tableCellMinWidth),
                MarkdownStyle.tableCellMaxWidth
            )
            let fitted = tv.sizeThatFits(
                CGSize(width: clamped, height: .greatestFiniteMagnitude)
            )
            return CGSize(width: clamped, height: fitted.height)
        }
        var targetWidth = width
        if hugsContentWidth {
            let ideal = tv.sizeThatFits(
                CGSize(
                    width: CGFloat.greatestFiniteMagnitude,
                    height: CGFloat.greatestFiniteMagnitude
                )
            )
            // An empty block (the separators normalize() puts around images)
            // measures 0 wide — keep the proposal there rather than collapsing
            // the whole column to nothing.
            if ideal.width > 0, ideal.width.isFinite {
                targetWidth = min(width, ceil(ideal.width))
            }
        }
        let fitted = tv.sizeThatFits(
            CGSize(width: targetWidth, height: .greatestFiniteMagnitude)
        )
        return CGSize(width: targetWidth, height: fitted.height)
    }

    func updateUIView(_ tv: EditorTextView, context: Context) {
        let coord = context.coordinator
        coord.model = model
        coord.blockId = blockId
        coord.onPasteImage = onPasteImage
        coord.singleLine = singleLine
        coord.onReturn = onReturn
        coord.onDeleteTable = onDeleteTable
        tv.onDeleteBackwardAtStart = { [weak coord] in coord?.handleDeleteBackwardAtStart() }
        tv.onPasteImage = { [weak coord] image in coord?.onPasteImage?(image) }
        tv.onIssueRefTap = onIssueRefTap

        // Apply EXTERNAL content changes only (structural edits / remote apply),
        // identified by a bumped revision. The user's own keystrokes never bump
        // the revision, so we never clobber what they just typed.
        if revision != coord.appliedRevision {
            coord.appliedRevision = revision
            let savedRange = tv.selectedRange
            coord.beginProgrammaticChange()
            tv.attributedText = content
            if textAlignment != .natural { tv.textAlignment = textAlignment }
            coord.endProgrammaticChange()
            let pos = min(savedRange.location, tv.textStorage.length)
            tv.selectedRange = NSRange(location: pos, length: 0)
        }

        // Caret requested by a structural mutation (merge/split), applied inline
        // — no DispatchQueue hop. Consumed once.
        if let desired = model.consumeDesiredSelection(for: blockId) {
            let pos = min(desired, tv.textStorage.length)
            tv.selectedRange = NSRange(location: pos, length: 0)
        }

        if content.length == 0, let placeholder {
            coord.showPlaceholder(in: tv, text: placeholder)
        } else {
            coord.hidePlaceholder()
        }

        if !isReadOnly, isFocused, !tv.isFirstResponder {
            tv.becomeFirstResponder()
            toolbar?.textView = tv
        }
        if !isReadOnly, tv.isFirstResponder {
            toolbar?.textView = tv
        }
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    @MainActor
    final class Coordinator: NSObject, UITextViewDelegate {
        weak var textView: EditorTextView?
        var model: IssueEditorModel?
        var blockId: UUID?
        var onPasteImage: ((UIImage) -> Void)?
        /// EXP-726 — see `BlockTextEditor.singleLine`.
        var singleLine = false
        var onReturn: (() -> Void)?
        /// EXP-727 — see `BlockTextEditor.onDeleteTable`.
        var onDeleteTable: (() -> Void)?
        var appliedRevision = 0

        private var isProgrammaticChange = false
        private var placeholderLabel: UILabel?
        /// Re-entrancy guard: assigning `selectedRange` fires this delegate
        /// method again.
        private var isSnappingCaret = false
        /// Last collapsed caret, which gives the snap its direction of travel
        /// (nil after a ranged selection).
        private var lastCollapsedCaret: Int?

        func beginProgrammaticChange() { isProgrammaticChange = true }
        func endProgrammaticChange() { isProgrammaticChange = false }

        func handleDeleteBackwardAtStart() {
            guard let model, let blockId else { return }
            model.deleteImage(beforeTextBlock: blockId)
        }

        func showPlaceholder(in tv: UITextView, text: String) {
            if placeholderLabel == nil {
                let label = UILabel()
                label.text = text
                label.font = MarkdownStyle.bodyFont
                label.textColor = MarkdownStyle.placeholderColor
                label.translatesAutoresizingMaskIntoConstraints = false
                label.isUserInteractionEnabled = false
                tv.addSubview(label)
                NSLayoutConstraint.activate([
                    label.topAnchor.constraint(equalTo: tv.topAnchor, constant: tv.textContainerInset.top),
                    label.leadingAnchor.constraint(equalTo: tv.leadingAnchor, constant: tv.textContainerInset.left + tv.textContainer.lineFragmentPadding),
                ])
                placeholderLabel = label
            }
            placeholderLabel?.isHidden = false
        }

        func hidePlaceholder() {
            placeholderLabel?.isHidden = true
        }

        // MARK: UITextViewDelegate

        /// EXP-727 — the ONE table manipulation mobile ships. The edit menu is
        /// what a long-press on a cell already opens, so "Delete table" rides
        /// it instead of a second long-press gesture fighting UITextView's own
        /// (which would cost cells their selection and loupe). Nil keeps the
        /// system menu for every other block.
        func textView(
            _ textView: UITextView,
            editMenuForTextIn range: NSRange,
            suggestedActions: [UIMenuElement]
        ) -> UIMenu? {
            guard let onDeleteTable else { return nil }
            let delete = UIAction(
                title: "Delete table",
                image: AppIcons.uiImage(AppIcons.uiDelete, pointSize: 16),
                attributes: .destructive
            ) { _ in onDeleteTable() }
            return UIMenu(children: suggestedActions + [delete])
        }

        func textViewDidBeginEditing(_ tv: UITextView) {
            guard let blockId else { return }
            model?.setFocused(blockId)
            // SwiftUI's keyboard avoidance only shrinks the safe area — it
            // never scrolls a UIKit first responder into view, so a focused
            // block near the bottom (the comment composer) stayed half-hidden
            // behind the keyboard (EXP-135). Reveal the caret once the
            // keyboard animation and the avoidance insets have settled.
            Task { [weak self, weak tv] in
                try? await Task.sleep(nanoseconds: 400_000_000)
                guard let self, let tv, tv.isFirstResponder else { return }
                self.scrollCaretIntoView(tv)
            }
        }

        func textViewDidEndEditing(_ tv: UITextView) {
            // The rail must never come back mid-mode (EXP-568); it stays in
            // link mode on purpose while its own URL field holds the responder.
            (tv.inputAccessoryView as? MarkdownToolbar)?.handleHostEndedEditing()
            guard let blockId else { return }
            model?.clearFocusIfMatches(blockId)
        }

        func textViewDidChange(_ tv: UITextView) {
            guard !isProgrammaticChange else { return }
            placeholderLabel?.isHidden = tv.textStorage.length != 0
            guard let model, let blockId else { return }
            let snapshot = NSAttributedString(attributedString: tv.attributedText)
            model.updateText(id: blockId, content: snapshot)
            // Keep the caret visible as typing grows the block (a no-op when
            // it already is — scrollRectToVisible ignores visible rects).
            if tv.isFirstResponder {
                scrollCaretIntoView(tv)
            }
            scheduleChipPass()
        }

        // MARK: Chip decoration (EXP-322)

        private var chipPassScheduled = false

        /// Re-chip after the edit cycle, coalesced to one pass per runloop turn.
        /// Every mutation path in this file funnels through `textViewDidChange`
        /// — typing, autocorrect/dictation, paste, drag-drop, undo, and the
        /// three places that mutate storage directly and then call it by hand —
        /// so this is the single hook the decoration needs.
        private func scheduleChipPass(delay: TimeInterval = 0) {
            guard !chipPassScheduled else { return }
            chipPassScheduled = true
            let work = { [weak self] in
                self?.chipPassScheduled = false
                self?.applyChips()
            }
            if delay > 0 {
                DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: work)
            } else {
                DispatchQueue.main.async(execute: work)
            }
        }

        private func applyChips() {
            guard let tv = textView, let model, let blockId, !tv.isReadOnlyRendering else { return }
            // Never rewrite storage mid-composition: replacing the whole string
            // out from under an IME's marked range drops or garbles the
            // in-flight characters (and UIKit can raise on the stale range).
            // Re-arm instead of dropping the pass — a ref can newly resolve at
            // any moment (the lookup cache re-checks misses, and the member
            // list syncs in), so the chip would otherwise never appear.
            guard tv.markedTextRange == nil else {
                scheduleChipPass(delay: 0.2)
                return
            }
            let lengthBefore = tv.textStorage.length
            let result = model.chipDecoration(for: tv.textStorage, selection: tv.selectedRange)
            guard result.changed else { return }
            beginProgrammaticChange()
            tv.textStorage.beginEditing()
            tv.textStorage.setAttributedString(result.attributed)
            tv.textStorage.endEditing()
            // Keep the SELECTION, not just the caret: collapsing it here wiped
            // an active selection whenever a ref happened to resolve.
            let length = tv.textStorage.length
            let location = min(result.selection.location, length)
            tv.selectedRange = NSRange(
                location: location,
                length: min(result.selection.length, length - location)
            )
            // Without this the NEXT typed character inherits the chip's color
            // and marker attributes.
            tv.typingAttributes = MarkdownChipDecorator.sanitizedTypingAttributes(tv.typingAttributes)
            endProgrammaticChange()
            // No revision bump: the text view already holds this content, and a
            // bump would re-apply `attributedText` and disturb the caret.
            // The mapped selection HAS to travel with it: `textViewDidChangeSelection`
            // is suppressed during a programmatic change, so the model would
            // otherwise keep pre-pass offsets for a post-pass document.
            model.applyDecoration(
                id: blockId,
                content: NSAttributedString(attributedString: tv.textStorage),
                selection: tv.selectedRange,
                lengthDelta: length - lengthBefore
            )
        }

        /// Scrolls the nearest enclosing scroll view (the SwiftUI ScrollView
        /// hosting the editor) so the caret AND a margin below it are visible.
        /// The margin keeps the row under the focused field — the comment
        /// composer's send button — above the keyboard too (EXP-135).
        private func scrollCaretIntoView(_ tv: UITextView) {
            guard let selection = tv.selectedTextRange else { return }
            var ancestor = tv.superview
            while let view = ancestor, !(view is UIScrollView) { ancestor = view.superview }
            guard let scrollView = ancestor as? UIScrollView else { return }
            let caret = tv.caretRect(for: selection.end)
            guard !caret.isNull, !caret.isInfinite else { return }
            var target = tv.convert(caret, to: scrollView)
            // Cap the margin to what the viewport can actually show — the
            // fixed +88 exceeded the composer's small bounded scroller and
            // pushed the caret out of view (EXP-246).
            target.size.height += min(88, max(0, scrollView.bounds.height - caret.height - 8))
            scrollView.scrollRectToVisible(target, animated: true)
        }

        func textViewDidChangeSelection(_ tv: UITextView) {
            // A chip is one atom for the caret too (EXP-655): the caret rests
            // before the `#` or after the whole pill, never between its
            // characters. `allowSeam` is the programmatic case — the caret
            // applyChips / a revision re-apply / consumeDesiredSelection leaves
            // between token and title is deliberate, so typing keeps extending
            // the token; any USER caret (a tap on the title, arrow keys) snaps
            // out of the pill.
            if !isSnappingCaret, tv.isEditable, tv.markedTextRange == nil,
               tv.selectedRange.length == 0 {
                let snapped = MarkdownChipDecorator.snappedCaret(
                    in: tv.textStorage,
                    proposed: tv.selectedRange.location,
                    previous: lastCollapsedCaret,
                    allowSeam: isProgrammaticChange
                )
                if snapped != tv.selectedRange.location {
                    isSnappingCaret = true
                    tv.selectedRange = NSRange(location: snapped, length: 0)
                    isSnappingCaret = false
                }
            }
            lastCollapsedCaret = tv.selectedRange.length == 0 ? tv.selectedRange.location : nil
            if !isProgrammaticChange, let model, let blockId {
                model.updateSelection(blockId: blockId, range: tv.selectedRange)
            }
            // UITextView recomputes typing attributes from the character before
            // the caret on every selection change, so parking the caret at a
            // chip's edge re-poisons them (EXP-322).
            tv.typingAttributes = MarkdownChipDecorator.sanitizedTypingAttributes(tv.typingAttributes)
            (tv.inputAccessoryView as? MarkdownToolbar)?.updateState()
        }

        func textView(_ tv: UITextView, shouldChangeTextIn range: NSRange, replacementText text: String) -> Bool {
            let storage = tv.textStorage

            // EXP-726 — table cell: Return leaves for the next cell, and a
            // pasted multi-line block collapses to one line. A cell is ONE
            // inline paragraph on every client, so a newline may never enter
            // its storage (the serializer would flatten it anyway; keeping the
            // display honest is the point).
            if singleLine, text.rangeOfCharacter(from: .newlines) != nil {
                if text.count == 1 {
                    onReturn?()
                    return false
                }
                let flattened = text.components(separatedBy: .newlines).joined(separator: " ")
                let attrs = MarkdownChipDecorator.sanitizedTypingAttributes(tv.typingAttributes)
                storage.beginEditing()
                storage.replaceCharacters(
                    in: range, with: NSAttributedString(string: flattened, attributes: attrs))
                storage.endEditing()
                tv.selectedRange = NSRange(
                    location: range.location + (flattened as NSString).length, length: 0)
                tv.typingAttributes = attrs
                textViewDidChange(tv)
                return false
            }

            // Copy/paste (or an intra-app text drag) of a chip arrives as the
            // PLAIN text "#EXP-42\u{FFFC}": `allowsEditingTextAttributes` is
            // off, so the display-only title attachment does not survive the
            // pasteboard and the bare object-replacement character would be
            // saved as a stray `￼` for every client. Drop it on the way in —
            // the decoration pass re-adds a real attachment a turn later.
            // (The serializer strips U+FFFC too; this keeps the caret and the
            // on-screen text honest as well.) Runs BEFORE the empty-storage
            // guard below: pasting into an empty block must be sanitized too.
            if text.contains("\u{FFFC}") {
                let clean = expWithoutObjectReplacements(text)
                let attrs = MarkdownChipDecorator.sanitizedTypingAttributes(tv.typingAttributes)
                storage.beginEditing()
                storage.replaceCharacters(
                    in: range, with: NSAttributedString(string: clean, attributes: attrs))
                storage.endEditing()
                tv.selectedRange = NSRange(
                    location: range.location + (clean as NSString).length, length: 0)
                tv.typingAttributes = attrs
                textViewDidChange(tv)
                return false
            }

            guard storage.length > 0 else { return true }
            let nsString = storage.string as NSString

            // A chip deletes as one atom (EXP-322). Backspacing at its right
            // edge would otherwise remove only the display-only title
            // attachment, which the next decoration pass re-inserts — so
            // backspace would appear stuck.
            if text.isEmpty, range.length > 0,
               let atom = MarkdownChipDecorator.chipAtomRange(in: storage, endingAt: NSMaxRange(range)),
               atom.location < range.location {
                storage.beginEditing()
                storage.replaceCharacters(in: atom, with: "")
                storage.endEditing()
                tv.selectedRange = NSRange(location: atom.location, length: 0)
                tv.typingAttributes = MarkdownChipDecorator.sanitizedTypingAttributes(tv.typingAttributes)
                textViewDidChange(tv)
                return false
            }

            // Backspace on an empty list item → exit list mode.
            if text.isEmpty, range.length > 0 {
                let paraRange = nsString.safeParagraphRange(at: range.location)
                if let attrs = storage.attributesIfInBounds(at: paraRange.location),
                   attrs[.markdownListType] as? String != nil {
                    let paraText = nsString.substring(with: paraRange).trimmingCharacters(in: .newlines)
                    let listContent = stripListPrefix(paraText)
                    if listContent.isEmpty {
                        clearListParagraph(tv: tv, storage: storage, paraRange: paraRange)
                        return false
                    }
                }
            }

            // Enter in a list → continue or exit.
            guard text == "\n" else { return true }
            let paraRange = nsString.safeParagraphRange(at: range.location)

            // Enter at the end of a heading starts a BODY paragraph (EXP-568).
            // UITextView otherwise carries the heading font AND
            // `.markdownHeadingLevel` into the new line, so the next paragraph
            // silently serialized as a second heading.
            if let attrs = storage.attributesIfInBounds(at: paraRange.location),
               let level = attrs[.markdownHeadingLevel] as? Int, level > 0 {
                let tail = nsString
                    .substring(with: NSRange(
                        location: range.location,
                        length: max(0, NSMaxRange(paraRange) - range.location)))
                    .trimmingCharacters(in: .newlines)
                if tail.isEmpty {
                    let base = MarkdownStyle.baseAttributes
                    storage.replaceCharacters(
                        in: range, with: NSAttributedString(string: "\n", attributes: base))
                    tv.selectedRange = NSRange(location: range.location + 1, length: 0)
                    tv.typingAttributes = base
                    textViewDidChange(tv)
                    return false
                }
            }

            guard let attrs = storage.attributesIfInBounds(at: paraRange.location),
                  let listType = attrs[.markdownListType] as? String else { return true }

            let paraText = nsString.substring(with: paraRange).trimmingCharacters(in: .newlines)
            let listContent = stripListPrefix(paraText)
            if listContent.isEmpty {
                clearListParagraph(tv: tv, storage: storage, paraRange: paraRange)
                return false
            }

            let prefix: String
            var newAttrs = attrs
            if listType == "ordered" {
                let prev = (attrs[.markdownListItemIndex] as? Int) ?? 1
                newAttrs[.markdownListItemIndex] = prev + 1
                prefix = "\(prev + 1). "
            } else if listType == "checklist" {
                prefix = "\u{2610} "
            } else {
                prefix = "\u{2022} "
            }
            storage.replaceCharacters(in: range, with: NSAttributedString(string: "\n\(prefix)", attributes: newAttrs))
            tv.selectedRange = NSRange(location: range.location + 1 + prefix.count, length: 0)
            tv.typingAttributes = newAttrs
            textViewDidChange(tv)
            return false
        }

        private func stripListPrefix(_ text: String) -> String {
            text.replacingOccurrences(
                of: #"^(\d+\.\s|[\u{2022}\u{2610}\u{2611}]\s?)"#,
                with: "",
                options: .regularExpression
            ).trimmingCharacters(in: .whitespaces)
        }

        private func clearListParagraph(tv: UITextView, storage: NSTextStorage, paraRange: NSRange) {
            if paraRange.length > 0, NSMaxRange(paraRange) <= storage.length {
                storage.replaceCharacters(in: paraRange, with: "")
            }
            let style = NSMutableParagraphStyle()
            style.lineSpacing = 4
            var typing = MarkdownStyle.baseAttributes
            typing[.paragraphStyle] = style
            tv.typingAttributes = typing
            textViewDidChange(tv)
        }
    }
}

// MARK: - Block Image View

private struct BlockImageView: View {
    let model: IssueEditorModel
    let blockId: UUID
    let url: String
    let alt: String
    let baseURL: URL?
    let accountId: String
    let httpClient: HTTPClient?
    let pendingImages: [String: PendingImage]
    var isReadOnly = false
    /// Height cap for compact contexts (the comment composer) — the aspect-fit
    /// image shrinks to fit, leading-aligned.
    var maxHeight: CGFloat?
    var onDelete: () -> Void
    var onTapBelow: () -> Void
    var onRetry: () -> Void

    @State private var loadedImage: UIImage?
    @State private var failed = false

    private var uploadState: ImageUploadState { model.uploadState(for: blockId) }

    /// Aspect ratio (width / height) used to reserve space before/while loading,
    /// preventing the layout jump. Sourced from the decoded image, then the
    /// pending image's measured dimensions, then a 4:3 fallback.
    private var aspectRatio: CGFloat {
        if let img = loadedImage, img.size.height > 0 {
            return img.size.width / img.size.height
        }
        if let pending = pendingImages[url], let w = pending.width, let h = pending.height, h > 0 {
            return CGFloat(w) / CGFloat(h)
        }
        return 4.0 / 3.0
    }

    var body: some View {
        VStack(spacing: 0) {
            ZStack(alignment: .topTrailing) {
                imageBody
                    .frame(maxWidth: .infinity)
                    .aspectRatio(aspectRatio, contentMode: .fit)
                    .frame(maxHeight: maxHeight ?? .infinity, alignment: .leading)
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                    .animation(.easeInOut(duration: 0.15), value: aspectRatio)

                if !isReadOnly {
                    Button(action: onDelete) {
                        // Lucide's circle-x is a stroke glyph with no palette
                        // fill, so the legibility the filled symbol got for
                        // free comes from a scrim behind it instead.
                        AppIcon(AppIcons.uiClear, size: 22)
                            .foregroundStyle(.white.opacity(0.9))
                            .padding(2)
                            .background(Circle().fill(.black.opacity(0.5)))
                    }
                    .padding(8)
                }
            }
            .padding(.vertical, 4)

            if !isReadOnly {
                Color.clear
                    .frame(height: 20)
                    .contentShape(Rectangle())
                    .onTapGesture { onTapBelow() }
            }
        }
        .task(id: url) { await loadImage() }
    }

    @ViewBuilder
    private var imageBody: some View {
        if let loadedImage {
            ZStack(alignment: .bottomLeading) {
                Image(uiImage: loadedImage)
                    .resizable()
                    .aspectRatio(contentMode: .fit)
                if uploadState == .uploading {
                    uploadingOverlay
                } else if case .failed(let reason) = uploadState {
                    uploadFailedOverlay(reason)
                }
            }
        } else if failed {
            placeholderTile {
                Button(action: onRetry) {
                    VStack(spacing: 6) {
                        AppIcon(AppIcons.uiRefresh, size: 24)
                        Text("Tap to retry")
                            .font(.caption)
                    }
                    .foregroundStyle(.white.opacity(0.6))
                }
            }
        } else {
            placeholderTile {
                ProgressView().tint(.white.opacity(0.4))
            }
        }
    }

    @ViewBuilder
    private func placeholderTile<Content: View>(@ViewBuilder content: () -> Content) -> some View {
        RoundedRectangle(cornerRadius: 8)
            .fill(Color.white.opacity(0.06))
            .overlay { content() }
    }

    private var uploadingOverlay: some View {
        HStack(spacing: 6) {
            ProgressView().tint(.white).controlSize(.small)
            Text("Uploading…").font(.caption).foregroundStyle(.white)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(.black.opacity(0.45), in: Capsule())
        .padding(8)
    }

    /// Upload-failed badge on a still-visible draft image. Storage-full gets
    /// an explanation (neutral copy — no billing language, EXP-216) instead of
    /// looking like a transient error; retry stays available either way.
    private func uploadFailedOverlay(_ reason: ImageUploadFailureReason) -> some View {
        Button(action: onRetry) {
            HStack(spacing: 6) {
                AppIcon(AppIcons.uiWarning, size: AppIcon.Size.small)
                Text(reason == .storageFull
                    ? "Team storage is full. Tap to retry"
                    : "Upload failed. Tap to retry")
                    .font(.caption)
            }
            .foregroundStyle(.white)
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .background(.black.opacity(0.45), in: Capsule())
            .padding(8)
        }
        .buttonStyle(.plain)
    }

    private func loadImage() async {
        failed = false
        let loader = AttachmentImageLoader(
            baseURL: baseURL,
            accountId: accountId,
            httpClient: httpClient,
            pendingImages: pendingImages
        )
        do {
            let image = try await loader.load(url)
            loadedImage = image
        } catch {
            // Keep any previously-loaded image (e.g. across a draft→real URL
            // swap) rather than flashing the placeholder.
            if loadedImage == nil {
                failed = true
                log.error("Image load failed for \(url, privacy: .public): \(error.localizedDescription)")
            }
        }
    }
}

// MARK: - Bounded editor height

/// Wraps an editor in a vertical ScrollView whose visible height hugs the
/// content between `minHeight` and `maxHeight`. The editor deliberately has
/// no internal scrolling, and a bare `.frame(maxHeight:)` clamp does NOT clip
/// an overflowing child — content taller than the clamp rendered centered
/// outside the composer card with the caret visually detached (EXP-246).
/// Safe re the file-top NOTE: that concerned an UNBOUNDED-height nested
/// ScrollView blowing the width out; a vertical ScrollView proposes its own
/// finite width, which sizeThatFits adopts.
struct BoundedEditorHeight: ViewModifier {
    let minHeight: CGFloat
    let maxHeight: CGFloat
    @State private var contentHeight: CGFloat = 0

    func body(content: Content) -> some View {
        ScrollView {
            content
                .onGeometryChange(for: CGFloat.self, of: { $0.size.height }) { newHeight in
                    contentHeight = newHeight
                }
        }
        .frame(height: min(max(contentHeight, minHeight), maxHeight))
        .scrollBounceBehavior(.basedOnSize)
    }
}

extension View {
    /// See `BoundedEditorHeight`.
    func boundedEditorHeight(minHeight: CGFloat, maxHeight: CGFloat) -> some View {
        modifier(BoundedEditorHeight(minHeight: minHeight, maxHeight: maxHeight))
    }
}
