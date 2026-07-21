import ExpUI
import ExpCore
import os
import PhotosUI
import SwiftUI
import UIKit

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

    @State private var photoItem: PhotosPickerItem?
    @State private var showPhotoPicker = false
    @State private var showLinkAlert = false
    @State private var linkURLText = ""
    @State private var toolbar = MarkdownToolbar()

    // NOTE: deliberately no internal ScrollView. Every usage embeds this
    // editor inside an outer ScrollView (issue detail, create sheet, comment
    // composer); a nested vertical ScrollView proposed an unbounded height
    // reports its content's IDEAL size in both axes, so one long unwrappable
    // line (e.g. a code span) blew the whole column out to ~3× screen width
    // and embedded images rendered at native pixel size.
    var body: some View {
        Group {
                VStack(spacing: 0) {
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
                                toolbar: toolbar,
                                isReadOnly: isReadOnly,
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
                                onDelete: { model.deleteImageBlock(id: id) },
                                onTapBelow: { focusBlock(after: id) },
                                onRetry: { Task { await model.retryImage(blockId: id) } }
                            )
                            .id(id)
                        }
                    }
                }
                .padding(.horizontal, isReadOnly ? 0 : 8)
                .padding(.top, isReadOnly ? 0 : 12)
        }
        .overlay(alignment: .top) {
            // The two token shapes are mutually exclusive, so at most one bar
            // has candidates at a time.
            if !isReadOnly, !model.mentionCandidates.isEmpty {
                mentionBar
            } else if !isReadOnly, !model.issueRefCandidates.isEmpty {
                issueRefBar
            }
        }
        .onChange(of: mentionMembers) { _, newValue in model.mentionMembers = newValue }
        .photosPicker(isPresented: $showPhotoPicker, selection: $photoItem, matching: .images)
        .onChange(of: photoItem) { _, newItem in
            guard let newItem else { return }
            Task { await ingestPhoto(newItem) }
        }
        .alert("Add Link", isPresented: $showLinkAlert) {
            TextField("https://", text: $linkURLText)
                .textInputAutocapitalization(.never)
                .keyboardType(.URL)
            Button("Cancel", role: .cancel) { linkURLText = "" }
            Button("Add") {
                applyLink(urlText: linkURLText)
                linkURLText = ""
            }
        } message: {
            Text("Link the selected text to a URL.")
        }
        .onAppear {
            guard !isReadOnly else { return }
            toolbar.onImagePick = { showPhotoPicker = true }
            toolbar.onInsertLink = { showLinkAlert = true }
            model.mentionMembers = mentionMembers
        }
    }

    // @-mention autocomplete: a non-focus-stealing candidate bar. Tapping inserts
    // the canonical `@email` token via the model (which keeps the text view first
    // responder), so the keyboard never collapses.
    private var mentionBar: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(model.mentionCandidates) { member in
                    Button {
                        model.applyMention(member)
                    } label: {
                        VStack(alignment: .leading, spacing: 0) {
                            Text(member.name).font(.caption.weight(.medium)).foregroundStyle(.white)
                            Text(member.email).font(.caption2).foregroundStyle(.white.opacity(TextOpacity.secondary))
                        }
                        .padding(.horizontal, 10).padding(.vertical, 6)
                        .background(Color.white.opacity(0.1), in: Capsule())
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
        }
        .background(.ultraThinMaterial)
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .padding(.horizontal, 8)
    }

    // #-issue-ref autocomplete: the same non-focus-stealing candidate bar as
    // mentions. Tapping inserts the plain `#IDENTIFIER` interchange token via
    // the model, so the keyboard never collapses and the markdown stays plain.
    private var issueRefBar: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(model.issueRefCandidates) { candidate in
                    Button {
                        model.applyIssueRef(candidate)
                    } label: {
                        VStack(alignment: .leading, spacing: 0) {
                            Text(candidate.identifier).font(.caption.weight(.medium)).foregroundStyle(.white)
                            Text(candidate.title)
                                .font(.caption2)
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                .lineLimit(1)
                                .frame(maxWidth: 160, alignment: .leading)
                        }
                        .padding(.horizontal, 10).padding(.vertical, 6)
                        .background(Color.white.opacity(0.1), in: Capsule())
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
        }
        .background(.ultraThinMaterial)
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .padding(.horizontal, 8)
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

    // MARK: - Link insertion

    private func applyLink(urlText: String) {
        let trimmed = urlText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, let tv = toolbar.textView else { return }
        let normalized = trimmed.contains("://") ? trimmed : "https://\(trimmed)"
        guard let url = URL(string: normalized) else { return }
        let range = tv.selectedRange
        if range.length > 0 {
            tv.textStorage.addAttributes([
                .link: url,
                .foregroundColor: MarkdownStyle.linkColor,
            ], range: range)
        } else {
            let linkText = NSAttributedString(string: normalized, attributes: [
                .link: url,
                .foregroundColor: MarkdownStyle.linkColor,
                .font: MarkdownStyle.bodyFont,
            ])
            tv.textStorage.insert(linkText, at: range.location)
            tv.selectedRange = NSRange(location: range.location + linkText.length, length: 0)
        }
        tv.delegate?.textViewDidChange?(tv)
    }
}

// MARK: - Editor Text View (UITextView subclass)

