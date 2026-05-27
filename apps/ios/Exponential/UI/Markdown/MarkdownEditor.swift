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

    @State private var photoItem: PhotosPickerItem?
    @State private var showPhotoPicker = false

    var body: some View {
        MarkdownEditorRepresentable(
            text: $text,
            pendingImages: $pendingImages,
            placeholder: placeholder,
            baseURL: baseURL,
            accountId: accountId,
            httpClient: httpClient,
            showPhotoPicker: $showPhotoPicker
        )
        .photosPicker(isPresented: $showPhotoPicker, selection: $photoItem, matching: .images)
        .onChange(of: photoItem) { _, newItem in
            guard let newItem else { return }
            Task { await ingestPhoto(newItem) }
        }
    }

    private func ingestPhoto(_ item: PhotosPickerItem) async {
        defer { photoItem = nil }
        guard let data = try? await item.loadTransferable(type: Data.self) else { return }
        let contentType = item.supportedContentTypes.first?.preferredMIMEType ?? "image/jpeg"
        let ext = item.supportedContentTypes.first?.preferredFilenameExtension ?? "jpg"
        let filename = "image-\(Int(Date().timeIntervalSince1970)).\(ext)"
        let draftUrl = MarkdownImageUtils.draftUrl()
        pendingImages[draftUrl] = PendingImage(data: data, filename: filename, contentType: contentType)
        text += "\n![image](\(draftUrl))\n"
    }
}

// MARK: - UITextView subclass that blocks cursor on image lines

private final class BlockImageTextView: UITextView {
    override func closestPosition(to point: CGPoint) -> UITextPosition? {
        guard let pos = super.closestPosition(to: point) else { return nil }
        if isOnImageLine(pos) {
            return nudgeOffImageLine(pos) ?? pos
        }
        return pos
    }

    override func closestPosition(to point: CGPoint, within range: UITextRange) -> UITextPosition? {
        guard let pos = super.closestPosition(to: point, within: range) else { return nil }
        if isOnImageLine(pos) {
            return nudgeOffImageLine(pos) ?? pos
        }
        return pos
    }

    override func paste(_ sender: Any?) {
        let pb = UIPasteboard.general
        if pb.hasImages, let image = pb.image, let data = image.jpegData(compressionQuality: 0.85) {
            if let coordinator = delegate as? MarkdownEditorRepresentable.Coordinator {
                coordinator.pasteImageData(data, filename: "pasted-\(Int(Date().timeIntervalSince1970)).jpg", contentType: "image/jpeg")
                return
            }
        }
        super.paste(sender)
    }

    override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
        if action == #selector(paste(_:)) && UIPasteboard.general.hasImages {
            return true
        }
        return super.canPerformAction(action, withSender: sender)
    }

    private func isOnImageLine(_ position: UITextPosition) -> Bool {
        let offset = self.offset(from: beginningOfDocument, to: position)
        guard offset >= 0, textStorage.length > 0 else { return false }
        let loc = min(offset, textStorage.length - 1)
        let paraRange = (textStorage.string as NSString).paragraphRange(for: NSRange(location: loc, length: 0))
        guard paraRange.length > 0, paraRange.location < textStorage.length else { return false }
        var found = false
        textStorage.enumerateAttribute(.attachment, in: paraRange, options: []) { val, _, stop in
            if val is NSTextAttachment { found = true; stop.pointee = true }
        }
        return found
    }

    private func nudgeOffImageLine(_ position: UITextPosition) -> UITextPosition? {
        let offset = self.offset(from: beginningOfDocument, to: position)
        guard textStorage.length > 0 else { return nil }
        let loc = min(offset, textStorage.length - 1)
        let paraRange = (textStorage.string as NSString).paragraphRange(for: NSRange(location: loc, length: 0))
        let afterPara = NSMaxRange(paraRange)
        if afterPara < textStorage.length {
            return self.position(from: beginningOfDocument, offset: afterPara)
        }
        if paraRange.location > 0 {
            return self.position(from: beginningOfDocument, offset: paraRange.location - 1)
        }
        return nil
    }
}

// MARK: - UIViewRepresentable

private struct MarkdownEditorRepresentable: UIViewRepresentable {
    @Binding var text: String
    @Binding var pendingImages: [String: PendingImage]
    var placeholder: String
    var baseURL: URL?
    var accountId: String
    var httpClient: HTTPClient?
    @Binding var showPhotoPicker: Bool

