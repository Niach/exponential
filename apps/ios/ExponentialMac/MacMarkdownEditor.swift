import AppKit
import ExpCore
import ExpUI
import os
import SwiftUI

private let log = Logger(subsystem: "at.exponential.mac", category: "MacMarkdownEditor")

// MARK: - Toolbar controller (acts on the focused text view)

@MainActor
@Observable
final class MacEditorToolbarController {
    weak var textView: MacEditorTextView?
    var onPickImage: (() -> Void)?
    var onInsertLink: (() -> Void)?

    struct ToolbarState: Equatable {
        var heading = false, bold = false, italic = false, strike = false
        var bullet = false, ordered = false, checklist = false, code = false, quote = false
    }
    var state = ToolbarState()

    func updateState() {
        guard let tv = textView else { return }
        let attrs = tv.typingAttributes
        let font = attrs[.font] as? NSFont
        let isHeading = (attrs[.markdownHeadingLevel] as? Int).map { $0 > 0 } ?? false
        let listType = attrs[.markdownListType] as? String
        state = ToolbarState(
            heading: isHeading,
            bold: expFontHasBold(font) && !isHeading,
            italic: expFontHasItalic(font),
            strike: (attrs[.strikethroughStyle] as? Int).map { $0 != 0 } ?? false,
            bullet: listType == "bullet",
            ordered: listType == "ordered",
            checklist: listType == "checklist",
            code: (attrs[.markdownCodeBlock] as? Bool) == true,
            quote: (attrs[.markdownBlockquote] as? Bool) == true
        )
    }

    private func refresh() {
        updateState()
        textView?.didChangeText()
    }

    private func paragraphRange(_ tv: MacEditorTextView) -> NSRange {
        guard let storage = tv.textStorage else { return NSRange(location: 0, length: 0) }
        return (storage.string as NSString).safeParagraphRange(at: tv.selectedRange().location)
    }

    // MARK: Inline

    func toggleBold() { toggleFontTrait(.bold) }
    func toggleItalic() { toggleFontTrait(.italic) }

    private func toggleFontTrait(_ trait: NSFontDescriptor.SymbolicTraits) {
        guard let tv = textView, let storage = tv.textStorage else { return }
        let range = tv.selectedRange()
        let currentFont = (tv.typingAttributes[.font] as? NSFont) ?? MarkdownStyle.bodyFont
        var traits = currentFont.fontDescriptor.symbolicTraits
        if traits.contains(trait) { traits.remove(trait) } else { traits.insert(trait) }
        let desc = currentFont.fontDescriptor.withSymbolicTraits(traits)
        let newFont = NSFont(descriptor: desc, size: currentFont.pointSize) ?? currentFont
        if range.length > 0 { storage.addAttribute(.font, value: newFont, range: range) }
        tv.typingAttributes[.font] = newFont
        refresh()
    }

    func toggleStrikethrough() {
        guard let tv = textView, let storage = tv.textStorage else { return }
        let range = tv.selectedRange()
        let current = (tv.typingAttributes[.strikethroughStyle] as? Int) ?? 0
        let newValue = current == 0 ? NSUnderlineStyle.single.rawValue : 0
        if range.length > 0 {
            if newValue == 0 {
                storage.removeAttribute(.strikethroughStyle, range: range)
                storage.removeAttribute(.markdownStrikethrough, range: range)
            } else {
                storage.addAttributes([.strikethroughStyle: newValue, .markdownStrikethrough: true], range: range)
            }
        }
        tv.typingAttributes[.strikethroughStyle] = newValue
        if newValue != 0 {
            tv.typingAttributes[.markdownStrikethrough] = true
        } else {
            tv.typingAttributes.removeValue(forKey: .markdownStrikethrough)
        }
        refresh()
    }

    // MARK: Heading

