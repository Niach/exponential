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

    @State private var photoItem: PhotosPickerItem?
    @State private var showPhotoPicker = false
    @State private var showLinkAlert = false
    @State private var linkURLText = ""
    @State private var toolbar = MarkdownToolbar()

    var body: some View {
        ScrollViewReader { _ in
            ScrollView {
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
                                onPasteImage: { image in insert(uiImage: image) }
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
                                onDelete: { model.deleteImageBlock(id: id) },
                                onTapBelow: { focusBlock(after: id) },
                                onRetry: { triggerRetry() }
                            )
                            .id(id)
                        }
                    }
                }
                .padding(.horizontal, 8)
                .padding(.top, 12)
                .padding(.bottom, 60)
            }
        }
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
            toolbar.onImagePick = { showPhotoPicker = true }
            toolbar.onInsertLink = { showLinkAlert = true }
        }
    }

    private func isSolePlaceholderBlock(_ id: UUID) -> Bool {
        model.blocks.count == 1 && model.blocks.first?.id == id
    }

    private func focusBlock(after id: UUID) {
        guard let idx = model.blocks.firstIndex(where: { $0.id == id }), idx + 1 < model.blocks.count else { return }
        model.setFocused(model.blocks[idx + 1].id)
    }

    private func triggerRetry() {
        // Retry simply re-runs the host's commit; failed drafts still carry
        // their in-memory data, so the next commit re-uploads them.
        model.onEdit?()
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
    var onPasteImage: (UIImage) -> Void

    func makeUIView(context: Context) -> EditorTextView {
        let tv = EditorTextView()
        tv.backgroundColor = .clear
        tv.textColor = MarkdownStyle.textColor
        tv.tintColor = MarkdownStyle.linkColor
        tv.font = MarkdownStyle.bodyFont
        tv.isEditable = true
        tv.isScrollEnabled = false
        tv.alwaysBounceVertical = false
        tv.textContainerInset = UIEdgeInsets(top: 4, left: 0, bottom: 4, right: 0)
        tv.keyboardAppearance = .dark // app chrome is forced-dark
        tv.autocorrectionType = .default
        tv.autocapitalizationType = .sentences
        tv.typingAttributes = MarkdownStyle.baseAttributes
        tv.inputAccessoryView = toolbar
        tv.delegate = context.coordinator

        let coord = context.coordinator
        coord.textView = tv
        coord.model = model
        coord.blockId = blockId
        coord.onPasteImage = onPasteImage
        coord.appliedRevision = revision

        tv.onDeleteBackwardAtStart = { [weak coord] in coord?.handleDeleteBackwardAtStart() }
        tv.onPasteImage = { [weak coord] image in coord?.onPasteImage?(image) }

        coord.beginProgrammaticChange()
        tv.attributedText = content
        coord.endProgrammaticChange()
        if content.length == 0, let placeholder {
            coord.showPlaceholder(in: tv, text: placeholder)
        }
        return tv
    }

    func updateUIView(_ tv: EditorTextView, context: Context) {
        let coord = context.coordinator
        coord.model = model
        coord.blockId = blockId
        coord.onPasteImage = onPasteImage
        tv.onDeleteBackwardAtStart = { [weak coord] in coord?.handleDeleteBackwardAtStart() }
        tv.onPasteImage = { [weak coord] image in coord?.onPasteImage?(image) }

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

        if isFocused, !tv.isFirstResponder {
            tv.becomeFirstResponder()
            toolbar.textView = tv
        }
        if tv.isFirstResponder {
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

                Button(action: onDelete) {
                    Image(systemName: "xmark.circle.fill")
                        .font(.system(size: 22))
                        .symbolRenderingMode(.palette)
                        .foregroundStyle(.white.opacity(0.8), .black.opacity(0.5))
                }
                .padding(8)
            }
            .padding(.vertical, 4)

            Color.clear
                .frame(height: 20)
                .contentShape(Rectangle())
                .onTapGesture { onTapBelow() }
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
