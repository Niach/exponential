import cmark_gfm
import cmark_gfm_extensions
import Foundation
import os
#if os(iOS)
import UIKit
#elseif os(macOS)
import AppKit
#endif

private let log = Logger(subsystem: "com.exponential", category: "MarkdownConversion")

public enum ContentBlock: Identifiable, Equatable {
    case text(id: UUID, attributedContent: NSAttributedString)
    case image(id: UUID, url: String, alt: String)

    public var id: UUID {
        switch self {
        case .text(let id, _): return id
        case .image(let id, _, _): return id
        }
    }

    public static func normalize(_ blocks: inout [ContentBlock]) {
        if blocks.isEmpty {
            blocks = [.text(id: UUID(), attributedContent: NSAttributedString())]
            return
        }
        if case .image = blocks.first {
            blocks.insert(.text(id: UUID(), attributedContent: NSAttributedString()), at: 0)
        }
        if case .image = blocks.last {
            blocks.append(.text(id: UUID(), attributedContent: NSAttributedString()))
        }
        var i = 1
        while i < blocks.count {
            if case .image = blocks[i], case .image = blocks[i - 1] {
                blocks.insert(.text(id: UUID(), attributedContent: NSAttributedString()), at: i)
            }
            i += 1
        }
    }
}

public enum MarkdownConversion {

    // MARK: - NSAttributedString → Markdown