    func toggleHeading() {
        guard let tv = textView, let storage = tv.textStorage else { return }
        let paraRange = paragraphRange(tv)
        let current = (storage.attributesIfInBounds(at: paraRange.location)?[.markdownHeadingLevel] as? Int)
            ?? (tv.typingAttributes[.markdownHeadingLevel] as? Int) ?? 0
        let nextLevel = current >= 3 ? 0 : current + 1
        if nextLevel > 0 {
            let font = MarkdownStyle.headingFont(level: nextLevel)
            if paraRange.length > 0 {
                storage.addAttributes([.font: font, .markdownHeadingLevel: nextLevel], range: paraRange)
            }
            tv.typingAttributes[.font] = font
            tv.typingAttributes[.markdownHeadingLevel] = nextLevel
        } else {
            if paraRange.length > 0 {
                storage.addAttribute(.font, value: MarkdownStyle.bodyFont, range: paraRange)
                storage.removeAttribute(.markdownHeadingLevel, range: paraRange)
            }
            tv.typingAttributes[.font] = MarkdownStyle.bodyFont
            tv.typingAttributes.removeValue(forKey: .markdownHeadingLevel)
        }
        refresh()
    }

    // MARK: Lists

    func toggleBulletList() { insertOrToggleList(type: "bullet", prefix: "\u{2022} ", initialIndex: 0) }
    func toggleOrderedList() { insertOrToggleList(type: "ordered", prefix: "1. ", initialIndex: 1) }
    func toggleChecklist() { insertOrToggleList(type: "checklist", prefix: "\u{2610} ", initialIndex: 0) }

    private func insertOrToggleList(type: String, prefix: String, initialIndex: Int) {
        guard let tv = textView, let storage = tv.textStorage else { return }
        let paraRange = paragraphRange(tv)
        let currentType = (storage.attributesIfInBounds(at: paraRange.location)?[.markdownListType] as? String)
            ?? (tv.typingAttributes[.markdownListType] as? String)

        if currentType == type {
            if paraRange.length > 0 {
                storage.removeAttribute(.markdownListType, range: paraRange)
                storage.removeAttribute(.markdownListItemIndex, range: paraRange)
                storage.removeAttribute(.markdownListDepth, range: paraRange)
                let style = NSMutableParagraphStyle()
                style.lineSpacing = 4
                storage.addAttribute(.paragraphStyle, value: style, range: paraRange)
            }
            tv.typingAttributes.removeValue(forKey: .markdownListType)
            tv.typingAttributes.removeValue(forKey: .markdownListItemIndex)
            tv.typingAttributes.removeValue(forKey: .markdownListDepth)
        } else {
            let style = NSMutableParagraphStyle()
            style.lineSpacing = 4
            style.headIndent = 24
            style.firstLineHeadIndent = 0
            let listAttrs: [NSAttributedString.Key: Any] = [
                .markdownListType: type,
                .markdownListItemIndex: initialIndex,
                .markdownListDepth: 0,
                .paragraphStyle: style,
            ]
            let paraText = paraRange.length > 0 ? (storage.string as NSString).substring(with: paraRange) : ""
            var insertAttrs = MarkdownStyle.baseAttributes
            insertAttrs.merge(listAttrs) { _, new in new }
            let prefixStr = NSAttributedString(string: prefix, attributes: insertAttrs)
            if paraText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                if paraRange.length > 0 {
                    storage.replaceCharacters(in: paraRange, with: prefixStr)
                } else {
                    storage.insert(prefixStr, at: tv.selectedRange().location)
                }
                tv.setSelectedRange(NSRange(location: paraRange.location + prefix.count, length: 0))
            } else {
                if paraRange.length > 0 { storage.addAttributes(listAttrs, range: paraRange) }
                storage.insert(prefixStr, at: paraRange.location)
                tv.setSelectedRange(NSRange(location: tv.selectedRange().location + prefix.count, length: 0))
            }
            tv.typingAttributes.merge(listAttrs) { _, new in new }
        }
        refresh()
    }

    // MARK: Code / Quote

    func toggleCode() {
        guard let tv = textView, let storage = tv.textStorage else { return }
        let paraRange = paragraphRange(tv)
        let isCode = (storage.attributesIfInBounds(at: paraRange.location)?[.markdownCodeBlock] as? Bool)
            ?? (tv.typingAttributes[.markdownCodeBlock] as? Bool) ?? false
        if isCode {
            if paraRange.length > 0 {
                storage.removeAttribute(.markdownCodeBlock, range: paraRange)
                storage.removeAttribute(.markdownCodeBlockLang, range: paraRange)
                storage.removeAttribute(.backgroundColor, range: paraRange)
                storage.addAttribute(.font, value: MarkdownStyle.bodyFont, range: paraRange)
            }
            tv.typingAttributes.removeValue(forKey: .markdownCodeBlock)
            tv.typingAttributes.removeValue(forKey: .markdownCodeBlockLang)
            tv.typingAttributes.removeValue(forKey: .backgroundColor)
            tv.typingAttributes[.font] = MarkdownStyle.bodyFont
        } else {
            if paraRange.length > 0 {
                storage.addAttributes([
                    .markdownCodeBlock: true,
                    .font: MarkdownStyle.monospaceFont,
                    .backgroundColor: MarkdownStyle.codeBlockBackground,
                ], range: paraRange)
            }
            tv.typingAttributes[.markdownCodeBlock] = true
            tv.typingAttributes[.font] = MarkdownStyle.monospaceFont
            tv.typingAttributes[.backgroundColor] = MarkdownStyle.codeBlockBackground
        }
        refresh()
    }

    func toggleBlockquote() {
        guard let tv = textView, let storage = tv.textStorage else { return }
        let paraRange = paragraphRange(tv)
        let isQuote = (storage.attributesIfInBounds(at: paraRange.location)?[.markdownBlockquote] as? Bool)
            ?? (tv.typingAttributes[.markdownBlockquote] as? Bool) ?? false
        if isQuote {
            if paraRange.length > 0 {
                storage.removeAttribute(.markdownBlockquote, range: paraRange)
                storage.addAttribute(.foregroundColor, value: MarkdownStyle.textColor, range: paraRange)
            }
            tv.typingAttributes.removeValue(forKey: .markdownBlockquote)
            tv.typingAttributes[.foregroundColor] = MarkdownStyle.textColor
        } else {
            if paraRange.length > 0 {
                storage.addAttributes([
                    .markdownBlockquote: true,
                    .foregroundColor: MarkdownStyle.blockquoteTextColor,
                ], range: paraRange)
            }
            tv.typingAttributes[.markdownBlockquote] = true
            tv.typingAttributes[.foregroundColor] = MarkdownStyle.blockquoteTextColor
        }
        refresh()
    }

    func applyLink(_ urlText: String) {
        let trimmed = urlText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, let tv = textView, let storage = tv.textStorage else { return }
        let normalized = trimmed.contains("://") ? trimmed : "https://\(trimmed)"
        guard let url = URL(string: normalized) else { return }
        let range = tv.selectedRange()
        if range.length > 0 {
            storage.addAttributes([.link: url, .foregroundColor: MarkdownStyle.linkColor], range: range)
        } else {
            let linkText = NSAttributedString(string: normalized, attributes: [
                .link: url, .foregroundColor: MarkdownStyle.linkColor, .font: MarkdownStyle.bodyFont,
            ])
            storage.insert(linkText, at: range.location)
            tv.setSelectedRange(NSRange(location: range.location + linkText.length, length: 0))
        }
        refresh()
    }
}

