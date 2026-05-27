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

// MARK: - UITextView subclass

private final class BlockImageTextView: UITextView {

    // Intercept touch-based cursor placement: if it lands on an image line, move off
    override func closestPosition(to point: CGPoint) -> UITextPosition? {
        guard let pos = super.closestPosition(to: point) else { return nil }
        return isOnImageLine(pos) ? (nudgeOffImageLine(pos) ?? pos) : pos
    }

    override func closestPosition(to point: CGPoint, within range: UITextRange) -> UITextPosition? {
        guard let pos = super.closestPosition(to: point, within: range) else { return nil }
        return isOnImageLine(pos) ? (nudgeOffImageLine(pos) ?? pos) : pos
    }

    // Paste images from clipboard
    override func paste(_ sender: Any?) {
        let pb = UIPasteboard.general
        if pb.hasImages, let image = pb.image {
            if let coordinator = delegate as? MarkdownEditorRepresentable.Coordinator {
                coordinator.pasteImage(image)
                return
            }
        }
        super.paste(sender)
    }

    override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
        if action == #selector(paste(_:)) && UIPasteboard.general.hasImages { return true }
        return super.canPerformAction(action, withSender: sender)
    }

    func isOnImageLine(_ position: UITextPosition) -> Bool {
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

    func nudgeOffImageLine(_ position: UITextPosition) -> UITextPosition? {
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

    func imageParaRange(at charIndex: Int) -> NSRange? {
        guard textStorage.length > 0, charIndex >= 0 else { return nil }
        let loc = min(charIndex, textStorage.length - 1)
        let paraRange = (textStorage.string as NSString).paragraphRange(for: NSRange(location: loc, length: 0))
        guard paraRange.length > 0, paraRange.location < textStorage.length else { return nil }
        var found = false
        textStorage.enumerateAttribute(.attachment, in: paraRange, options: []) { val, _, stop in
            if val is NSTextAttachment { found = true; stop.pointee = true }
        }
        return found ? paraRange : nil
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
        let tv = BlockImageTextView()
        tv.backgroundColor = .clear
        tv.textColor = MarkdownStyle.textColor
        tv.tintColor = MarkdownStyle.linkColor
        tv.font = MarkdownStyle.bodyFont
        tv.isEditable = true
        tv.isScrollEnabled = false
        tv.alwaysBounceVertical = false
        tv.textContainerInset = UIEdgeInsets(top: 12, left: 8, bottom: 60, right: 8)
        tv.keyboardAppearance = .dark
        tv.autocorrectionType = .default
        tv.autocapitalizationType = .sentences
        tv.typingAttributes = MarkdownStyle.baseAttributes

        let toolbar = MarkdownToolbar()
        toolbar.textView = tv
        toolbar.onImagePick = { showPhotoPicker = true }
        tv.inputAccessoryView = toolbar

        let coord = context.coordinator
        tv.delegate = coord
        coord.toolbar = toolbar
        coord.textView = tv

        let attributed = MarkdownConversion.markdownToAttributedString(text, baseURL: baseURL)
        tv.attributedText = attributed
        coord.lastMarkdown = text

        if attributed.length == 0 { coord.showPlaceholder(in: tv, placeholder: placeholder) }
        loadImages(for: tv, coordinator: coord)
        return tv
    }

    func updateUIView(_ tv: BlockImageTextView, context: Context) {
        guard !context.coordinator.isUpdating else { return }
        guard text != context.coordinator.lastMarkdown else { return }

        context.coordinator.isUpdating = true
        context.coordinator.imageLoadTasks.removeAll()
        let savedRange = tv.selectedRange
        let attributed = MarkdownConversion.markdownToAttributedString(text, baseURL: baseURL)
        tv.attributedText = attributed
        context.coordinator.lastMarkdown = text

        let pos = min(savedRange.location, tv.textStorage.length)
        tv.selectedRange = NSRange(location: pos, length: 0)

        if attributed.length == 0 {
            context.coordinator.showPlaceholder(in: tv, placeholder: placeholder)
        } else {
            context.coordinator.hidePlaceholder(in: tv)
        }

        loadImages(for: tv, coordinator: context.coordinator)
        context.coordinator.isUpdating = false
    }

    func makeCoordinator() -> Coordinator { Coordinator(parent: self) }

    private func loadImages(for tv: UITextView, coordinator: Coordinator) {
        guard tv.attributedText.length > 0 else { return }

        tv.attributedText.enumerateAttribute(.markdownImageURL, in: NSRange(location: 0, length: tv.attributedText.length), options: []) { value, _, _ in
            guard let urlStr = value as? String, coordinator.imageLoadTasks[urlStr] == nil else { return }

            coordinator.imageLoadTasks[urlStr] = Task { @MainActor in
                do {
                    let data: Data
                    if urlStr.hasPrefix("draft://"), let pending = pendingImages[urlStr] {
                        data = pending.data
                    } else if urlStr.contains("/api/"), let httpClient, !accountId.isEmpty, let url = URL(string: urlStr) {
                        let (d, _) = try await httpClient.get(url, accountId: accountId)
                        data = d
                    } else if let url = URL(string: urlStr) {
                        let (d, _) = try await URLSession.shared.data(from: url)
                        data = d
                    } else { return }

                    guard let fullImage = UIImage(data: data) else { return }
                    let scaled = Self.scaleImage(fullImage, maxWidth: tv.bounds.width - tv.textContainerInset.left - tv.textContainerInset.right - 2 * tv.textContainer.lineFragmentPadding)

                    tv.textStorage.enumerateAttribute(.markdownImageURL, in: NSRange(location: 0, length: tv.textStorage.length), options: []) { val, attrRange, _ in
                        guard let val = val as? String, val == urlStr else { return }
                        tv.textStorage.enumerateAttribute(.attachment, in: attrRange, options: []) { att, _, _ in
                            guard let attachment = att as? NSTextAttachment else { return }
                            attachment.image = scaled
                            attachment.bounds = CGRect(origin: .zero, size: scaled.size)
                        }
                    }
                    tv.layoutManager.invalidateDisplay(forCharacterRange: NSRange(location: 0, length: tv.textStorage.length))
                } catch {
                    log.error("Image load failed: \(error.localizedDescription)")
                }
            }
        }
    }

    static func scaleImage(_ image: UIImage, maxWidth: CGFloat) -> UIImage {
        let w = max(min(maxWidth, image.size.width), 100)
        let h = w * (image.size.height / image.size.width)
        let size = CGSize(width: w, height: h)
        return UIGraphicsImageRenderer(size: size).image { _ in
            image.draw(in: CGRect(origin: .zero, size: size))
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
        weak var textView: BlockImageTextView?
        private var placeholderLabel: UILabel?

        init(parent: MarkdownEditorRepresentable) { self.parent = parent }

        func pasteImage(_ image: UIImage) {
            guard let data = image.jpegData(compressionQuality: 0.85) else { return }
            let draftUrl = MarkdownImageUtils.draftUrl()
            parent.pendingImages[draftUrl] = PendingImage(data: data, filename: "pasted-\(Int(Date().timeIntervalSince1970)).jpg", contentType: "image/jpeg")
            parent.text += "\n![image](\(draftUrl))\n"
        }

        func showPlaceholder(in tv: UITextView, placeholder: String) {
            if placeholderLabel == nil {
                let label = UILabel()
                label.text = placeholder
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

        func hidePlaceholder(in tv: UITextView) { placeholderLabel?.isHidden = true }

        // MARK: UITextViewDelegate

        func textViewDidChange(_ tv: UITextView) {
            guard !isUpdating else { return }
            if tv.textStorage.length == 0 { showPlaceholder(in: tv, placeholder: parent.placeholder) }
            else { hidePlaceholder(in: tv) }

            debounceTask?.cancel()
            debounceTask = Task { @MainActor [weak self] in
                try? await Task.sleep(for: .milliseconds(500))
                guard !Task.isCancelled else { return }
                self?.flushToMarkdown(tv)
            }
        }

        func textViewDidChangeSelection(_ tv: UITextView) {
            toolbar?.updateState()
            guard !isUpdating else { return }

            // Async cursor correction — after UIKit finishes its selection handling
            let sel = tv.selectedRange
            guard sel.length == 0, let blockTV = tv as? BlockImageTextView else { return }
            if let pos = blockTV.position(from: blockTV.beginningOfDocument, offset: sel.location),
               blockTV.isOnImageLine(pos) {
                DispatchQueue.main.async { [weak blockTV] in
                    guard let blockTV else { return }
                    if let newPos = blockTV.nudgeOffImageLine(pos) {
                        let newOffset = blockTV.offset(from: blockTV.beginningOfDocument, to: newPos)
                        blockTV.selectedRange = NSRange(location: newOffset, length: 0)
                    }
                }
            }
        }

        func textViewDidEndEditing(_ tv: UITextView) {
            debounceTask?.cancel()
            flushToMarkdown(tv)
        }

        func textView(_ tv: UITextView, shouldChangeTextIn range: NSRange, replacementText text: String) -> Bool {
            let storage = tv.textStorage
            guard storage.length > 0 else { return true }

            let blockTV = tv as? BlockImageTextView

            // Backspace right after an image line → delete the image
            if text.isEmpty, range.length > 0, range.location > 0 {
                let prevCharIdx = range.location - 1
                if let imgRange = blockTV?.imageParaRange(at: prevCharIdx) {
                    storage.replaceCharacters(in: imgRange, with: "")
                    tv.selectedRange = NSRange(location: max(0, imgRange.location), length: 0)
                    textViewDidChange(tv)
                    return false
                }
            }

            // Block all edits on image lines — redirect typed text below
            let loc = min(range.location, storage.length - 1)
            guard loc >= 0 else { return true }
            if let imgRange = blockTV?.imageParaRange(at: loc) {
                if text.isEmpty { return false }
                let afterImg = NSMaxRange(imgRange)
                let insertPos: Int
                if afterImg < storage.length {
                    insertPos = afterImg
                } else {
                    storage.append(NSAttributedString(string: "\n", attributes: MarkdownStyle.baseAttributes))
                    insertPos = storage.length
                }
                storage.insert(NSAttributedString(string: text, attributes: MarkdownStyle.baseAttributes), at: insertPos)
                tv.selectedRange = NSRange(location: insertPos + text.count, length: 0)
                textViewDidChange(tv)
                return false
            }

            // Backspace on empty list item → exit list mode
            if text.isEmpty, range.length > 0 {
                let paraRange = (storage.string as NSString).paragraphRange(for: NSRange(location: min(range.location, storage.length - 1), length: 0))
                guard paraRange.location < storage.length else { return true }
                let attrs = storage.attributes(at: paraRange.location, effectiveRange: nil)
                if attrs[.markdownListType] as? String != nil {
                    let paraText = (storage.string as NSString).substring(with: paraRange).trimmingCharacters(in: .newlines)
                    let content = paraText.replacingOccurrences(of: #"^(\d+\.\s|[\u{2022}\u{2610}\u{2611}]\s?)"#, with: "", options: .regularExpression).trimmingCharacters(in: .whitespaces)
                    if content.isEmpty {
                        if paraRange.length > 0 { storage.replaceCharacters(in: paraRange, with: "") }
                        let style = NSMutableParagraphStyle(); style.lineSpacing = 4
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
            let content = paraText.replacingOccurrences(of: #"^(\d+\.\s|[\u{2022}\u{2610}\u{2611}]\s?)"#, with: "", options: .regularExpression).trimmingCharacters(in: .whitespaces)

            if content.isEmpty {
                if paraRange.length > 0 { storage.replaceCharacters(in: paraRange, with: "") }
                let style = NSMutableParagraphStyle(); style.lineSpacing = 4
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

        func textView(_ tv: UITextView, shouldInteractWith attachment: NSTextAttachment, in range: NSRange, interaction: UITextItemInteraction) -> Bool { false }

        private func flushToMarkdown(_ tv: UITextView) {
            isUpdating = true
            let md = MarkdownConversion.attributedStringToMarkdown(tv.attributedText)
            lastMarkdown = md
            parent.text = md
            isUpdating = false
        }
    }
}