    func makeUIView(context: Context) -> BlockImageTextView {
        let textView = BlockImageTextView()
        textView.backgroundColor = .clear
        textView.textColor = MarkdownStyle.textColor
        textView.tintColor = MarkdownStyle.linkColor
        textView.font = MarkdownStyle.bodyFont
        textView.isEditable = true
        textView.isScrollEnabled = false
        textView.alwaysBounceVertical = false
        textView.textContainerInset = UIEdgeInsets(top: 12, left: 8, bottom: 60, right: 8)
        textView.keyboardAppearance = .dark
        textView.autocorrectionType = .default
        textView.autocapitalizationType = .sentences
        textView.typingAttributes = MarkdownStyle.baseAttributes

        let toolbar = MarkdownToolbar()
        toolbar.textView = textView
        toolbar.onImagePick = { showPhotoPicker = true }
        textView.inputAccessoryView = toolbar

        let coordinator = context.coordinator
        textView.delegate = coordinator
        coordinator.toolbar = toolbar

        let attributed = MarkdownConversion.markdownToAttributedString(text, baseURL: baseURL)
        textView.attributedText = attributed
        coordinator.lastMarkdown = text

        if attributed.length == 0 {
            coordinator.showPlaceholder(in: textView, placeholder: placeholder)
        }

        loadImages(for: textView, coordinator: coordinator)

        return textView
    }