// MARK: - Self-sizing NSTextView subclass

final class MacEditorTextView: NSTextView {
    var onDeleteBackwardAtStart: (() -> Void)?
    var onPasteImage: ((Data, Int?, Int?) -> Void)?

    override var intrinsicContentSize: NSSize {
        guard let lm = layoutManager, let tc = textContainer else { return super.intrinsicContentSize }
        lm.ensureLayout(for: tc)
        let height = lm.usedRect(for: tc).height + textContainerInset.height * 2
        return NSSize(width: NSView.noIntrinsicMetric, height: max(ceil(height), 22))
    }

    override func didChangeText() {
        super.didChangeText()
        invalidateIntrinsicContentSize()
    }

    override func deleteBackward(_ sender: Any?) {
        if selectedRange().location == 0, selectedRange().length == 0 {
            onDeleteBackwardAtStart?()
            return
        }
        super.deleteBackward(sender)
    }

    override func paste(_ sender: Any?) {
        if let images = NSPasteboard.general.readObjects(forClasses: [NSImage.self], options: nil) as? [NSImage],
           let image = images.first,
           let tiff = image.tiffRepresentation,
           let rep = NSBitmapImageRep(data: tiff),
           let data = rep.representation(using: .png, properties: [:]) {
            onPasteImage?(data, rep.pixelsWide > 0 ? rep.pixelsWide : nil, rep.pixelsHigh > 0 ? rep.pixelsHigh : nil)
            return
        }
        super.paste(sender)
    }

