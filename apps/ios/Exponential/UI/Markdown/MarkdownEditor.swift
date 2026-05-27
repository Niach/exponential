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
        let placeholder = MarkdownImageUtils.draftUrl()
        pendingImages[placeholder] = PendingImage(
            data: data,
            filename: filename,
            contentType: contentType
        )
        text += "\n![image](\(placeholder))\n"
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

    func makeUIView(context: Context) -> UITextView {
        let textView = UITextView()
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
        toolbar.onImagePick = {
            showPhotoPicker = true
        }
        textView.inputAccessoryView = toolbar

        let coordinator = context.coordinator
        textView.delegate = coordinator
        coordinator.toolbar = toolbar

        let tapGesture = UITapGestureRecognizer(target: coordinator, action: #selector(Coordinator.handleTap(_:)))
        tapGesture.delegate = coordinator
        textView.addGestureRecognizer(tapGesture)

        let attributed = MarkdownConversion.markdownToAttributedString(text, baseURL: baseURL)
        textView.attributedText = attributed
        coordinator.lastMarkdown = text

        if attributed.length == 0 {
            coordinator.showPlaceholder(in: textView, placeholder: placeholder)
        }

        loadImages(for: textView, coordinator: coordinator)

        return textView
    }

    func updateUIView(_ textView: UITextView, context: Context) {
        guard !context.coordinator.isUpdating else { return }
        guard text != context.coordinator.lastMarkdown else { return }

        log.debug("updateUIView: text changed externally, re-parsing (\(text.count) chars)")
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
        let fullRange = NSRange(location: 0, length: attrText.length)

        attrText.enumerateAttribute(.markdownImageURL, in: fullRange, options: []) { value, range, _ in
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
                    let displayScale = displayWidth / fullImage.size.width
                    let displayHeight = fullImage.size.height * displayScale
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
    final class Coordinator: NSObject, UITextViewDelegate, UIGestureRecognizerDelegate {
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
            guard !isUpdating else { return }
            redirectCursorFromImageLine(textView)
        }

        private func redirectCursorFromImageLine(_ textView: UITextView) {
            let storage = textView.textStorage
            guard storage.length > 0 else { return }
            let sel = textView.selectedRange
            guard sel.length == 0 else { return }

            let loc = min(sel.location, storage.length - 1)
            guard loc >= 0 else { return }
            let paraRange = (storage.string as NSString).paragraphRange(for: NSRange(location: loc, length: 0))
            guard paraRange.location < storage.length else { return }

            var hasImage = false
            storage.enumerateAttribute(.attachment, in: paraRange, options: []) { val, _, stop in
                if val is NSTextAttachment { hasImage = true; stop.pointee = true }
            }
            guard hasImage else { return }

            isUpdating = true
            let afterPara = NSMaxRange(paraRange)
            if afterPara < storage.length {
                textView.selectedRange = NSRange(location: afterPara, length: 0)
            } else if paraRange.location > 0 {
                textView.selectedRange = NSRange(location: paraRange.location - 1, length: 0)
            }
            isUpdating = false
        }

        func textViewDidEndEditing(_ textView: UITextView) {
            debounceTask?.cancel()
            flushToMarkdown(textView)
        }

        func textView(_ textView: UITextView, shouldChangeTextIn range: NSRange, replacementText text: String) -> Bool {
            let storage = textView.textStorage

            // Prevent typing on the same line as an image — redirect to next line
            if !text.isEmpty, storage.length > 0 {
                let loc = min(range.location, storage.length - 1)
                let paraRange = (storage.string as NSString).paragraphRange(for: NSRange(location: max(0, loc), length: 0))
                if paraRange.location < storage.length {
                    var hasImage = false
                    storage.enumerateAttribute(.attachment, in: paraRange, options: []) { val, _, stop in
                        if val is NSTextAttachment { hasImage = true; stop.pointee = true }
                    }
                    if hasImage {
                        let afterImage = NSMaxRange(paraRange)
                        if afterImage <= storage.length {
                            let insertAttrs = MarkdownStyle.baseAttributes
                            if afterImage == storage.length || (storage.string as NSString).character(at: afterImage - 1) != 0x0A {
                                storage.insert(NSAttributedString(string: "\n", attributes: insertAttrs), at: afterImage)
                            }
                            let newPos = min(afterImage, storage.length)
                            storage.insert(NSAttributedString(string: text, attributes: insertAttrs), at: newPos)
                            textView.selectedRange = NSRange(location: newPos + text.count, length: 0)
                            textViewDidChange(textView)
                            return false
                        }
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
                    let paraText = (storage.string as NSString).substring(with: paraRange)
                        .trimmingCharacters(in: .newlines)
                    let contentOnly = paraText
                        .replacingOccurrences(of: #"^(\d+\.\s|[\u{2022}\u{2610}\u{2611}]\s?)"#, with: "", options: .regularExpression)
                        .trimmingCharacters(in: .whitespaces)
                    if contentOnly.isEmpty {
                        let style = NSMutableParagraphStyle()
                        style.lineSpacing = 4
                        if paraRange.length > 0 {
                            storage.replaceCharacters(in: paraRange, with: "")
                        }
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

            let paraText = (storage.string as NSString).substring(with: paraRange)
                .trimmingCharacters(in: .newlines)

            let contentOnly = paraText
                .replacingOccurrences(of: #"^(\d+\.\s|[\u{2022}\u{2610}\u{2611}]\s?)"#, with: "", options: .regularExpression)
                .trimmingCharacters(in: .whitespaces)

            if contentOnly.isEmpty {
                let style = NSMutableParagraphStyle()
                style.lineSpacing = 4
                if paraRange.length > 0 {
                    storage.replaceCharacters(in: paraRange, with: "")
                }
                textView.typingAttributes = MarkdownStyle.baseAttributes
                textView.typingAttributes[.paragraphStyle] = style
                textView.typingAttributes.removeValue(forKey: .markdownListType)
                textView.typingAttributes.removeValue(forKey: .markdownListItemIndex)
                textView.typingAttributes.removeValue(forKey: .markdownListDepth)
                textViewDidChange(textView)
                return false
            }

            let depth = (attrs[.markdownListDepth] as? Int) ?? 0
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
            let newAttrStr = NSAttributedString(string: newLineStr, attributes: newAttrs)

            storage.replaceCharacters(in: range, with: newAttrStr)
            textView.selectedRange = NSRange(location: range.location + newLineStr.count, length: 0)

            textView.typingAttributes = newAttrs
            textView.typingAttributes[.markdownListDepth] = depth

            textViewDidChange(textView)
            return false
        }

        // Allow tap gesture to coexist with text view's built-in gestures
        func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool {
            true
        }

        @objc func handleTap(_ gesture: UITapGestureRecognizer) {
            guard let textView = gesture.view as? UITextView else { return }
            let point = gesture.location(in: textView)
            let layoutManager = textView.layoutManager
            let textContainer = textView.textContainer

            var fraction: CGFloat = 0
            let glyphIndex = layoutManager.glyphIndex(for: point, in: textContainer, fractionOfDistanceThroughGlyph: &fraction)
            let charIndex = layoutManager.characterIndexForGlyph(at: glyphIndex)

            guard charIndex < textView.textStorage.length else { return }

            let char = (textView.textStorage.string as NSString).substring(with: NSRange(location: charIndex, length: 1))

            if char == "\u{2610}" || char == "\u{2611}" {
                let replacement = char == "\u{2610}" ? "\u{2611}" : "\u{2610}"
                let attrs = textView.textStorage.attributes(at: charIndex, effectiveRange: nil)
                textView.textStorage.replaceCharacters(in: NSRange(location: charIndex, length: 1),
                                                        with: NSAttributedString(string: replacement, attributes: attrs))
                textViewDidChange(textView)
            }
        }

        private func flushToMarkdown(_ textView: UITextView) {
            isUpdating = true
            log.debug("flushToMarkdown: length=\(textView.textStorage.length)")
            let md = MarkdownConversion.attributedStringToMarkdown(textView.attributedText)
            log.debug("flushToMarkdown: md=\(md.prefix(200))")
            lastMarkdown = md
            parent.text = md
            isUpdating = false
        }
    }
}