private final class EditorTextView: UITextView {
    var onDeleteBackwardAtStart: (() -> Void)?
    var onPasteImage: ((UIImage) -> Void)?
    var onIssueRefTap: ((String) -> Void)?
    /// Display-only rendering: issue-ref taps still navigate, but checkbox
    /// glyph taps must not mutate the (never-persisted) text.
    var isReadOnlyRendering = false

    override init(frame: CGRect, textContainer: NSTextContainer?) {
        super.init(frame: frame, textContainer: textContainer)
        let tap = UITapGestureRecognizer(target: self, action: #selector(handleTap(_:)))
        tap.delegate = self
        addGestureRecognizer(tap)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError() }

    @objc private func handleTap(_ gesture: UITapGestureRecognizer) {
        let point = gesture.location(in: self)
        let charIndex = layoutManager.characterIndex(for: point, in: textContainer, fractionOfDistanceBetweenInsertionPoints: nil)
        guard charIndex < textStorage.length else { return }
        // Issue-ref pill: navigate to the referenced issue (render-only
        // decoration applied by IssueEditorModel.load).
        if let issueId = textStorage.attributes(at: charIndex, effectiveRange: nil)[.markdownIssueRef] as? String {
            onIssueRefTap?(issueId)
            return
        }
        guard !isReadOnlyRendering else { return }
        let char = (textStorage.string as NSString).substring(with: NSRange(location: charIndex, length: 1))
        if char == "\u{2610}" || char == "\u{2611}" {
            let replacement = char == "\u{2610}" ? "\u{2611}" : "\u{2610}"
            let attrs = textStorage.attributes(at: charIndex, effectiveRange: nil)
            textStorage.replaceCharacters(in: NSRange(location: charIndex, length: 1), with: NSAttributedString(string: replacement, attributes: attrs))
            delegate?.textViewDidChange?(self)
        }
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

private struct BlockTextEditor: UIViewRepresentable {
    let model: IssueEditorModel
    let blockId: UUID
    let content: NSAttributedString
    let revision: Int
    let isFocused: Bool
    let placeholder: String?
    let toolbar: MarkdownToolbar
    var isReadOnly = false
    var onPasteImage: (UIImage) -> Void
    var onIssueRefTap: ((String) -> Void)?

    func makeUIView(context: Context) -> EditorTextView {
        let tv = EditorTextView()
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
        if !isReadOnly {
            tv.inputAccessoryView = toolbar
        }
        tv.delegate = context.coordinator

        let coord = context.coordinator
        coord.textView = tv
        coord.model = model
        coord.blockId = blockId
        coord.onPasteImage = onPasteImage
        coord.appliedRevision = revision

        tv.onDeleteBackwardAtStart = { [weak coord] in coord?.handleDeleteBackwardAtStart() }
        tv.onPasteImage = { [weak coord] image in coord?.onPasteImage?(image) }
        tv.onIssueRefTap = onIssueRefTap

        coord.beginProgrammaticChange()
        tv.attributedText = content
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
    func sizeThatFits(
        _ proposal: ProposedViewSize,
        uiView tv: EditorTextView,
        context: Context
    ) -> CGSize? {
        guard let width = proposal.width, width.isFinite, width > 0 else {
            return nil
        }
        let fitted = tv.sizeThatFits(
            CGSize(width: width, height: .greatestFiniteMagnitude)
        )
        return CGSize(width: width, height: fitted.height)
    }

    func updateUIView(_ tv: EditorTextView, context: Context) {
        let coord = context.coordinator
        coord.model = model
        coord.blockId = blockId
        coord.onPasteImage = onPasteImage
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
            toolbar.textView = tv
        }
        if !isReadOnly, tv.isFirstResponder {
            toolbar.textView = tv
        }
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    @MainActor
    final class Coordinator: NSObject, UITextViewDelegate {
        weak var textView: EditorTextView?
        var model: IssueEditorModel?
        var blockId: UUID?
        var onPasteImage: ((UIImage) -> Void)?
        var appliedRevision = 0

        private var isProgrammaticChange = false
        private var placeholderLabel: UILabel?

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
            target.size.height += 88
            scrollView.scrollRectToVisible(target, animated: true)
        }

        func textViewDidChangeSelection(_ tv: UITextView) {
            if !isProgrammaticChange, let model, let blockId {
                model.updateSelection(blockId: blockId, range: tv.selectedRange)
            }
            (tv.inputAccessoryView as? MarkdownToolbar)?.updateState()
        }

        func textView(_ tv: UITextView, shouldChangeTextIn range: NSRange, replacementText text: String) -> Bool {
            let storage = tv.textStorage
            guard storage.length > 0 else { return true }
            let nsString = storage.string as NSString

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
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                    .animation(.easeInOut(duration: 0.15), value: aspectRatio)

                if !isReadOnly {
                    Button(action: onDelete) {
                        Image(systemName: "xmark.circle.fill")
                            .font(.system(size: 22))
                            .symbolRenderingMode(.palette)
                            .foregroundStyle(.white.opacity(0.8), .black.opacity(0.5))
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
                        Image(systemName: "arrow.clockwise")
                            .font(.system(size: 24))
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
                Image(systemName: "exclamationmark.triangle")
                    .font(.caption)
                Text(reason == .storageFull
                    ? "Team storage is full — tap to retry"
                    : "Upload failed — tap to retry")
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
