import cmark_gfm
import cmark_gfm_extensions
import Foundation
import os
import UIKit

private let log = Logger(subsystem: "com.exponential", category: "MarkdownConversion")

enum MarkdownConversion {

    // MARK: - Markdown → NSAttributedString

    static func markdownToAttributedString(_ markdown: String, baseURL: URL? = nil) -> NSAttributedString {
        cmark_gfm_core_extensions_ensure_registered()

        guard let parser = cmark_parser_new(CMARK_OPT_UNSAFE) else {
            return NSAttributedString(string: markdown, attributes: MarkdownStyle.baseAttributes)
        }
        defer { cmark_parser_free(parser) }

        for name in ["strikethrough", "table", "autolink", "tasklist"] {
            if let ext = cmark_find_syntax_extension(name) {
                cmark_parser_attach_syntax_extension(parser, ext)
            }
        }

        markdown.withCString { ptr in
            cmark_parser_feed(parser, ptr, strlen(ptr))
        }

        guard let doc = cmark_parser_finish(parser) else {
            return NSAttributedString(string: markdown, attributes: MarkdownStyle.baseAttributes)
        }
        defer { cmark_node_free(doc) }

        let result = NSMutableAttributedString()
        var context = RenderContext(baseURL: baseURL)
        renderNode(doc, into: result, context: &context)

        if result.length > 0, result.string.hasSuffix("\n") {
            result.deleteCharacters(in: NSRange(location: result.length - 1, length: 1))
        }

        return result
    }

    // MARK: - NSAttributedString → Markdown

    static func attributedStringToMarkdown(_ attrStr: NSAttributedString) -> String {
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

    // MARK: - Markdown → HTML (kept for other callers)

    static func markdownToHTML(_ markdown: String, baseURL: URL? = nil) -> String {
        cmark_gfm_core_extensions_ensure_registered()

        guard let parser = cmark_parser_new(CMARK_OPT_UNSAFE) else { return escapeHTML(markdown) }
        defer { cmark_parser_free(parser) }

        for name in ["strikethrough", "table", "autolink", "tasklist"] {
            if let ext = cmark_find_syntax_extension(name) {
                cmark_parser_attach_syntax_extension(parser, ext)
            }
        }

        markdown.withCString { ptr in
            cmark_parser_feed(parser, ptr, strlen(ptr))
        }

        guard let doc = cmark_parser_finish(parser) else { return escapeHTML(markdown) }
        defer { cmark_node_free(doc) }

        let extensions = cmark_parser_get_syntax_extensions(parser)
        guard let cHTML = cmark_render_html(doc, CMARK_OPT_UNSAFE, extensions) else { return "" }
        defer { free(cHTML) }

        var html = String(cString: cHTML)
        if let baseURL {
            html = resolveRelativeURLs(in: html, baseURL: baseURL)
        }
        return html
    }
}

// MARK: - AST Rendering

private struct StyleFrame {
    var font: UIFont
    var foregroundColor: UIColor
    var extraAttributes: [NSAttributedString.Key: Any]
}

private struct ListContext {
    let ordered: Bool
    var itemIndex: Int
    let depth: Int
}

private struct ImageInfo {
    let url: String
    let alt: String
    let attachment: NSTextAttachment
    let range: NSRange
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
    var imageInfos: [ImageInfo] = []

    var currentFont: UIFont {
        styleStack.last?.font ?? MarkdownStyle.bodyFont
    }

    var currentColor: UIColor {
        styleStack.last?.foregroundColor ?? MarkdownStyle.textColor
    }

    var currentExtraAttributes: [NSAttributedString.Key: Any] {
        var merged: [NSAttributedString.Key: Any] = [:]
        for frame in styleStack {
            merged.merge(frame.extraAttributes) { _, new in new }
        }
        return merged
    }