    override func mouseDown(with event: NSEvent) {
        if let lm = layoutManager, let tc = textContainer, let storage = textStorage {
            let point = convert(event.locationInWindow, from: nil)
            let p = NSPoint(x: point.x - textContainerOrigin.x, y: point.y - textContainerOrigin.y)
            let idx = lm.characterIndex(for: p, in: tc, fractionOfDistanceBetweenInsertionPoints: nil)
            if idx < storage.length {
                let ch = (storage.string as NSString).substring(with: NSRange(location: idx, length: 1))
                if ch == "\u{2610}" || ch == "\u{2611}" {
                    let replacement = ch == "\u{2610}" ? "\u{2611}" : "\u{2610}"
                    let attrs = storage.attributes(at: idx, effectiveRange: nil)
                    storage.replaceCharacters(in: NSRange(location: idx, length: 1),
                                              with: NSAttributedString(string: replacement, attributes: attrs))
                    didChangeText()
                    return
                }
            }
        }
        super.mouseDown(with: event)
    }
}

// MARK: - Block text editor (NSViewRepresentable)

struct MacBlockTextEditor: NSViewRepresentable {
    let model: IssueEditorModel
    let blockId: UUID
    let content: NSAttributedString
    let revision: Int
    let isFocused: Bool
    let controller: MacEditorToolbarController

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSView(context: Context) -> MacEditorTextView {
        let tv = MacEditorTextView()
        tv.drawsBackground = false
        tv.textColor = MarkdownStyle.textColor
        tv.insertionPointColor = .white
        tv.font = MarkdownStyle.bodyFont
        tv.isRichText = true
        tv.allowsUndo = true
        tv.isAutomaticQuoteSubstitutionEnabled = false
        tv.isAutomaticDashSubstitutionEnabled = false
        tv.isAutomaticTextReplacementEnabled = false
        tv.isVerticallyResizable = true
        tv.isHorizontallyResizable = false
        tv.textContainerInset = NSSize(width: 0, height: 4)
        tv.textContainer?.widthTracksTextView = true
        tv.textContainer?.lineFragmentPadding = 0
        tv.typingAttributes = MarkdownStyle.baseAttributes
        tv.delegate = context.coordinator
        tv.setContentHuggingPriority(.defaultHigh, for: .vertical)

        let coord = context.coordinator
        coord.textView = tv
        coord.model = model
        coord.blockId = blockId
        coord.controller = controller
        coord.appliedRevision = revision
        tv.onDeleteBackwardAtStart = { [weak coord] in coord?.handleDeleteBackwardAtStart() }
        tv.onPasteImage = { [weak coord] data, w, h in coord?.handlePaste(data: data, width: w, height: h) }

        coord.beginProgrammaticChange()
        tv.textStorage?.setAttributedString(content)
        coord.endProgrammaticChange()
        return tv
    }

    func updateNSView(_ tv: MacEditorTextView, context: Context) {
        let coord = context.coordinator
        coord.model = model
        coord.blockId = blockId
        coord.controller = controller
        tv.onDeleteBackwardAtStart = { [weak coord] in coord?.handleDeleteBackwardAtStart() }
        tv.onPasteImage = { [weak coord] data, w, h in coord?.handlePaste(data: data, width: w, height: h) }

        if revision != coord.appliedRevision {
            coord.appliedRevision = revision
            let saved = tv.selectedRange()
            coord.beginProgrammaticChange()
            tv.textStorage?.setAttributedString(content)
            coord.endProgrammaticChange()
            let pos = min(saved.location, tv.textStorage?.length ?? 0)
            tv.setSelectedRange(NSRange(location: pos, length: 0))
            tv.invalidateIntrinsicContentSize()
        }

        if let desired = model.consumeDesiredSelection(for: blockId) {
            let pos = min(desired, tv.textStorage?.length ?? 0)
            tv.setSelectedRange(NSRange(location: pos, length: 0))
        }

        if isFocused, tv.window?.firstResponder !== tv {
            tv.window?.makeFirstResponder(tv)
            controller.textView = tv
        }
    }