    public static func attributedStringToMarkdown(_ attrStr: NSAttributedString) -> String {
        let fullText = attrStr.string
        guard !fullText.isEmpty else { return "" }

        var markdown = ""
        var inCodeBlock = false
        var codeBlockLang: String?

        let paragraphs = splitIntoParagraphs(attrStr)
        log.debug("attributedStringToMarkdown: \(paragraphs.count) paragraphs from \(attrStr.length) chars")

        for (i, para) in paragraphs.enumerated() {
            guard para.location < attrStr.length, NSMaxRange(para) <= attrStr.length else {
                log.error("paragraph out of bounds: \(para.location)+\(para.length) vs \(attrStr.length)")
                continue
            }
            let paraStr = attrStr.attributedSubstring(from: para)
            let attrs = attrStr.attributes(at: para.location, effectiveRange: nil)

            if let isCode = attrs[.markdownCodeBlock] as? Bool, isCode {
                if !inCodeBlock {
                    if i > 0 { markdown += "\n" }
                    codeBlockLang = attrs[.markdownCodeBlockLang] as? String
                    markdown += "```\(codeBlockLang ?? "")\n"
                    inCodeBlock = true
                }
                markdown += paraStr.string
                if !paraStr.string.hasSuffix("\n") { markdown += "\n" }
                continue
            }

            if inCodeBlock {
                markdown += "```\n"
                inCodeBlock = false
                codeBlockLang = nil
            }

            if i > 0 {
                if let prevAttrs = i > 0 ? attrStr.attributes(at: paragraphs[i - 1].location, effectiveRange: nil) : nil,
                   (prevAttrs[.markdownListType] as? String) != nil,
                   (attrs[.markdownListType] as? String) != nil {
                    markdown += "\n"
                } else {
                    markdown += "\n\n"
                }
            }

            if let headingLevel = attrs[.markdownHeadingLevel] as? Int, headingLevel > 0 {
                markdown += String(repeating: "#", count: headingLevel) + " "
                markdown += extractInlineMarkdown(from: paraStr, isHeading: true)
                continue
            }

            if let isBlockquote = attrs[.markdownBlockquote] as? Bool, isBlockquote {
                markdown += "> "
                markdown += extractInlineMarkdown(from: paraStr, isHeading: false)
                continue
            }

            if let listType = attrs[.markdownListType] as? String {
                let depth = (attrs[.markdownListDepth] as? Int) ?? 0
                let indent = String(repeating: "  ", count: depth)
                if listType == "ordered" {
                    let index = (attrs[.markdownListItemIndex] as? Int) ?? 1
                    markdown += "\(indent)\(index). "
                } else if listType == "checklist" {
                    let checked = paraStr.string.hasPrefix("\u{2611}")
                    markdown += checked ? "\(indent)- [x] " : "\(indent)- [ ] "
                } else {
                    markdown += "\(indent)- "
                }
                markdown += extractInlineMarkdown(from: paraStr, isHeading: false, stripListPrefix: true)
                continue
            }

            markdown += extractInlineMarkdown(from: paraStr, isHeading: false)
        }

        if inCodeBlock {
            markdown += "```"
        }

        return markdown.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    // MARK: - Markdown → Blocks

    public static func markdownToBlocks(_ markdown: String, baseURL: URL? = nil) -> [ContentBlock] {
        cmark_gfm_core_extensions_ensure_registered()

        guard let parser = cmark_parser_new(CMARK_OPT_UNSAFE) else {
            return [.text(id: UUID(), attributedContent: NSAttributedString(string: markdown, attributes: MarkdownStyle.baseAttributes))]
        }
        defer { cmark_parser_free(parser) }

        for name in ["strikethrough", "table", "autolink"] {
            if let ext = cmark_find_syntax_extension(name) {
                cmark_parser_attach_syntax_extension(parser, ext)
            }
        }

        markdown.withCString { ptr in
            cmark_parser_feed(parser, ptr, strlen(ptr))
        }

        guard let doc = cmark_parser_finish(parser) else {
            return [.text(id: UUID(), attributedContent: NSAttributedString(string: markdown, attributes: MarkdownStyle.baseAttributes))]
        }
        defer { cmark_node_free(doc) }

        let collector = BlockCollector(baseURL: baseURL)
        var context = RenderContext(baseURL: baseURL)
        renderNodeToBlocks(doc, collector: collector, context: &context)
        return collector.finalize()
    }

    // MARK: - Blocks → Markdown

    public static func blocksToMarkdown(_ blocks: [ContentBlock]) -> String {
        var parts: [String] = []
        for block in blocks {
            switch block {
            case .text(_, let content):
                let md = attributedStringToMarkdown(content)
                if !md.isEmpty { parts.append(md) }
            case .image(_, let url, let alt):
                parts.append("![\(alt)](\(url))")
            }
        }
        return parts.joined(separator: "\n\n")
    }
}

// MARK: - AST Rendering

private struct StyleFrame {
    var font: PlatformFont
    var foregroundColor: PlatformColor
    var extraAttributes: [NSAttributedString.Key: Any]
}

private struct ListContext {
    let ordered: Bool
    var itemIndex: Int
    let depth: Int
}

private struct RenderContext {
    var baseURL: URL?
    var styleStack: [StyleFrame] = [StyleFrame(
        font: MarkdownStyle.bodyFont,
        foregroundColor: MarkdownStyle.textColor,
        extraAttributes: [:]
    )]
    var listStack: [ListContext] = []
    var headingLevel: Int = 0
    var inCodeBlock = false
    var inBlockquote = false
    var needsBlockSeparator = false

    var currentFont: PlatformFont {
        styleStack.last?.font ?? MarkdownStyle.bodyFont
    }

    var currentColor: PlatformColor {
        styleStack.last?.foregroundColor ?? MarkdownStyle.textColor
    }

    var currentExtraAttributes: [NSAttributedString.Key: Any] {
        var merged: [NSAttributedString.Key: Any] = [:]
        for frame in styleStack {
            merged.merge(frame.extraAttributes) { _, new in new }
        }
        return merged
    }

    mutating func pushStyle(font: PlatformFont? = nil, color: PlatformColor? = nil, extra: [NSAttributedString.Key: Any] = [:]) {
        styleStack.append(StyleFrame(
            font: font ?? currentFont,
            foregroundColor: color ?? currentColor,
            extraAttributes: extra
        ))
    }

    mutating func popStyle() {
        if styleStack.count > 1 { styleStack.removeLast() }
    }

