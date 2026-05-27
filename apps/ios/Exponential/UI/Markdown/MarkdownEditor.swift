import os
import PhotosUI
import SwiftUI
import UIKit

private let log = Logger(subsystem: "com.exponential", category: "MarkdownEditor")

// MARK: - Public SwiftUI View

struct MarkdownEditor: View {
    @Binding var text: String
    @Binding var pendingImages: [String: PendingImage]
    var placeholder: String = "Add description..."
    var baseURL: URL?
    var accountId: String = ""
    var httpClient: HTTPClient?

    @State private var blocks: [ContentBlock] = []
    @State private var focusedBlockId: UUID?
    @State private var lastFlushedMarkdown: String = ""
    @State private var photoItem: PhotosPickerItem?
    @State private var showPhotoPicker = false
    @State private var cursorAfterMerge: (blockId: UUID, position: Int)?
    @State private var toolbar = MarkdownToolbar()

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                VStack(spacing: 0) {
                    ForEach(blocks) { block in
                        switch block {
                        case .text(let id, let content):
                            BlockTextView(
                                blockId: id,
                                content: content,
                                placeholder: blocks.count == 1 && blocks.first?.id == id ? placeholder : nil,
                                isFocused: focusedBlockId == id,
                                toolbar: toolbar,
                                cursorPosition: cursorAfterMerge?.blockId == id ? cursorAfterMerge?.position : nil,
                                onTextChange: { newContent in
                                    updateTextBlock(id: id, content: newContent)
                                },
                                onFocus: {
                                    focusedBlockId = id
                                },
                                onBlur: {
                                    if focusedBlockId == id { focusedBlockId = nil }
                                },
                                onDeleteBackwardAtStart: {
                                    deleteImageBefore(textBlockId: id)
                                },
                                onPasteImage: { image in
                                    pasteImage(image)
                                }
                            )
                            .id(id)

                        case .image(let id, let url, _):
                            BlockImageView(
                                url: url,
                                pendingImages: pendingImages,
                                baseURL: baseURL,
                                accountId: accountId,
                                httpClient: httpClient,
                                onDelete: { deleteImageBlock(id: id) },
                                onTapBelow: {
                                    if let idx = blocks.firstIndex(where: { $0.id == id }), idx + 1 < blocks.count {
                                        focusedBlockId = blocks[idx + 1].id
                                    }
                                }
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
        .onAppear {
            toolbar.onImagePick = { showPhotoPicker = true }
            syncBlocksFromMarkdown()
        }
        .onChange(of: text) { _, newText in
            guard newText != lastFlushedMarkdown else { return }
            syncBlocksFromMarkdown()
        }
    }

    // MARK: - Sync

    private func syncBlocksFromMarkdown() {
        blocks = MarkdownConversion.markdownToBlocks(text, baseURL: baseURL)
    }

    private func flushBlocksToMarkdown() {
        let md = MarkdownConversion.blocksToMarkdown(blocks)
        lastFlushedMarkdown = md
        text = md
    }

    // MARK: - Block Manipulation

    private func updateTextBlock(id: UUID, content: NSAttributedString) {
        guard let idx = blocks.firstIndex(where: { $0.id == id }) else { return }
        blocks[idx] = .text(id: id, attributedContent: content)
        flushBlocksToMarkdown()
    }

    private func deleteImageBefore(textBlockId: UUID) {
        guard let textIndex = blocks.firstIndex(where: { $0.id == textBlockId }),
              textIndex > 0,
              case .image = blocks[textIndex - 1] else { return }

        if textIndex >= 2,
           case .text(let prevId, let prevContent) = blocks[textIndex - 2],
           case .text(_, let currentContent) = blocks[textIndex] {
            let merged = NSMutableAttributedString(attributedString: prevContent)
            let mergePoint = merged.length
            merged.append(currentContent)
            blocks.replaceSubrange((textIndex - 2)...textIndex, with: [
                .text(id: prevId, attributedContent: merged),
            ])
            cursorAfterMerge = (blockId: prevId, position: mergePoint)
            focusedBlockId = prevId
        } else {
            blocks.remove(at: textIndex - 1)
            ContentBlock.normalize(&blocks)
        }
        flushBlocksToMarkdown()
        DispatchQueue.main.async { cursorAfterMerge = nil }
    }

    private func deleteImageBlock(id: UUID) {
        guard let index = blocks.firstIndex(where: { $0.id == id }) else { return }
        let prevIndex = index - 1
        let nextIndex = index + 1

        if prevIndex >= 0, nextIndex < blocks.count,
           case .text(let prevId, let prevContent) = blocks[prevIndex],
           case .text(_, let nextContent) = blocks[nextIndex] {
            let merged = NSMutableAttributedString(attributedString: prevContent)
            merged.append(nextContent)
            blocks.replaceSubrange(prevIndex...nextIndex, with: [
                .text(id: prevId, attributedContent: merged),
            ])
            focusedBlockId = prevId
        } else {
            blocks.remove(at: index)
            ContentBlock.normalize(&blocks)
        }
        flushBlocksToMarkdown()
    }

    // MARK: - Image Insertion

    private func ingestPhoto(_ item: PhotosPickerItem) async {
        defer { photoItem = nil }
        guard let data = try? await item.loadTransferable(type: Data.self) else { return }
        let contentType = item.supportedContentTypes.first?.preferredMIMEType ?? "image/jpeg"
        let ext = item.supportedContentTypes.first?.preferredFilenameExtension ?? "jpg"
        let filename = "image-\(Int(Date().timeIntervalSince1970)).\(ext)"
        insertImageData(data: data, filename: filename, contentType: contentType)
    }

    private func pasteImage(_ image: UIImage) {
        guard let data = image.jpegData(compressionQuality: 0.85) else { return }
        insertImageData(data: data, filename: "pasted-\(Int(Date().timeIntervalSince1970)).jpg", contentType: "image/jpeg")
    }

    private func insertImageData(data: Data, filename: String, contentType: String) {
        let draftUrl = MarkdownImageUtils.draftUrl()
        pendingImages[draftUrl] = PendingImage(data: data, filename: filename, contentType: contentType)

        guard let focusedId = focusedBlockId,
              let blockIndex = blocks.firstIndex(where: { $0.id == focusedId }),
              case .text(_, let content) = blocks[blockIndex] else {
            let afterId = UUID()
            blocks.append(.image(id: UUID(), url: draftUrl, alt: "image"))
            blocks.append(.text(id: afterId, attributedContent: NSAttributedString()))
            ContentBlock.normalize(&blocks)
            focusedBlockId = afterId
            flushBlocksToMarkdown()
            return
        }

        let cursorPos = toolbar.textView?.selectedRange.location ?? content.length

        let beforeContent: NSAttributedString
        let afterContent: NSAttributedString

        if cursorPos <= 0 {
            beforeContent = NSAttributedString()
            afterContent = content
        } else if cursorPos >= content.length {
            beforeContent = content
            afterContent = NSAttributedString()
        } else {
            beforeContent = content.attributedSubstring(from: NSRange(location: 0, length: cursorPos))
            afterContent = content.attributedSubstring(from: NSRange(location: cursorPos, length: content.length - cursorPos))
        }

        let beforeId = UUID()
        let afterId = UUID()

        blocks.replaceSubrange(blockIndex...blockIndex, with: [
            .text(id: beforeId, attributedContent: beforeContent),
            .image(id: UUID(), url: draftUrl, alt: "image"),
            .text(id: afterId, attributedContent: afterContent),
        ])

        focusedBlockId = afterId
        flushBlocksToMarkdown()
    }
}

// MARK: - Block Text View (UIViewRepresentable)

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

private struct BlockTextView: UIViewRepresentable {
    let blockId: UUID
    let content: NSAttributedString
    let placeholder: String?
    let isFocused: Bool
    let toolbar: MarkdownToolbar
    let cursorPosition: Int?

    var onTextChange: (NSAttributedString) -> Void
    var onFocus: () -> Void
    var onBlur: () -> Void
    var onDeleteBackwardAtStart: () -> Void
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
        tv.keyboardAppearance = .dark
        tv.autocorrectionType = .default
        tv.autocapitalizationType = .sentences
        tv.typingAttributes = MarkdownStyle.baseAttributes
        tv.inputAccessoryView = toolbar

        tv.delegate = context.coordinator
        context.coordinator.textView = tv

        let coordinator = context.coordinator
        tv.onDeleteBackwardAtStart = { [weak coordinator] in
            coordinator?.onDeleteBackwardAtStart()
        }
        tv.onPasteImage = { [weak coordinator] image in
            coordinator?.onPasteImage(image)
        }

        tv.attributedText = content
        if content.length == 0, let placeholder {
            coordinator.showPlaceholder(in: tv, text: placeholder)
        }

        return tv
    }

    func updateUIView(_ tv: EditorTextView, context: Context) {
        let coord = context.coordinator
        guard !coord.isUpdating else { return }
        coord.isUpdating = true

        coord.onTextChangeCallback = onTextChange
        coord.onFocusCallback = onFocus
        coord.onBlurCallback = onBlur
        coord.onDeleteBackwardCallback = onDeleteBackwardAtStart
        coord.onPasteImageCallback = onPasteImage

        tv.onDeleteBackwardAtStart = { [weak coord] in coord?.onDeleteBackwardAtStart() }
        tv.onPasteImage = { [weak coord] image in coord?.onPasteImage(image) }

        if !coord.hasLocalEdits(for: content) {
            let savedRange = tv.selectedRange
            tv.attributedText = content
            let pos = min(savedRange.location, tv.textStorage.length)
            tv.selectedRange = NSRange(location: pos, length: 0)
            coord.lastContent = content
        }

        if let cursorPosition {
            let pos = min(cursorPosition, tv.textStorage.length)
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

        coord.isUpdating = false
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    @MainActor
    final class Coordinator: NSObject, UITextViewDelegate {
        var isUpdating = false
        var lastContent: NSAttributedString?
        var debounceTask: Task<Void, Never>?
        weak var textView: EditorTextView?
        private var placeholderLabel: UILabel?

        var onTextChangeCallback: ((NSAttributedString) -> Void)?
        var onFocusCallback: (() -> Void)?
        var onBlurCallback: (() -> Void)?
        var onDeleteBackwardCallback: (() -> Void)?
        var onPasteImageCallback: ((UIImage) -> Void)?

        func hasLocalEdits(for externalContent: NSAttributedString) -> Bool {
            guard let last = lastContent else { return false }
            return last.string != externalContent.string
        }

        func onDeleteBackwardAtStart() {
            debounceTask?.cancel()
            if let tv = textView { flushNow(tv) }
            onDeleteBackwardCallback?()
        }

        func onPasteImage(_ image: UIImage) {
            onPasteImageCallback?(image)
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
            onFocusCallback?()
        }

        func textViewDidChange(_ tv: UITextView) {
            guard !isUpdating else { return }
            if tv.textStorage.length == 0 {
                placeholderLabel?.isHidden = false
            } else {
                placeholderLabel?.isHidden = true
            }

            debounceTask?.cancel()
            debounceTask = Task { @MainActor [weak self] in
                try? await Task.sleep(for: .milliseconds(300))
                guard !Task.isCancelled else { return }
                self?.flushNow(tv)
            }
        }

        func textViewDidChangeSelection(_ tv: UITextView) {
            if let toolbar = tv.inputAccessoryView as? MarkdownToolbar {
                toolbar.updateState()
            }
        }

        func textViewDidEndEditing(_ tv: UITextView) {
            debounceTask?.cancel()
            flushNow(tv)
            onBlurCallback?()
        }

        func textView(_ tv: UITextView, shouldChangeTextIn range: NSRange, replacementText text: String) -> Bool {
            let storage = tv.textStorage
            guard storage.length > 0 else { return true }

            // Backspace on empty list item → exit list mode
            if text.isEmpty, range.length > 0 {
                let clampedLoc = min(range.location, storage.length - 1)
                guard clampedLoc >= 0 else { return true }
                let paraRange = (storage.string as NSString).paragraphRange(for: NSRange(location: clampedLoc, length: 0))
                guard paraRange.location < storage.length else { return true }
                let attrs = storage.attributes(at: paraRange.location, effectiveRange: nil)
                if attrs[.markdownListType] as? String != nil {
                    let paraText = (storage.string as NSString).substring(with: paraRange).trimmingCharacters(in: .newlines)
                    let content = paraText.replacingOccurrences(of: #"^(\d+\.\s|[\u{2022}\u{2610}\u{2611}]\s?)"#, with: "", options: .regularExpression).trimmingCharacters(in: .whitespaces)
                    if content.isEmpty {
                        if paraRange.length > 0 { storage.replaceCharacters(in: paraRange, with: "") }
                        let style = NSMutableParagraphStyle()
                        style.lineSpacing = 4
                        tv.typingAttributes = MarkdownStyle.baseAttributes
                        tv.typingAttributes[.paragraphStyle] = style
                        tv.typingAttributes.removeValue(forKey: .markdownListType)
                        tv.typingAttributes.removeValue(forKey: .markdownListItemIndex)
                        tv.typingAttributes.removeValue(forKey: .markdownListDepth)
                        textViewDidChange(tv)
                        return false
                    }
                }
            }

            // Enter in list → continue or exit
            guard text == "\n", range.location >= 0, storage.length > 0 else { return true }
            let paraRange = (storage.string as NSString).paragraphRange(for: NSRange(location: min(range.location, storage.length - 1), length: 0))
            guard paraRange.location < storage.length else { return true }
            let attrs = storage.attributes(at: paraRange.location, effectiveRange: nil)
            guard let listType = attrs[.markdownListType] as? String else { return true }

            let paraText = (storage.string as NSString).substring(with: paraRange).trimmingCharacters(in: .newlines)
            let listContent = paraText.replacingOccurrences(of: #"^(\d+\.\s|[\u{2022}\u{2610}\u{2611}]\s?)"#, with: "", options: .regularExpression).trimmingCharacters(in: .whitespaces)

            if listContent.isEmpty {
                if paraRange.length > 0 { storage.replaceCharacters(in: paraRange, with: "") }
                let style = NSMutableParagraphStyle()
                style.lineSpacing = 4
                tv.typingAttributes = MarkdownStyle.baseAttributes
                tv.typingAttributes[.paragraphStyle] = style
                tv.typingAttributes.removeValue(forKey: .markdownListType)
                tv.typingAttributes.removeValue(forKey: .markdownListItemIndex)
                tv.typingAttributes.removeValue(forKey: .markdownListDepth)
                textViewDidChange(tv)
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

        private func flushNow(_ tv: UITextView) {
            isUpdating = true
            let snapshot = NSAttributedString(attributedString: tv.attributedText)
            lastContent = snapshot
            onTextChangeCallback?(snapshot)
            isUpdating = false
        }
    }
}

// MARK: - Block Image View

private struct BlockImageView: View {
    let url: String
    let pendingImages: [String: PendingImage]
    let baseURL: URL?
    let accountId: String
    let httpClient: HTTPClient?
    var onDelete: () -> Void
    var onTapBelow: () -> Void

    @State private var loadedImage: UIImage?

    var body: some View {
        VStack(spacing: 0) {
            ZStack(alignment: .topTrailing) {
                if let loadedImage {
                    Image(uiImage: loadedImage)
                        .resizable()
                        .aspectRatio(contentMode: .fit)
                        .clipShape(RoundedRectangle(cornerRadius: 8))
                } else {
                    RoundedRectangle(cornerRadius: 8)
                        .fill(Color.white.opacity(0.06))
                        .frame(height: 160)
                        .overlay {
                            Image(systemName: "photo")
                                .foregroundStyle(.white.opacity(0.2))
                                .font(.system(size: 32))
                        }
                }

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

    private func loadImage() async {
        do {
            let data: Data
            if url.hasPrefix("draft://"), let pending = pendingImages[url] {
                data = pending.data
            } else if url.contains("/api/"), let httpClient, !accountId.isEmpty {
                let fullUrl: URL
                if let parsed = URL(string: url), parsed.scheme != nil {
                    fullUrl = parsed
                } else if let base = baseURL {
                    let baseStr = base.absoluteString.hasSuffix("/")
                        ? String(base.absoluteString.dropLast())
                        : base.absoluteString
                    guard let resolved = URL(string: baseStr + url) else { return }
                    fullUrl = resolved
                } else { return }
                let (d, _) = try await httpClient.get(fullUrl, accountId: accountId)
                data = d
            } else if let parsed = URL(string: url) {
                let (d, _) = try await URLSession.shared.data(from: parsed)
                data = d
            } else { return }

            guard let image = UIImage(data: data) else { return }
            await MainActor.run { loadedImage = image }
        } catch {
            log.error("Image load failed for \(url): \(error.localizedDescription)")
        }
    }
}