    @MainActor
    final class Coordinator: NSObject, NSTextViewDelegate {
        weak var textView: MacEditorTextView?
        var model: IssueEditorModel?
        var blockId: UUID?
        var controller: MacEditorToolbarController?
        var appliedRevision = 0
        private var isProgrammaticChange = false

        func beginProgrammaticChange() { isProgrammaticChange = true }
        func endProgrammaticChange() { isProgrammaticChange = false }

        func handleDeleteBackwardAtStart() {
            guard let model, let blockId else { return }
            model.deleteImage(beforeTextBlock: blockId)
        }

        func handlePaste(data: Data, width: Int?, height: Int?) {
            model?.insertImage(
                data: data,
                filename: "pasted-\(Int(Date().timeIntervalSince1970)).png",
                contentType: "image/png",
                width: width,
                height: height
            )
        }

        func textDidBeginEditing(_ notification: Notification) {
            guard let blockId, let tv = textView else { return }
            model?.setFocused(blockId)
            controller?.textView = tv
            controller?.updateState()
        }

        func textDidEndEditing(_ notification: Notification) {
            guard let blockId else { return }
            model?.clearFocusIfMatches(blockId)
        }

        func textDidChange(_ notification: Notification) {
            guard !isProgrammaticChange, let model, let blockId, let tv = textView else { return }
            let snapshot = NSAttributedString(attributedString: tv.attributedString())
            model.updateText(id: blockId, content: snapshot)
        }

        func textViewDidChangeSelection(_ notification: Notification) {
            if !isProgrammaticChange, let model, let blockId, let tv = textView {
                model.updateSelection(blockId: blockId, range: tv.selectedRange())
            }
            controller?.updateState()
        }

        func textView(_ textView: NSTextView, shouldChangeTextIn affectedCharRange: NSRange, replacementString: String?) -> Bool {
            guard let storage = textView.textStorage, storage.length > 0 else { return true }
            let text = replacementString ?? ""
            let nsString = storage.string as NSString

            // Backspace on an empty list item → exit list mode.
            if text.isEmpty, affectedCharRange.length > 0 {
                let paraRange = nsString.safeParagraphRange(at: affectedCharRange.location)
                if let attrs = storage.attributesIfInBounds(at: paraRange.location),
                   attrs[.markdownListType] as? String != nil {
                    let paraText = nsString.substring(with: paraRange).trimmingCharacters(in: .newlines)
                    if stripListPrefix(paraText).isEmpty {
                        clearListParagraph(tv: textView, storage: storage, paraRange: paraRange)
                        return false
                    }
                }
            }

            guard text == "\n" else { return true }
            let paraRange = nsString.safeParagraphRange(at: affectedCharRange.location)
            guard let attrs = storage.attributesIfInBounds(at: paraRange.location),
                  let listType = attrs[.markdownListType] as? String else { return true }

            let paraText = nsString.substring(with: paraRange).trimmingCharacters(in: .newlines)
            if stripListPrefix(paraText).isEmpty {
                clearListParagraph(tv: textView, storage: storage, paraRange: paraRange)
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
            storage.replaceCharacters(in: affectedCharRange, with: NSAttributedString(string: "\n\(prefix)", attributes: newAttrs))
            textView.setSelectedRange(NSRange(location: affectedCharRange.location + 1 + prefix.count, length: 0))
            textView.typingAttributes = newAttrs
            textView.didChangeText()
            return false
        }

        private func stripListPrefix(_ text: String) -> String {
            text.replacingOccurrences(
                of: #"^(\d+\.\s|[\u{2022}\u{2610}\u{2611}]\s?)"#,
                with: "", options: .regularExpression
            ).trimmingCharacters(in: .whitespaces)
        }

        private func clearListParagraph(tv: NSTextView, storage: NSTextStorage, paraRange: NSRange) {
            if paraRange.length > 0, NSMaxRange(paraRange) <= storage.length {
                storage.replaceCharacters(in: paraRange, with: "")
            }
            let style = NSMutableParagraphStyle()
            style.lineSpacing = 4
            var typing = MarkdownStyle.baseAttributes
            typing[.paragraphStyle] = style
            tv.typingAttributes = typing
            tv.didChangeText()
        }
    }
}

// MARK: - Block image view

struct MacBlockImageView: View {
    let model: IssueEditorModel
    let blockId: UUID
    let url: String
    let baseURL: URL?
    let accountId: String
    let httpClient: HTTPClient?
    let pendingImages: [String: PendingImage]
    var onDelete: () -> Void
    var onTapBelow: () -> Void