    mutating func pushStyle(font: UIFont? = nil, color: UIColor? = nil, extra: [NSAttributedString.Key: Any] = [:]) {
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

private func renderNode(_ node: UnsafeMutablePointer<cmark_node>, into result: NSMutableAttributedString, context: inout RenderContext) {
    let type = cmark_node_get_type(node)

    switch type {
    case CMARK_NODE_DOCUMENT:
        renderChildren(node, into: result, context: &context)

    case CMARK_NODE_PARAGRAPH:
        appendBlockSeparator(to: result, context: &context)
        if context.inBlockquote {
            context.pushStyle(
                color: MarkdownStyle.blockquoteTextColor,
                extra: [.markdownBlockquote: true]
            )
        }
        renderChildren(node, into: result, context: &context)
        if context.inBlockquote {
            context.popStyle()
        }
        context.needsBlockSeparator = true

    case CMARK_NODE_HEADING:
        appendBlockSeparator(to: result, context: &context)
        let level = Int(cmark_node_get_heading_level(node))
        context.headingLevel = level
        context.pushStyle(
            font: MarkdownStyle.headingFont(level: level),
            extra: [.markdownHeadingLevel: level]
        )
        renderChildren(node, into: result, context: &context)
        context.popStyle()
        context.headingLevel = 0
        context.needsBlockSeparator = true

    case CMARK_NODE_TEXT:
        let literal = String(cString: cmark_node_get_literal(node))
        result.append(NSAttributedString(string: literal, attributes: context.makeAttributes()))

    case CMARK_NODE_SOFTBREAK:
        result.append(NSAttributedString(string: " ", attributes: context.makeAttributes()))

    case CMARK_NODE_LINEBREAK:
        result.append(NSAttributedString(string: "\n", attributes: context.makeAttributes()))

    case CMARK_NODE_STRONG:
        let bold = addBoldTrait(to: context.currentFont)
        context.pushStyle(font: bold)
        renderChildren(node, into: result, context: &context)
        context.popStyle()

    case CMARK_NODE_EMPH:
        let italic = addItalicTrait(to: context.currentFont)
        context.pushStyle(font: italic)
        renderChildren(node, into: result, context: &context)
        context.popStyle()

    case CMARK_NODE_CODE:
        let literal = String(cString: cmark_node_get_literal(node))
        var attrs = context.makeAttributes()
        attrs[.font] = MarkdownStyle.monospaceFont
        attrs[.backgroundColor] = MarkdownStyle.codeBackground
        attrs[.markdownInlineCode] = true
        result.append(NSAttributedString(string: literal, attributes: attrs))

    case CMARK_NODE_CODE_BLOCK:
        appendBlockSeparator(to: result, context: &context)
        let literal = String(cString: cmark_node_get_literal(node))
        let lang = cmark_node_get_fence_info(node).flatMap { String(cString: $0) }
        var attrs = context.makeAttributes()
        attrs[.font] = MarkdownStyle.monospaceFont
        attrs[.backgroundColor] = MarkdownStyle.codeBlockBackground
        attrs[.markdownCodeBlock] = true
        if let lang, !lang.isEmpty {
            attrs[.markdownCodeBlockLang] = lang
        }
        let text = literal.hasSuffix("\n") ? String(literal.dropLast()) : literal
        result.append(NSAttributedString(string: text, attributes: attrs))
        context.needsBlockSeparator = true

    case CMARK_NODE_LINK:
        let urlStr = cmark_node_get_url(node).flatMap { String(cString: $0) } ?? ""
        let resolved = resolveURL(urlStr, baseURL: context.baseURL)
        var linkExtra: [NSAttributedString.Key: Any] = [:]
        if let resolved { linkExtra[.link] = resolved }
        context.pushStyle(
            color: MarkdownStyle.linkColor,
            extra: linkExtra
        )
        renderChildren(node, into: result, context: &context)
        context.popStyle()

    case CMARK_NODE_IMAGE:
        let urlStr = cmark_node_get_url(node).flatMap { String(cString: $0) } ?? ""
        let alt = collectText(from: node)
        let resolvedStr = resolveURLString(urlStr, baseURL: context.baseURL)

        if result.length > 0, !result.string.hasSuffix("\n") {
            result.append(NSAttributedString(string: "\n", attributes: context.makeAttributes()))
        }

        let attachment = NSTextAttachment()
        let placeholderWidth = UIScreen.main.bounds.width - 40
        let placeholder = createPlaceholderImage(width: placeholderWidth, height: 160)
        attachment.image = placeholder
        attachment.bounds = CGRect(x: 0, y: 0, width: placeholderWidth, height: 160)

        var attrs: [NSAttributedString.Key: Any] = context.makeAttributes()
        attrs[.markdownImageURL] = resolvedStr
        attrs[.markdownImageAlt] = alt

        let attachStr = NSMutableAttributedString(attachment: attachment)
        attachStr.addAttributes(attrs, range: NSRange(location: 0, length: attachStr.length))

        let imageRange = NSRange(location: result.length, length: attachStr.length)
        result.append(attachStr)
        result.append(NSAttributedString(string: "\n", attributes: context.makeAttributes()))

        context.imageInfos.append(ImageInfo(
            url: resolvedStr,
            alt: alt,
            attachment: attachment,
            range: imageRange
        ))

    case CMARK_NODE_LIST:
        let ordered = cmark_node_get_list_type(node) == CMARK_ORDERED_LIST
        let start = Int(cmark_node_get_list_start(node))
        let depth = context.listStack.count
        if depth == 0 {
            appendBlockSeparator(to: result, context: &context)
        }
        context.listStack.append(ListContext(ordered: ordered, itemIndex: start, depth: depth))
        renderChildren(node, into: result, context: &context)
        context.listStack.removeLast()
        if context.listStack.isEmpty {
            context.needsBlockSeparator = true
        }

    case CMARK_NODE_ITEM:
        if result.length > 0 && !result.string.hasSuffix("\n") {
            result.append(NSAttributedString(string: "\n", attributes: context.makeAttributes()))
        }

        let depth = context.listStack.last?.depth ?? 0
        let ordered = context.listStack.last?.ordered ?? false
        let index = context.listStack.last?.itemIndex ?? 1

        let isTaskItem = cmark_gfm_extensions_get_tasklist_item_checked(node) || isTasklistNode(node)
        let isChecked = cmark_gfm_extensions_get_tasklist_item_checked(node)

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

        result.append(NSAttributedString(string: prefix, attributes: prefixAttrs))

        context.pushStyle(extra: [
            .paragraphStyle: paragraphStyle,
            .markdownListType: listType,
            .markdownListItemIndex: ordered ? index : 0,
            .markdownListDepth: depth,
        ])
        renderChildren(node, into: result, context: &context)
        context.popStyle()

    case CMARK_NODE_BLOCK_QUOTE:
        appendBlockSeparator(to: result, context: &context)
        context.inBlockquote = true
        renderChildren(node, into: result, context: &context)
        context.inBlockquote = false
        context.needsBlockSeparator = true

    case CMARK_NODE_THEMATIC_BREAK:
        appendBlockSeparator(to: result, context: &context)
        var attrs = context.makeAttributes()
        attrs[.foregroundColor] = UIColor.white.withAlphaComponent(0.3)
        result.append(NSAttributedString(string: "───", attributes: attrs))
        context.needsBlockSeparator = true

    case CMARK_NODE_HTML_BLOCK:
        appendBlockSeparator(to: result, context: &context)
        if let literal = cmark_node_get_literal(node) {
            let text = String(cString: literal)
            result.append(NSAttributedString(string: text.trimmingCharacters(in: .whitespacesAndNewlines), attributes: context.makeAttributes()))
        }
        context.needsBlockSeparator = true

    case CMARK_NODE_HTML_INLINE:
        if let literal = cmark_node_get_literal(node) {
            let text = String(cString: literal)
            result.append(NSAttributedString(string: text, attributes: context.makeAttributes()))
        }

    default:
        if cmark_node_get_type_string(node) != nil {
            let typeStr = String(cString: cmark_node_get_type_string(node))
            if typeStr == "strikethrough" {
                context.pushStyle(extra: [
                    .strikethroughStyle: NSUnderlineStyle.single.rawValue,
                    .markdownStrikethrough: true,
                ])
                renderChildren(node, into: result, context: &context)
                context.popStyle()
                return
            }
        }
        renderChildren(node, into: result, context: &context)
    }
}

private func renderChildren(_ node: UnsafeMutablePointer<cmark_node>, into result: NSMutableAttributedString, context: inout RenderContext) {
    var child = cmark_node_first_child(node)
    while let c = child {
        renderNode(c, into: result, context: &context)
        child = cmark_node_next(c)
    }
}

private func appendBlockSeparator(to result: NSMutableAttributedString, context: inout RenderContext) {
    guard context.needsBlockSeparator, result.length > 0 else {
        context.needsBlockSeparator = false
        return
    }
    result.append(NSAttributedString(string: "\n", attributes: MarkdownStyle.baseAttributes))
    context.needsBlockSeparator = false
}

// MARK: - Font Helpers

private func addBoldTrait(to font: UIFont) -> UIFont {
    let descriptor = font.fontDescriptor
    var traits = descriptor.symbolicTraits
    traits.insert(.traitBold)
    guard let newDescriptor = descriptor.withSymbolicTraits(traits) else { return font }
    return UIFont(descriptor: newDescriptor, size: font.pointSize)
}

private func addItalicTrait(to font: UIFont) -> UIFont {
    let descriptor = font.fontDescriptor
    var traits = descriptor.symbolicTraits
    traits.insert(.traitItalic)
    guard let newDescriptor = descriptor.withSymbolicTraits(traits) else { return font }
    return UIFont(descriptor: newDescriptor, size: font.pointSize)
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

private func resolveURLString(_ urlStr: String, baseURL: URL?) -> String {
    if URL(string: urlStr)?.scheme != nil { return urlStr }
    guard let baseURL else { return urlStr }
    let base = baseURL.absoluteString.hasSuffix("/")
        ? String(baseURL.absoluteString.dropLast())
        : baseURL.absoluteString
    return base + urlStr
}

private func resolveRelativeURLs(in html: String, baseURL: URL) -> String {
    let base = baseURL.absoluteString.hasSuffix("/")
        ? String(baseURL.absoluteString.dropLast())
        : baseURL.absoluteString
    return html
        .replacingOccurrences(of: "src=\"/api/", with: "src=\"\(base)/api/")
        .replacingOccurrences(of: "href=\"/api/", with: "href=\"\(base)/api/")
}

private func escapeHTML(_ text: String) -> String {
    text.replacingOccurrences(of: "&", with: "&amp;")
        .replacingOccurrences(of: "<", with: "&lt;")
        .replacingOccurrences(of: ">", with: "&gt;")
}

private func isTasklistNode(_ node: UnsafeMutablePointer<cmark_node>) -> Bool {
    guard let child = cmark_node_first_child(node) else { return false }
    if cmark_node_get_type(child) == CMARK_NODE_PARAGRAPH {
        if let textNode = cmark_node_first_child(child),
           cmark_node_get_type(textNode) == CMARK_NODE_TEXT,
           let literal = cmark_node_get_literal(textNode) {
            let text = String(cString: literal)
            return text.hasPrefix("[ ] ") || text.hasPrefix("[x] ") || text.hasPrefix("[X] ")
        }
    }
    return false
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

private func createPlaceholderImage(width: CGFloat, height: CGFloat) -> UIImage {
    let renderer = UIGraphicsImageRenderer(size: CGSize(width: width, height: height))
    return renderer.image { ctx in
        UIColor.white.withAlphaComponent(0.06).setFill()
        let path = UIBezierPath(roundedRect: CGRect(x: 0, y: 0, width: width, height: height), cornerRadius: 8)
        path.fill()

        let icon = UIImage(systemName: "photo")?.withTintColor(
            UIColor.white.withAlphaComponent(0.2),
            renderingMode: .alwaysOriginal
        )
        if let icon {
            let iconSize: CGFloat = 32
            let iconRect = CGRect(
                x: (width - iconSize) / 2,
                y: (height - iconSize) / 2,
                width: iconSize,
                height: iconSize
            )
            icon.draw(in: iconRect)
        }
    }
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

        let font = attrs[.font] as? UIFont
        let isBold = font?.fontDescriptor.symbolicTraits.contains(.traitBold) == true && !isHeading
        let isItalic = font?.fontDescriptor.symbolicTraits.contains(.traitItalic) == true
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