    func updateUIView(_ textView: BlockImageTextView, context: Context) {
        guard !context.coordinator.isUpdating else { return }
        guard text != context.coordinator.lastMarkdown else { return }

        context.coordinator.isUpdating = true
        context.coordinator.imageLoadTasks.removeAll()
        let savedRange = textView.selectedRange
        let attributed = MarkdownConversion.markdownToAttributedString(text, baseURL: baseURL)
        textView.attributedText = attributed
        context.coordinator.lastMarkdown = text

        let clampedLocation = min(savedRange.location, textView.textStorage.length)
        let clampedLength = min(savedRange.length, textView.textStorage.length - clampedLocation)
        textView.selectedRange = NSRange(location: clampedLocation, length: clampedLength)

        if attributed.length == 0 {
            context.coordinator.showPlaceholder(in: textView, placeholder: placeholder)
        } else {
            context.coordinator.hidePlaceholder(in: textView)
        }

        loadImages(for: textView, coordinator: context.coordinator)
        context.coordinator.isUpdating = false
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    private func loadImages(for textView: UITextView, coordinator: Coordinator) {
        let attrText = textView.attributedText!
        guard attrText.length > 0 else { return }

        attrText.enumerateAttribute(.markdownImageURL, in: NSRange(location: 0, length: attrText.length), options: []) { value, _, _ in
            guard let urlStr = value as? String else { return }
            guard coordinator.imageLoadTasks[urlStr] == nil else { return }

            coordinator.imageLoadTasks[urlStr] = Task { @MainActor in
                do {
                    let data: Data
                    if urlStr.hasPrefix("draft://"), let pending = pendingImages[urlStr] {
                        data = pending.data
                    } else if urlStr.contains("/api/"), let httpClient, !accountId.isEmpty,
                              let url = URL(string: urlStr) {
                        let (d, _) = try await httpClient.get(url, accountId: accountId)
                        data = d
                    } else if let url = URL(string: urlStr) {
                        let (d, _) = try await URLSession.shared.data(from: url)
                        data = d
                    } else {
                        return
                    }

                    guard let fullImage = UIImage(data: data) else { return }

                    let maxWidth = textView.bounds.width
                        - textView.textContainerInset.left
                        - textView.textContainerInset.right
                        - 2 * textView.textContainer.lineFragmentPadding
                    let displayWidth = max(min(maxWidth, fullImage.size.width), 100)
                    let aspectRatio = fullImage.size.height / fullImage.size.width
                    let displayHeight = displayWidth * aspectRatio
                    let displaySize = CGSize(width: displayWidth, height: displayHeight)

                    let scaledImage = UIGraphicsImageRenderer(size: displaySize).image { _ in
                        fullImage.draw(in: CGRect(origin: .zero, size: displaySize))
                    }

                    textView.textStorage.enumerateAttribute(.markdownImageURL, in: NSRange(location: 0, length: textView.textStorage.length), options: []) { val, attrRange, _ in
                        guard let val = val as? String, val == urlStr else { return }
                        textView.textStorage.enumerateAttribute(.attachment, in: attrRange, options: []) { att, _, _ in
                            guard let attachment = att as? NSTextAttachment else { return }
                            attachment.image = scaledImage
                            attachment.bounds = CGRect(origin: .zero, size: displaySize)
                        }
                    }

                    textView.layoutManager.invalidateDisplay(forCharacterRange: NSRange(location: 0, length: textView.textStorage.length))
                } catch {
                    log.error("Image load failed: \(error.localizedDescription)")
                }
            }
        }
    }

    // MARK: - Coordinator

    @MainActor
    final class Coordinator: NSObject, UITextViewDelegate {
        var parent: MarkdownEditorRepresentable
        var isUpdating = false
        var lastMarkdown = ""
        var debounceTask: Task<Void, Never>?
        var imageLoadTasks: [String: Task<Void, Never>] = [:]
        weak var toolbar: MarkdownToolbar?
        private var placeholderLabel: UILabel?

        init(parent: MarkdownEditorRepresentable) {
            self.parent = parent
        }

        func pasteImageData(_ data: Data, filename: String, contentType: String) {
            let draftUrl = MarkdownImageUtils.draftUrl()
            parent.pendingImages[draftUrl] = PendingImage(data: data, filename: filename, contentType: contentType)
            parent.text += "\n![image](\(draftUrl))\n"
        }

        func showPlaceholder(in textView: UITextView, placeholder: String) {
            if placeholderLabel == nil {
                let label = UILabel()
                label.text = placeholder
                label.font = MarkdownStyle.bodyFont
                label.textColor = MarkdownStyle.placeholderColor
                label.translatesAutoresizingMaskIntoConstraints = false
                label.isUserInteractionEnabled = false
                textView.addSubview(label)
                NSLayoutConstraint.activate([
                    label.topAnchor.constraint(equalTo: textView.topAnchor, constant: textView.textContainerInset.top),
                    label.leadingAnchor.constraint(equalTo: textView.leadingAnchor, constant: textView.textContainerInset.left + textView.textContainer.lineFragmentPadding),
                ])
                placeholderLabel = label
            }
            placeholderLabel?.isHidden = false
        }

        func hidePlaceholder(in textView: UITextView) {
            placeholderLabel?.isHidden = true
        }

        // MARK: UITextViewDelegate

        func textViewDidChange(_ textView: UITextView) {
            guard !isUpdating else { return }

            if textView.textStorage.length == 0 {
                showPlaceholder(in: textView, placeholder: parent.placeholder)
            } else {
                hidePlaceholder(in: textView)
            }

            debounceTask?.cancel()
            debounceTask = Task { @MainActor [weak self] in
                try? await Task.sleep(for: .milliseconds(500))
                guard !Task.isCancelled else { return }
                self?.flushToMarkdown(textView)
            }
        }

        func textViewDidChangeSelection(_ textView: UITextView) {
            toolbar?.updateState()
        }

        func textViewDidEndEditing(_ textView: UITextView) {
            debounceTask?.cancel()
            flushToMarkdown(textView)
        }

        func textView(_ textView: UITextView, shouldChangeTextIn range: NSRange, replacementText text: String) -> Bool {
            let storage = textView.textStorage

            // Block any edit on an image line
            if storage.length > 0 {
                let loc = min(range.location, storage.length - 1)
                guard loc >= 0 else { return true }
                let paraRange = (storage.string as NSString).paragraphRange(for: NSRange(location: loc, length: 0))
                if paraRange.location < storage.length, paraRange.length > 0 {
                    var hasImage = false
                    storage.enumerateAttribute(.attachment, in: paraRange, options: []) { val, _, stop in
                        if val is NSTextAttachment { hasImage = true; stop.pointee = true }
                    }
                    if hasImage {
                        if text.isEmpty { return false }
                        let afterPara = NSMaxRange(paraRange)
                        let insertPos: Int
                        if afterPara < storage.length {
                            insertPos = afterPara
                        } else {
                            storage.append(NSAttributedString(string: "\n", attributes: MarkdownStyle.baseAttributes))
                            insertPos = storage.length
                        }
                        storage.insert(NSAttributedString(string: text, attributes: MarkdownStyle.baseAttributes), at: insertPos)
                        textView.selectedRange = NSRange(location: insertPos + text.count, length: 0)
                        textViewDidChange(textView)
                        return false
                    }
                }
            }

            // Backspace on empty list item → exit list mode
            if text.isEmpty, range.length > 0, storage.length > 0 {
                let loc = min(range.location, storage.length - 1)
                let paraRange = (storage.string as NSString).paragraphRange(for: NSRange(location: loc, length: 0))
                guard paraRange.location < storage.length else { return true }
                let attrs = storage.attributes(at: paraRange.location, effectiveRange: nil)
                if attrs[.markdownListType] as? String != nil {
                    let paraText = (storage.string as NSString).substring(with: paraRange).trimmingCharacters(in: .newlines)
                    let contentOnly = paraText
                        .replacingOccurrences(of: #"^(\d+\.\s|[\u{2022}\u{2610}\u{2611}]\s?)"#, with: "", options: .regularExpression)
                        .trimmingCharacters(in: .whitespaces)
                    if contentOnly.isEmpty {
                        if paraRange.length > 0 { storage.replaceCharacters(in: paraRange, with: "") }
                        let style = NSMutableParagraphStyle()
                        style.lineSpacing = 4
                        textView.typingAttributes = MarkdownStyle.baseAttributes
                        textView.typingAttributes[.paragraphStyle] = style
                        textView.typingAttributes.removeValue(forKey: .markdownListType)
                        textView.typingAttributes.removeValue(forKey: .markdownListItemIndex)
                        textView.typingAttributes.removeValue(forKey: .markdownListDepth)
                        textViewDidChange(textView)
                        return false
                    }
                }
            }

            // Enter in list → continue or exit
            guard text == "\n" else { return true }
            guard range.location >= 0, storage.length > 0 else { return true }

            let clampedLoc = min(range.location, storage.length - 1)
            let paraRange = (storage.string as NSString).paragraphRange(for: NSRange(location: max(0, clampedLoc), length: 0))
            guard paraRange.location < storage.length else { return true }
            let attrs = storage.attributes(at: paraRange.location, effectiveRange: nil)
            guard let listType = attrs[.markdownListType] as? String else { return true }

            let paraText = (storage.string as NSString).substring(with: paraRange).trimmingCharacters(in: .newlines)
            let contentOnly = paraText
                .replacingOccurrences(of: #"^(\d+\.\s|[\u{2022}\u{2610}\u{2611}]\s?)"#, with: "", options: .regularExpression)
                .trimmingCharacters(in: .whitespaces)

            if contentOnly.isEmpty {
                if paraRange.length > 0 { storage.replaceCharacters(in: paraRange, with: "") }
                let style = NSMutableParagraphStyle()
                style.lineSpacing = 4
                textView.typingAttributes = MarkdownStyle.baseAttributes
                textView.typingAttributes[.paragraphStyle] = style
                textView.typingAttributes.removeValue(forKey: .markdownListType)
                textView.typingAttributes.removeValue(forKey: .markdownListItemIndex)
                textView.typingAttributes.removeValue(forKey: .markdownListDepth)
                textViewDidChange(textView)
                return false
            }

            let newIndex: Int
            let prefix: String
            if listType == "ordered" {
                let prevIndex = (attrs[.markdownListItemIndex] as? Int) ?? 1
                newIndex = prevIndex + 1
                prefix = "\(newIndex). "
            } else if listType == "checklist" {
                newIndex = 0
                prefix = "\u{2610} "
            } else {
                newIndex = 0
                prefix = "\u{2022} "
            }

            let newLineStr = "\n\(prefix)"
            var newAttrs = attrs
            newAttrs[.markdownListItemIndex] = newIndex
            storage.replaceCharacters(in: range, with: NSAttributedString(string: newLineStr, attributes: newAttrs))
            textView.selectedRange = NSRange(location: range.location + newLineStr.count, length: 0)
            textView.typingAttributes = newAttrs
            textViewDidChange(textView)
            return false
        }

        // Checkbox toggle via interaction delegate
        func textView(_ textView: UITextView, shouldInteractWith textAttachment: NSTextAttachment, in characterRange: NSRange, interaction: UITextItemInteraction) -> Bool {
            false
        }

        private func flushToMarkdown(_ textView: UITextView) {
            isUpdating = true
            let md = MarkdownConversion.attributedStringToMarkdown(textView.attributedText)
            lastMarkdown = md
            parent.text = md
            isUpdating = false
        }
    }
}