    @State private var image: NSImage?
    @State private var failed = false

    private var uploadState: ImageUploadState { model.uploadState(for: blockId) }

    private var aspectRatio: CGFloat {
        if let image, image.size.height > 0 { return image.size.width / image.size.height }
        if let p = pendingImages[url], let w = p.width, let h = p.height, h > 0 { return CGFloat(w) / CGFloat(h) }
        return 4.0 / 3.0
    }

    var body: some View {
        VStack(spacing: 0) {
            ZStack(alignment: .topTrailing) {
                Group {
                    if let image {
                        Image(nsImage: image).resizable().aspectRatio(contentMode: .fit)
                    } else if failed {
                        RoundedRectangle(cornerRadius: 8).fill(Color.white.opacity(0.06))
                            .overlay { Image(systemName: "exclamationmark.triangle").foregroundStyle(.secondary) }
                    } else {
                        RoundedRectangle(cornerRadius: 8).fill(Color.white.opacity(0.06))
                            .overlay { ProgressView().controlSize(.small) }
                    }
                }
                .frame(maxWidth: .infinity)
                .aspectRatio(aspectRatio, contentMode: .fit)
                .clipShape(RoundedRectangle(cornerRadius: 8))
                .overlay(alignment: .bottomLeading) {
                    if uploadState == .uploading {
                        HStack(spacing: 6) {
                            ProgressView().controlSize(.small)
                            Text("Uploading…").font(.caption)
                        }
                        .padding(.horizontal, 8).padding(.vertical, 4)
                        .background(.black.opacity(0.45), in: Capsule())
                        .padding(8)
                    }
                }

                Button(action: onDelete) {
                    Image(systemName: "xmark.circle.fill").font(.system(size: 18))
                        .symbolRenderingMode(.palette)
                        .foregroundStyle(.white.opacity(0.85), .black.opacity(0.5))
                }
                .buttonStyle(.plain)
                .padding(6)
            }
            .padding(.vertical, 4)

            Color.clear.frame(height: 16).contentShape(Rectangle()).onTapGesture { onTapBelow() }
        }
        .task(id: url) { await load() }
    }

    private func load() async {
        failed = false
        let loader = MacAttachmentImageLoader(baseURL: baseURL, accountId: accountId, httpClient: httpClient, pendingImages: pendingImages)
        do {
            image = try await loader.load(url)
        } catch {
            if image == nil { failed = true }
        }
    }
}

// MARK: - Toolbar bar (SwiftUI)

struct MacMarkdownToolbarView: View {
    let controller: MacEditorToolbarController

    var body: some View {
        HStack(spacing: 2) {
            button("photo") { controller.onPickImage?() }
            divider
            button("textformat.size", controller.state.heading) { controller.toggleHeading() }
            button("bold", controller.state.bold) { controller.toggleBold() }
            button("italic", controller.state.italic) { controller.toggleItalic() }
            button("strikethrough", controller.state.strike) { controller.toggleStrikethrough() }
            divider
            button("list.bullet", controller.state.bullet) { controller.toggleBulletList() }
            button("list.number", controller.state.ordered) { controller.toggleOrderedList() }
            button("checklist", controller.state.checklist) { controller.toggleChecklist() }
            button("chevron.left.forwardslash.chevron.right", controller.state.code) { controller.toggleCode() }
            button("text.quote", controller.state.quote) { controller.toggleBlockquote() }
            divider
            button("link") { controller.onInsertLink?() }
            Spacer()
        }
        .padding(.horizontal, 6).padding(.vertical, 4)
        .background(Color.white.opacity(0.05))
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    private var divider: some View {
        Rectangle().fill(Color.white.opacity(0.12)).frame(width: 1, height: 16).padding(.horizontal, 3)
    }

    private func button(_ symbol: String, _ active: Bool = false, _ action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 13, weight: .medium))
                .frame(width: 26, height: 24)
                .foregroundStyle(active ? AnyShapeStyle(StatusColor.inProgress) : AnyShapeStyle(.secondary))
        }
        .buttonStyle(.borderless)
    }
}

// MARK: - Editor container