    func makeAttributes() -> [NSAttributedString.Key: Any] {
        var attrs: [NSAttributedString.Key: Any] = [
            .font: currentFont,
            .foregroundColor: currentColor,
        ]
        attrs.merge(currentExtraAttributes) { _, new in new }
        return attrs
    }
}

// MARK: - Block-Aware AST Rendering

private class BlockCollector {
    var blocks: [ContentBlock] = []
    var currentText = NSMutableAttributedString()
    let baseURL: URL?

    init(baseURL: URL?) { self.baseURL = baseURL }

    func flushText() {
        let content = NSMutableAttributedString(attributedString: currentText)
        if content.length > 0, content.string.hasSuffix("\n") {
            content.deleteCharacters(in: NSRange(location: content.length - 1, length: 1))
        }
        blocks.append(.text(id: UUID(), attributedContent: content))
        currentText = NSMutableAttributedString()
    }

    func emitImage(url: String, alt: String) {
        flushText()
        blocks.append(.image(id: UUID(), url: url, alt: alt))
    }

    func finalize() -> [ContentBlock] {
        flushText()
        ContentBlock.normalize(&blocks)
        return blocks
    }
}

private func renderNodeToBlocks(_ node: UnsafeMutablePointer<cmark_node>, collector: BlockCollector, context: inout RenderContext) {
    let type = cmark_node_get_type(node)

    switch type {
    case CMARK_NODE_DOCUMENT:
        renderChildrenToBlocks(node, collector: collector, context: &context)

    case CMARK_NODE_PARAGRAPH:
        appendBlockSeparatorToCollector(collector: collector, context: &context)
        if context.inBlockquote {
            context.pushStyle(color: MarkdownStyle.blockquoteTextColor, extra: [.markdownBlockquote: true])
        }
        renderChildrenToBlocks(node, collector: collector, context: &context)
        if context.inBlockquote { context.popStyle() }
        context.needsBlockSeparator = true

    case CMARK_NODE_HEADING:
        appendBlockSeparatorToCollector(collector: collector, context: &context)
        let level = Int(cmark_node_get_heading_level(node))
        context.headingLevel = level
        context.pushStyle(font: MarkdownStyle.headingFont(level: level), extra: [.markdownHeadingLevel: level])
        renderChildrenToBlocks(node, collector: collector, context: &context)
        context.popStyle()
        context.headingLevel = 0
        context.needsBlockSeparator = true

    case CMARK_NODE_TEXT:
        let literal = String(cString: cmark_node_get_literal(node))
        collector.currentText.append(NSAttributedString(string: literal, attributes: context.makeAttributes()))

    case CMARK_NODE_SOFTBREAK:
        collector.currentText.append(NSAttributedString(string: " ", attributes: context.makeAttributes()))

    case CMARK_NODE_LINEBREAK:
        collector.currentText.append(NSAttributedString(string: "\n", attributes: context.makeAttributes()))

    case CMARK_NODE_STRONG:
        let bold = expBoldFont(context.currentFont)
        context.pushStyle(font: bold)
        renderChildrenToBlocks(node, collector: collector, context: &context)
        context.popStyle()

    case CMARK_NODE_EMPH:
        let italic = expItalicFont(context.currentFont)
        context.pushStyle(font: italic)
        renderChildrenToBlocks(node, collector: collector, context: &context)
        context.popStyle()

    case CMARK_NODE_CODE:
        let literal = String(cString: cmark_node_get_literal(node))
        var attrs = context.makeAttributes()
        attrs[.font] = MarkdownStyle.monospaceFont
        attrs[.backgroundColor] = MarkdownStyle.codeBackground
        attrs[.markdownInlineCode] = true
        collector.currentText.append(NSAttributedString(string: literal, attributes: attrs))

    case CMARK_NODE_CODE_BLOCK:
        appendBlockSeparatorToCollector(collector: collector, context: &context)
        let literal = String(cString: cmark_node_get_literal(node))
        let lang = cmark_node_get_fence_info(node).flatMap { String(cString: $0) }
        var attrs = context.makeAttributes()
        attrs[.font] = MarkdownStyle.monospaceFont
        attrs[.backgroundColor] = MarkdownStyle.codeBlockBackground
        attrs[.markdownCodeBlock] = true
        if let lang, !lang.isEmpty { attrs[.markdownCodeBlockLang] = lang }
        let text = literal.hasSuffix("\n") ? String(literal.dropLast()) : literal
        collector.currentText.append(NSAttributedString(string: text, attributes: attrs))
        context.needsBlockSeparator = true

    case CMARK_NODE_LINK:
        let urlStr = cmark_node_get_url(node).flatMap { String(cString: $0) } ?? ""
        let resolved = resolveURL(urlStr, baseURL: context.baseURL)
        var linkExtra: [NSAttributedString.Key: Any] = [:]
        if let resolved { linkExtra[.link] = resolved }
        context.pushStyle(color: MarkdownStyle.linkColor, extra: linkExtra)
        renderChildrenToBlocks(node, collector: collector, context: &context)
        context.popStyle()

    case CMARK_NODE_IMAGE:
        let urlStr = cmark_node_get_url(node).flatMap { String(cString: $0) } ?? ""
        let alt = collectText(from: node)
        collector.emitImage(url: urlStr, alt: alt)
        context.needsBlockSeparator = false

    case CMARK_NODE_LIST:
        let ordered = cmark_node_get_list_type(node) == CMARK_ORDERED_LIST
        let start = Int(cmark_node_get_list_start(node))
        let depth = context.listStack.count
        if depth == 0 { appendBlockSeparatorToCollector(collector: collector, context: &context) }
        context.listStack.append(ListContext(ordered: ordered, itemIndex: start, depth: depth))
        renderChildrenToBlocks(node, collector: collector, context: &context)
        context.listStack.removeLast()
        if context.listStack.isEmpty { context.needsBlockSeparator = true }

    case CMARK_NODE_ITEM:
        if collector.currentText.length > 0, !collector.currentText.string.hasSuffix("\n") {
            collector.currentText.append(NSAttributedString(string: "\n", attributes: context.makeAttributes()))
        }
        let depth = context.listStack.last?.depth ?? 0
        let ordered = context.listStack.last?.ordered ?? false
        let index = context.listStack.last?.itemIndex ?? 1
        let task = taskItemState(node)
        let isTaskItem = task.isTask
        let isChecked = task.checked
        if isTaskItem { stripTaskMarker(node) }
        let listType: String
        let prefix: String
        if isTaskItem {
            listType = "checklist"
            prefix = isChecked ? "\u{2611} " : "\u{2610} "
        } else if ordered {
            listType = "ordered"
            prefix = "\(index). "
        } else {
            listType = "bullet"
            prefix = "\u{2022} "
        }
        if var last = context.listStack.last {
            last.itemIndex += 1
            context.listStack[context.listStack.count - 1] = last
        }
        let paragraphStyle = NSMutableParagraphStyle()
        paragraphStyle.lineSpacing = 4
        let indent: CGFloat = CGFloat(depth) * 20 + 24
        paragraphStyle.headIndent = indent
        paragraphStyle.firstLineHeadIndent = CGFloat(depth) * 20
        var prefixAttrs = context.makeAttributes()
        prefixAttrs[.paragraphStyle] = paragraphStyle
        prefixAttrs[.markdownListType] = listType
        prefixAttrs[.markdownListItemIndex] = ordered ? index : 0
        prefixAttrs[.markdownListDepth] = depth
        collector.currentText.append(NSAttributedString(string: prefix, attributes: prefixAttrs))
        context.pushStyle(extra: [
            .paragraphStyle: paragraphStyle,
            .markdownListType: listType,
            .markdownListItemIndex: ordered ? index : 0,
            .markdownListDepth: depth,
        ])
        renderChildrenToBlocks(node, collector: collector, context: &context)
        context.popStyle()

    case CMARK_NODE_BLOCK_QUOTE:
        appendBlockSeparatorToCollector(collector: collector, context: &context)
        context.inBlockquote = true
        renderChildrenToBlocks(node, collector: collector, context: &context)
        context.inBlockquote = false
        context.needsBlockSeparator = true

    case CMARK_NODE_THEMATIC_BREAK:
        appendBlockSeparatorToCollector(collector: collector, context: &context)
        var attrs = context.makeAttributes()
        attrs[.foregroundColor] = PlatformColor.white.withAlphaComponent(0.3)
        collector.currentText.append(NSAttributedString(string: "───", attributes: attrs))
        context.needsBlockSeparator = true

    case CMARK_NODE_HTML_BLOCK:
        appendBlockSeparatorToCollector(collector: collector, context: &context)
        if let literal = cmark_node_get_literal(node) {
            let text = String(cString: literal)
            collector.currentText.append(NSAttributedString(string: text.trimmingCharacters(in: .whitespacesAndNewlines), attributes: context.makeAttributes()))
        }
        context.needsBlockSeparator = true

    case CMARK_NODE_HTML_INLINE:
        if let literal = cmark_node_get_literal(node) {
            let text = String(cString: literal)
            collector.currentText.append(NSAttributedString(string: text, attributes: context.makeAttributes()))
        }

    default:
        if cmark_node_get_type_string(node) != nil {
            let typeStr = String(cString: cmark_node_get_type_string(node))
            if typeStr == "strikethrough" {
                context.pushStyle(extra: [
                    .strikethroughStyle: NSUnderlineStyle.single.rawValue,
                    .markdownStrikethrough: true,
                ])
                renderChildrenToBlocks(node, collector: collector, context: &context)
                context.popStyle()
                return
            }
        }
        renderChildrenToBlocks(node, collector: collector, context: &context)
    }
}

private func renderChildrenToBlocks(_ node: UnsafeMutablePointer<cmark_node>, collector: BlockCollector, context: inout RenderContext) {
    var child = cmark_node_first_child(node)
    while let c = child {
        renderNodeToBlocks(c, collector: collector, context: &context)
        child = cmark_node_next(c)
    }
}

private func appendBlockSeparatorToCollector(collector: BlockCollector, context: inout RenderContext) {
    guard context.needsBlockSeparator, collector.currentText.length > 0 else {
        context.needsBlockSeparator = false
        return
    }
    collector.currentText.append(NSAttributedString(string: "\n", attributes: MarkdownStyle.baseAttributes))
    context.needsBlockSeparator = false
}

// MARK: - URL Helpers

private func resolveURL(_ urlStr: String, baseURL: URL?) -> URL? {
    if let url = URL(string: urlStr), url.scheme != nil { return url }
    guard let baseURL else { return URL(string: urlStr) }
    let base = baseURL.absoluteString.hasSuffix("/")
        ? String(baseURL.absoluteString.dropLast())
        : baseURL.absoluteString
    return URL(string: base + urlStr)
}

// Task-list detection WITHOUT cmark's tasklist extension. We parse `- [ ]`
// as a plain bullet so the `[ ]`/`[x]` marker stays in the literal and inspect
// it here — the extension consumes the marker and then can't distinguish an
// UNCHECKED task item from a regular bullet (its checked-getter returns false
// for both), which made unchecked checkboxes round-trip as bullets.
private func firstTextNode(under item: UnsafeMutablePointer<cmark_node>) -> UnsafeMutablePointer<cmark_node>? {
    guard let para = cmark_node_first_child(item),
          cmark_node_get_type(para) == CMARK_NODE_PARAGRAPH,
          let text = cmark_node_first_child(para),
          cmark_node_get_type(text) == CMARK_NODE_TEXT else { return nil }
    return text
}

private func taskItemState(_ node: UnsafeMutablePointer<cmark_node>) -> (isTask: Bool, checked: Bool) {
    guard let textNode = firstTextNode(under: node),
          let literal = cmark_node_get_literal(textNode) else { return (false, false) }
    let text = String(cString: literal)
    if text.hasPrefix("[ ] ") { return (true, false) }
    if text.hasPrefix("[x] ") || text.hasPrefix("[X] ") { return (true, true) }
    return (false, false)
}

private func stripTaskMarker(_ node: UnsafeMutablePointer<cmark_node>) {
    guard let textNode = firstTextNode(under: node),
          let literal = cmark_node_get_literal(textNode) else { return }
    var text = String(cString: literal)
    for marker in ["[ ] ", "[x] ", "[X] "] where text.hasPrefix(marker) {
        text.removeFirst(marker.count)
        break
    }
    text.withCString { _ = cmark_node_set_literal(textNode, $0) }
}

private func collectText(from node: UnsafeMutablePointer<cmark_node>) -> String {
    var text = ""
    var child = cmark_node_first_child(node)
    while let c = child {
        if cmark_node_get_type(c) == CMARK_NODE_TEXT, let literal = cmark_node_get_literal(c) {
            text += String(cString: literal)
        }
        child = cmark_node_next(c)
    }
    return text
}

// MARK: - Reverse Conversion Helpers

private func splitIntoParagraphs(_ attrStr: NSAttributedString) -> [NSRange] {
    let string = attrStr.string as NSString
    var ranges: [NSRange] = []
    var start = 0
    let length = string.length

    while start < length {
        let lineRange = string.lineRange(for: NSRange(location: start, length: 0))
        var end = NSMaxRange(lineRange)
        while end > lineRange.location && (string.character(at: end - 1) == 0x0A || string.character(at: end - 1) == 0x0D) {
            end -= 1
        }
        let trimmedRange = NSRange(location: lineRange.location, length: end - lineRange.location)
        if trimmedRange.length > 0 {
            ranges.append(trimmedRange)
        }
        start = NSMaxRange(lineRange)
    }

    return ranges
}

private func extractInlineMarkdown(from attrStr: NSAttributedString, isHeading: Bool, stripListPrefix: Bool = false) -> String {
    var markdown = ""
    let fullRange = NSRange(location: 0, length: attrStr.length)
    var string = attrStr.string
    var effectiveRange = fullRange

    if stripListPrefix {
        let prefixPattern = #"^[\u{2022}\u{2610}\u{2611}]\s?"#
        if let range = string.range(of: prefixPattern, options: .regularExpression) {
            let prefixLen = string.distance(from: string.startIndex, to: range.upperBound)
            string = String(string[range.upperBound...])
            effectiveRange = NSRange(location: prefixLen, length: attrStr.length - prefixLen)
            if effectiveRange.length <= 0 { return "" }
        }
    }

    attrStr.enumerateAttributes(in: effectiveRange, options: []) { attrs, range, _ in
        let substring = (attrStr.string as NSString).substring(with: range)

        if let imageURL = attrs[.markdownImageURL] as? String {
            let alt = (attrs[.markdownImageAlt] as? String) ?? ""
            markdown += "![\(alt)](\(imageURL))"
            return
        }

        if attrs[.attachment] is NSTextAttachment {
            return
        }

        if let isCode = attrs[.markdownInlineCode] as? Bool, isCode {
            markdown += "`\(substring)`"
            return
        }

        if let url = attrs[.link] as? URL {
            markdown += "[\(substring)](\(url.absoluteString))"
            return
        }
        if let url = attrs[.link] as? String {
            markdown += "[\(substring)](\(url))"
            return
        }

        let font = attrs[.font] as? PlatformFont
        let isBold = expFontHasBold(font) && !isHeading
        let isItalic = expFontHasItalic(font)
        let isStrike = attrs[.markdownStrikethrough] as? Bool == true

        var text = substring
        if isStrike { text = "~~\(text)~~" }
        if isBold && isItalic { text = "***\(text)***" }
        else if isBold { text = "**\(text)**" }
        else if isItalic { text = "*\(text)*" }

        markdown += text
    }

    return markdown
}