struct MacMarkdownEditor: View {
    let model: IssueEditorModel
    var placeholder = "Add description…"
    var baseURL: URL?
    var accountId = ""
    var httpClient: HTTPClient?
    var mentionMembers: [MentionMember] = []

    @State private var controller = MacEditorToolbarController()
    @State private var showLinkPrompt = false
    @State private var linkText = ""

    var body: some View {
        VStack(spacing: 6) {
            MacMarkdownToolbarView(controller: controller)
            if !model.mentionCandidates.isEmpty { mentionBar }
            // No inner ScrollView: each block self-sizes, so the editor grows with
            // its content and the surrounding page scroll handles overflow (one
            // scroll for the whole issue, not a nested editor scroll).
            VStack(alignment: .leading, spacing: 0) {
                ForEach(model.blocks) { block in
                    switch block {
                    case .text(let id, let content):
                        ZStack(alignment: .topLeading) {
                            MacBlockTextEditor(
                                model: model, blockId: id, content: content,
                                revision: model.revision(for: id),
                                isFocused: model.focusedBlockId == id,
                                controller: controller
                            )
                            if isSolePlaceholder(id) {
                                Text(placeholder).foregroundStyle(.tertiary).padding(.top, 4).allowsHitTesting(false)
                            }
                        }
                    case .image(let id, let url, let alt):
                        MacBlockImageView(
                            model: model, blockId: id, url: url, baseURL: baseURL,
                            accountId: accountId, httpClient: httpClient, pendingImages: model.pendingImages,
                            onDelete: { model.deleteImageBlock(id: id) },
                            onTapBelow: { focusBlock(after: id) }
                        )
                        .id(id)
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .onAppear {
            controller.onPickImage = { pickImage() }
            controller.onInsertLink = { linkText = ""; showLinkPrompt = true }
            model.mentionMembers = mentionMembers
        }
        .onChange(of: mentionMembers) { _, newValue in model.mentionMembers = newValue }
        .sheet(isPresented: $showLinkPrompt) {
            VStack(alignment: .leading, spacing: 12) {
                Text("Add Link").font(.headline)
                TextField("https://", text: $linkText).textFieldStyle(.roundedBorder).frame(width: 280)
                HStack {
                    Spacer()
                    Button("Cancel") { showLinkPrompt = false }
                    Button("Add") { controller.applyLink(linkText); showLinkPrompt = false }
                        .buttonStyle(.borderedProminent)
                        .tint(Accent.indigo)
                }
            }
            .padding(20)
        }
    }

    // @-mention candidate bar. Tapping inserts the canonical `@email` token via
    // the shared model (which keeps the text view first responder).
    private var mentionBar: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(model.mentionCandidates) { member in
                    Button {
                        model.applyMention(member)
                    } label: {
                        VStack(alignment: .leading, spacing: 0) {
                            Text(member.name).font(.caption.weight(.medium))
                            Text(member.email).font(.caption2).foregroundStyle(.secondary)
                        }
                        .padding(.horizontal, 10).padding(.vertical, 4)
                        .background(Color.primary.opacity(0.08), in: Capsule())
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.vertical, 2)
        }
        .frame(maxHeight: 44)
    }

    private func isSolePlaceholder(_ id: UUID) -> Bool {
        guard model.blocks.count == 1, case .text(let bid, let content) = model.blocks[0] else { return false }
        return bid == id && content.length == 0
    }

    private func focusBlock(after id: UUID) {
        guard let idx = model.blocks.firstIndex(where: { $0.id == id }), idx + 1 < model.blocks.count else { return }
        model.setFocused(model.blocks[idx + 1].id)
    }

    private func pickImage() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.image]
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url, let data = try? Data(contentsOf: url) else { return }
        let ext = url.pathExtension.lowercased()
        let contentType = ext == "png" ? "image/png" : (ext == "gif" ? "image/gif" : "image/jpeg")
        var w: Int?, h: Int?
        if let rep = NSImage(data: data)?.representations.first {
            w = rep.pixelsWide > 0 ? rep.pixelsWide : nil
            h = rep.pixelsHigh > 0 ? rep.pixelsHigh : nil
        }
        model.insertImage(data: data, filename: url.lastPathComponent, contentType: contentType, width: w, height: h)
    }
}
