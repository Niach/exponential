import cmark_gfm
import cmark_gfm_extensions
import Foundation
import SwiftUI
import os
import UIKit

private let log = Logger(subsystem: "com.exponential", category: "MarkdownConversion")

/// Parse-time deviations from the interchange contract, for renders that never
/// serialize back (EXP-440: the agent steering feed). The default — no options —
/// IS the contract every editable surface must keep.
public struct MarkdownParseOptions: OptionSet, Sendable {
    public let rawValue: Int

    public init(rawValue: Int) { self.rawValue = rawValue }

    /// Attach cmark's GFM autolink extension, so a bare URL renders tappable.
    /// READ-ONLY renders only — it breaks byte parity on save (see the
    /// no-autolink note in `markdownToBlocks`).
    public static let autolinkBareURLs = MarkdownParseOptions(rawValue: 1 << 0)

    /// Chat semantics: a single newline (a cmark softbreak) renders as a LINE
    /// BREAK rather than a space, because agent narration and steered messages
    /// are written as terminal output, not as GFM paragraphs.
    public static let hardLineBreaks = MarkdownParseOptions(rawValue: 1 << 1)
}

public enum ContentBlock: Identifiable, Equatable {
    case text(id: UUID, attributedContent: NSAttributedString)
    case image(id: UUID, url: String, alt: String)
    /// A GFM pipe table (EXP-726). Its CELL ids share this id namespace, so
    /// `IssueEditorModel` routes a cell edit exactly like a block edit.
    case table(id: UUID, table: TableBlock)

    public var id: UUID {
        switch self {
        case .text(let id, _): return id
        case .image(let id, _, _): return id
        case .table(let id, _): return id
        }
    }

    /// Everything that is NOT an editable text run — an image or a table. Those
    /// blocks are never adjacent to one another in the document: `normalize`
    /// keeps an (initially empty) text block between them, above the first and
    /// below the last, so the editor always has somewhere to put the caret.
    public var isBlockLevel: Bool {
        if case .text = self { return false }
        return true
    }

    public static func normalize(_ blocks: inout [ContentBlock]) {
        if blocks.isEmpty {
            blocks = [.text(id: UUID(), attributedContent: NSAttributedString())]
            return
        }
        if blocks.first?.isBlockLevel == true {
            blocks.insert(.text(id: UUID(), attributedContent: NSAttributedString()), at: 0)
        }
        if blocks.last?.isBlockLevel == true {
            blocks.append(.text(id: UUID(), attributedContent: NSAttributedString()))
        }
        var i = 1
        while i < blocks.count {
            if blocks[i].isBlockLevel, blocks[i - 1].isBlockLevel {
                blocks.insert(.text(id: UUID(), attributedContent: NSAttributedString()), at: i)
            }
            i += 1
        }
    }
}

public enum MarkdownConversion {

    // MARK: - NSAttributedString → Markdown

    /// The GFM interchange form of an intentional blank line — a paragraph
    /// holding only a no-break space, written as the entity so it survives
    /// every client's parser as a visually empty paragraph (EXP-7 on web,
    /// EXP-689 here).
    public static let blankLineMarker = "&nbsp;"

    /// Empty, or whitespace-only (U+00A0 included) and not fence content.
    private static func isBlankParagraph(_ range: NSRange, in attrStr: NSAttributedString) -> Bool {
        if range.length == 0 {
            // A zero-length line inside a fence is a blank code line, kept
            // by `splitIntoParagraphs` precisely so it survives.
            guard range.location < attrStr.length else { return true }
            return (attrStr.attribute(.markdownCodeBlock, at: range.location, effectiveRange: nil) as? Bool) != true
        }
        let attrs = attrStr.attributes(at: range.location, effectiveRange: nil)
        if attrs[.markdownCodeBlock] as? Bool == true {
            return false
        }
        let text = (attrStr.string as NSString).substring(with: range)
        return text.unicodeScalars.allSatisfy { CharacterSet.whitespaces.contains($0) }
    }

    public static func attributedStringToMarkdown(_ attrStr: NSAttributedString) -> String {
        let fullText = attrStr.string
        guard !fullText.isEmpty else { return "" }

        var markdown = ""
        var inCodeBlock = false
        var codeBlockLang: String?

        let paragraphs = splitIntoParagraphs(attrStr)
        log.debug("attributedStringToMarkdown: \(paragraphs.count) paragraphs from \(attrStr.length) chars")

        // EXP-689: an intentional blank line (two Returns) is an empty,
        // whitespace-only paragraph. GFM cannot carry one as bare newlines —
        // every parser folds `A\n\n\n\nB` into `A\n\nB` — so INTERIOR blank
        // paragraphs are written as the contract's `&nbsp;` line (web's
        // MarkdownParagraph does exactly this) and leading/trailing ones are
        // dropped as meaningless spacing. A blank line inside a fence is code.
        let blank = paragraphs.map { isBlankParagraph($0, in: attrStr) }
        let firstContent = blank.firstIndex(of: false) ?? paragraphs.count
        let lastContent = blank.lastIndex(of: false) ?? -1

        for (i, para) in paragraphs.enumerated() {
            guard para.location < attrStr.length, NSMaxRange(para) <= attrStr.length else {
                log.error("paragraph out of bounds: \(para.location)+\(para.length) vs \(attrStr.length)")
                continue
            }
            if blank[i] {
                guard i > firstContent, i < lastContent else { continue }
                if inCodeBlock {
                    markdown += "```"
                    inCodeBlock = false
                    codeBlockLang = nil
                }
                markdown += "\n\n" + blankLineMarker
                continue
            }
            let paraStr = attrStr.attributedSubstring(from: para)
            let attrs = attrStr.attributes(at: para.location, effectiveRange: nil)

            if let isCode = attrs[.markdownCodeBlock] as? Bool, isCode {
                let lang = attrs[.markdownCodeBlockLang] as? String
                // Back-to-back fences with DIFFERENT languages must not merge
                // into the first fence: close the open one, then the reopen logic
                // below starts a fresh fence with the new language. (Same-lang or
                // both-untagged adjacent fences still merge — content-equivalent.)
                if inCodeBlock, lang != codeBlockLang {
                    markdown += "```\n"
                    inCodeBlock = false
                }
                if !inCodeBlock {
                    if i > 0 { markdown += "\n" }
                    codeBlockLang = lang
                    markdown += "```\(codeBlockLang ?? "")\n"
                    inCodeBlock = true
                }
                markdown += expWithoutObjectReplacements(paraStr.string)
                if !paraStr.string.hasSuffix("\n") { markdown += "\n" }
                continue
            }

            if inCodeBlock {
                // Close without a trailing newline: the block separator below (or
                // the table branch) supplies the spacing, so a fence followed by
                // another block no longer accretes an extra blank line per save.
                markdown += "```"
                inCodeBlock = false
                codeBlockLang = nil
            }

            if i > 0 {
                // A blank line between two items is a paragraph break, never a
                // tight-list joiner, whatever attributes its newline carries.
                if !blank[i - 1],
                   let prevAttrs = i > 0 ? attrStr.attributes(at: paragraphs[i - 1].location, effectiveRange: nil) : nil,
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

    public static func markdownToBlocks(
        _ markdown: String,
        baseURL: URL? = nil,
        options: MarkdownParseOptions = [],
        overrides: MarkdownStyle.Overrides = .none
    ) -> [ContentBlock] {
        cmark_gfm_core_extensions_ensure_registered()

        guard let parser = cmark_parser_new(CMARK_OPT_UNSAFE) else {
            return [.text(id: UUID(), attributedContent: NSAttributedString(string: markdown, attributes: MarkdownStyle.baseAttributes))]
        }
        defer { cmark_parser_free(parser) }

        // NOTE: no autolink extension by DEFAULT — web (tiptap-markdown) and
        // Android leave bare URLs/emails as plain text, so autolinking here
        // would rewrite `https://x` to `[https://x](https://x)` and — worse —
        // the email part of an `@<email>` mention to `@[email](mailto:email)`
        // on every load→save cycle, breaking the byte-parity interchange
        // contract (and the server's `@email` mention resolution with it). It
        // stays banned for anything that serializes; `.autolinkBareURLs` is
        // only for display-only renders (the agent steering feed, EXP-440).
        for name in ["strikethrough", "table"] {
            if let ext = cmark_find_syntax_extension(name) {
                cmark_parser_attach_syntax_extension(parser, ext)
            }
        }
        if options.contains(.autolinkBareURLs), let ext = cmark_find_syntax_extension("autolink") {
            cmark_parser_attach_syntax_extension(parser, ext)
        }

        markdown.withCString { ptr in
            cmark_parser_feed(parser, ptr, strlen(ptr))
        }

        guard let doc = cmark_parser_finish(parser) else {
            return [.text(id: UUID(), attributedContent: NSAttributedString(string: markdown, attributes: MarkdownStyle.baseAttributes))]
        }
        defer { cmark_node_free(doc) }

        let collector = BlockCollector(baseURL: baseURL)
        var context = RenderContext(baseURL: baseURL, options: options, overrides: overrides)
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
            case .table(_, let table):
                let md = serializeTable(table)
                if !md.isEmpty { parts.append(md) }
            }
        }
        return parts.joined(separator: "\n\n")
    }

    // MARK: - Tables → Markdown (EXP-726)

    /// The canonical GFM pipe-table form every client emits. See
    /// `MarkdownTable.swift` for the full contract.
    static func serializeTable(_ table: TableBlock) -> String {
        guard table.columnCount > 0 else { return "" }
        var lines: [String] = [serializeTableRow(table.header)]
        lines.append(
            "| " + table.alignments.map(\.delimiter).joined(separator: " | ") + " |"
        )
        for row in table.rows { lines.append(serializeTableRow(row)) }
        return lines.joined(separator: "\n")
    }

    private static func serializeTableRow(_ cells: [TableCell]) -> String {
        // An empty cell therefore renders as `|  |` (two spaces), which is the
        // contract's empty-cell form on every client.
        "| " + cells.map { serializeTableCell($0.content) }.joined(separator: " | ") + " |"
    }

    /// One cell is ONE inline paragraph: no block constructs, no newlines, and
    /// a literal `|` escaped so it cannot split the row.
    private static func serializeTableCell(_ content: NSAttributedString) -> String {
        var text = extractInlineMarkdown(from: content, isHeading: false)
        if text.contains("\n") || text.contains("\r") {
            text = text
                .components(separatedBy: .newlines)
                .joined(separator: " ")
        }
        text = text.replacingOccurrences(of: "|", with: "\\|")
        // The `| ` / ` |` delimiters supply the padding; anything else would
        // drift from the canonical bytes.
        return text.trimmingCharacters(in: .whitespaces)
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
    var options: MarkdownParseOptions = []
    /// EXP-698: display-only palette deviations (the chat feeds' code tint).
    var overrides: MarkdownStyle.Overrides = .none
    var styleStack: [StyleFrame] = [StyleFrame(
        font: MarkdownStyle.bodyFont,
        foregroundColor: MarkdownStyle.textColor,
        extraAttributes: [:]
    )]
    var listStack: [ListContext] = []
    var headingLevel: Int = 0
    var inCodeBlock = false
    var inBlockquote = false
    /// EXP-726: rendering the inline children of a `table_cell`. A cell is ONE
    /// inline paragraph, so an image stays literal `![alt](url)` text and both
    /// break kinds collapse to a space — whatever `.hardLineBreaks` says.
    var inTableCell = false
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
        // Drop the trailing newline only when it is the base-attributed
        // block-separator between this run and the next block. A CODE-attributed
        // trailing newline is fence content (a blank line inside the fence), so
        // it must survive — paired with the code-aware split, it round-trips.
        if content.length > 0, content.string.hasSuffix("\n"),
           (content.attribute(.markdownCodeBlock, at: content.length - 1, effectiveRange: nil) as? Bool) != true {
            content.deleteCharacters(in: NSRange(location: content.length - 1, length: 1))
        }
        blocks.append(.text(id: UUID(), attributedContent: content))
        currentText = NSMutableAttributedString()
    }

    func emitImage(url: String, alt: String) {
        flushText()
        blocks.append(.image(id: UUID(), url: url, alt: alt))
    }

    func emitTable(_ table: TableBlock) {
        flushText()
        blocks.append(.table(id: UUID(), table: table))
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
            context.pushStyle(color: MarkdownStyle.blockquoteTextColor, extra: [
                .markdownBlockquote: true,
                // Indent clears the gutter for the quote bar drawn by
                // MarkdownLayoutManager (EXP-246).
                .paragraphStyle: MarkdownStyle.blockquoteParagraphStyle,
            ])
        }
        let paragraphStart = collector.currentText.length
        renderChildrenToBlocks(node, collector: collector, context: &context)
        if context.inBlockquote { context.popStyle() }
        // EXP-689: a whitespace-only plain paragraph is the stored form of an
        // intentional blank line (`&nbsp;`, decoded by cmark to U+00A0) —
        // fold it to a genuinely empty line so the editor shows no invisible
        // character and the save path writes the `&nbsp;` form back.
        if !context.inBlockquote, context.listStack.isEmpty,
           collector.currentText.length > paragraphStart {
            let appended = (collector.currentText.string as NSString)
                .substring(from: paragraphStart)
            if appended.unicodeScalars.allSatisfy({ CharacterSet.whitespaces.contains($0) }) {
                collector.currentText.deleteCharacters(
                    in: NSRange(location: paragraphStart, length: collector.currentText.length - paragraphStart)
                )
            }
        }
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
        // GFM folds a single newline into a space; chat-shaped sources keep it
        // as a line break (`.hardLineBreaks`, EXP-440). Inside a table cell it
        // is always a space (EXP-726).
        let softbreak = (context.options.contains(.hardLineBreaks) && !context.inTableCell) ? "\n" : " "
        collector.currentText.append(NSAttributedString(string: softbreak, attributes: context.makeAttributes()))

    case CMARK_NODE_LINEBREAK:
        let linebreak = context.inTableCell ? " " : "\n"
        collector.currentText.append(NSAttributedString(string: linebreak, attributes: context.makeAttributes()))

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
        // EXP-698: the chat feeds tint inline code (`Semantic.codeText` on a
        // `codeFill` wash); everywhere else keeps the neutral white@8 %.
        attrs[.backgroundColor] = context.overrides.inlineCodeBackground
            .map { PlatformColor($0) } ?? MarkdownStyle.codeBackground
        if let foreground = context.overrides.inlineCodeForeground {
            attrs[.foregroundColor] = PlatformColor(foreground)
        }
        attrs[.markdownInlineCode] = true
        collector.currentText.append(NSAttributedString(string: literal, attributes: attrs))

    case CMARK_NODE_CODE_BLOCK:
        appendBlockSeparatorToCollector(collector: collector, context: &context)
        let literal = String(cString: cmark_node_get_literal(node))
        let lang = cmark_node_get_fence_info(node).flatMap { String(cString: $0) }
        var attrs = context.makeAttributes()
        attrs[.font] = MarkdownStyle.monospaceFont
        // No `.backgroundColor` here: UITextView paints it per line fragment
        // (a stripe per line). MarkdownLayoutManager draws the whole fence as
        // ONE rounded box off `.markdownCodeBlock` instead (EXP-246).
        attrs[.markdownCodeBlock] = true
        if let lang, !lang.isEmpty { attrs[.markdownCodeBlockLang] = lang }
        var text = literal.hasSuffix("\n") ? String(literal.dropLast()) : literal
        // A fence containing only blank lines would otherwise append an EMPTY run
        // and vanish (attributes can't ride a zero-length string) — restore one
        // newline so the code attribute has a character to carry. Keep dropLast
        // for normal fences (removing it would show a phantom trailing blank line
        // inside every code block in the editor).
        if text.isEmpty && !literal.isEmpty { text = "\n" }
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
        // A table cell holds one inline paragraph and can never host an image
        // BLOCK, so the image stays literal source text there (EXP-726) —
        // exactly what web, desktop and Android do inside cells.
        if context.inTableCell {
            collector.currentText.append(NSAttributedString(
                string: "![\(alt)](\(urlStr))", attributes: context.makeAttributes()))
            return
        }
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
        // The item boundary IS this "\n": clear any separator the previous
        // item's paragraph left pending, or the paragraph handler inside THIS
        // item fires it AFTER the baked prefix and splits the content onto its
        // own line ("2. \nSecond") — which serializes back as an empty item
        // plus a duplicate-index item instead of round-tripping byte-identical.
        context.needsBlockSeparator = false
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
            if typeStr == "table" {
                appendTable(node, collector: collector, context: &context)
                return
            }
        }
        renderChildrenToBlocks(node, collector: collector, context: &context)
    }
}

// EXP-726: a GFM pipe table becomes a real `ContentBlock.table` — a grid of
// per-cell attributed strings the editor renders and edits cell by cell, and
// the save path re-emits in the shared canonical form.
//
// The node shape is `table` → `table_header` / `table_row` → `table_cell`
// (the extension's own type strings; `CMARK_NODE_TABLE*` are runtime values,
// not compile-time cases, so the switch above dispatches on the string). Column
// count and alignments come off the TABLE node via the extension getters.
private func appendTable(
    _ node: UnsafeMutablePointer<cmark_node>,
    collector: BlockCollector,
    context: inout RenderContext
) {
    let columns = Int(cmark_gfm_extensions_get_table_columns(node))
    guard columns > 0 else {
        renderChildrenToBlocks(node, collector: collector, context: &context)
        return
    }

    var alignments: [TableAlignment] = []
    if let raw = cmark_gfm_extensions_get_table_alignments(node) {
        for column in 0..<columns {
            switch raw[column] {
            case UInt8(ascii: "l"): alignments.append(.left)
            case UInt8(ascii: "c"): alignments.append(.center)
            case UInt8(ascii: "r"): alignments.append(.right)
            default: alignments.append(.none)
            }
        }
    }

    var header: [TableCell]?
    var rows: [[TableCell]] = []
    var child = cmark_node_first_child(node)
    while let row = child {
        defer { child = cmark_node_next(row) }
        guard let typePtr = cmark_node_get_type_string(row) else { continue }
        let rowType = String(cString: typePtr)
        guard rowType == "table_header" || rowType == "table_row" else { continue }
        let isHeader = cmark_gfm_extensions_get_table_row_is_header(row) != 0
            || rowType == "table_header"

        var cells: [TableCell] = []
        var cellNode = cmark_node_first_child(row)
        while let cell = cellNode {
            defer { cellNode = cmark_node_next(cell) }
            guard let cellTypePtr = cmark_node_get_type_string(cell),
                  String(cString: cellTypePtr) == "table_cell" else { continue }
            cells.append(TableCell(content: renderTableCell(cell, context: context)))
        }
        // Ragged rows are padded with empty cells and over-long ones truncated
        // (`TableBlock.init` does both) so the grid is always rectangular.
        if isHeader, header == nil {
            while cells.count < columns { cells.append(TableCell()) }
            header = Array(cells.prefix(columns))
        } else {
            rows.append(cells)
        }
    }

    guard let header else {
        renderChildrenToBlocks(node, collector: collector, context: &context)
        return
    }
    collector.emitTable(TableBlock(header: header, rows: rows, alignments: alignments))
    context.needsBlockSeparator = false
}

/// One cell's inline children, rendered into a FRESH buffer so nothing leaks
/// into the surrounding text block. Leading/trailing whitespace is trimmed —
/// the serializer's `| ` / ` |` delimiters supply the padding, so a
/// GitHub-style width-padded source normalizes to the canonical bytes.
private func renderTableCell(
    _ node: UnsafeMutablePointer<cmark_node>,
    context: RenderContext
) -> NSAttributedString {
    let cellCollector = BlockCollector(baseURL: context.baseURL)
    var cellContext = context
    cellContext.inTableCell = true
    cellContext.needsBlockSeparator = false
    cellContext.headingLevel = 0
    cellContext.listStack = []
    cellContext.inBlockquote = false
    renderChildrenToBlocks(node, collector: cellCollector, context: &cellContext)
    return trimmedAttributedString(cellCollector.currentText)
}

private func trimmedAttributedString(_ attributed: NSAttributedString) -> NSAttributedString {
    let ns = attributed.string as NSString
    var start = 0
    var end = ns.length
    let whitespace = CharacterSet.whitespacesAndNewlines
    while start < end,
          let scalar = Unicode.Scalar(ns.character(at: start)),
          whitespace.contains(scalar) {
        start += 1
    }
    while end > start,
          let scalar = Unicode.Scalar(ns.character(at: end - 1)),
          whitespace.contains(scalar) {
        end -= 1
    }
    guard start != 0 || end != ns.length else {
        return NSAttributedString(attributedString: attributed)
    }
    return attributed.attributedSubstring(from: NSRange(location: start, length: end - start))
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

/// Drop every U+FFFC (OBJECT REPLACEMENT CHARACTER) from text on its way into
/// the markdown.
///
/// U+FFFC has NO legitimate place in GFM source — it is never typeable and
/// never authored, it only ever enters the editor's storage as a display-only
/// artifact: the issue-ref chip title rides one as an `NSTextAttachment`
/// (EXP-322), and copy/paste or drag-and-drop of a chip re-enters it as a BARE
/// character (`allowsEditingTextAttributes` is off, so the attachment
/// attribute does not survive the pasteboard). So stripping it unconditionally
/// here is correct, and this is the one chokepoint no decoration or paste path
/// can bypass — attribute-based skipping alone is not enough, because the
/// verbatim code-fence emitter re-emits its source string without consulting
/// attributes at all (EXP-322).
public func expWithoutObjectReplacements(_ text: String) -> String {
    text.contains("\u{FFFC}") ? text.replacingOccurrences(of: "\u{FFFC}", with: "") : text
}

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
        // Zero-length lines are kept too: inside a fence they are blank code
        // lines the save path writes back verbatim; elsewhere they are the
        // user's intentional blank lines, which `attributedStringToMarkdown`
        // persists as `&nbsp;` paragraphs (EXP-689). The base-attributed
        // block-separator newline is never a standalone zero-length line (it
        // terminates the preceding content line), so ordinary block spacing
        // is byte-identical to before.
        if trimmedRange.length > 0 || trimmedRange.location < length {
            ranges.append(trimmedRange)
        }
        start = NSMaxRange(lineRange)
    }

    return ranges
}

private func extractInlineMarkdown(from attrStr: NSAttributedString, isHeading: Bool, stripListPrefix: Bool = false) -> String {
    var markdown = ""
    let fullRange = NSRange(location: 0, length: attrStr.length)
    let string = attrStr.string
    var effectiveRange = fullRange

    if stripListPrefix {
        // Must strip EVERY visual prefix the load path bakes as literal text
        // (CMARK_NODE_ITEM branch): bullet "• ", checkbox "☐ "/"☑ ", and the
        // ordered "<n>. " form from `prefix = "\(index). "`. The save path
        // re-emits the marker itself from the list attributes, so any
        // unstripped prefix duplicates on every save ("1. 1. First").
        // Deliberately NOT a regex: `range(of:options:.regularExpression)` is
        // backed by different engines across OS releases, and the alternation
        // form `(?:[\u{2022}\u{2610}\u{2611}]|\d+\.)` silently stopped
        // matching glyph prefixes on emoji-bearing strings on the iOS 26
        // simulator — plain scalar inspection is deterministic everywhere.
        // The length is UTF-16 (NSRange space — enumerateAttributes must never
        // see an out-of-bounds range); every prefix scalar is BMP, so scalar
        // count == UTF-16 count. Clamp as a belt-and-suspenders guard.
        let prefixUTF16 = bakedListPrefixUTF16Length(of: string)
        if prefixUTF16 > 0 {
            let loc = min(prefixUTF16, attrStr.length)
            effectiveRange = NSRange(location: loc, length: attrStr.length - loc)
            if effectiveRange.length <= 0 { return "" }
        }
    }

    var runs: [InlineRun] = []
    attrStr.enumerateAttributes(in: effectiveRange, options: []) { attrs, range, _ in
        let substring = (attrStr.string as NSString).substring(with: range)
        runs.append(InlineRun(text: substring, attrs: attrs, isHeading: isHeading))
    }

    // A link's text carries the emphasis/code attributes of the marks nested
    // inside it, and enumerateAttributes splits it into one run per attribute
    // change — so a link must be emitted as ONE `[...](href)` spanning every
    // consecutive run with the same href, with the inner delimiters composed
    // INSIDE the brackets. Short-circuiting per run used to strip the inner
    // formatting, drop the href entirely around inline code, and split
    // `[**bold** rest](u)` into two adjacent links (REV2-19).
    var i = 0
    while i < runs.count {
        let run = runs[i]
        if let imageURL = run.imageURL {
            markdown += "![\(run.imageAlt)](\(imageURL))"
            i += 1
            continue
        }
        if run.isAttachment {
            i += 1
            continue
        }
        guard let href = run.href else {
            markdown += run.styled
            i += 1
            continue
        }
        // (Image/attachment runs never carry a href, so they break the group.)
        var inner = ""
        while i < runs.count, runs[i].href == href {
            inner += runs[i].styled
            i += 1
        }
        if !inner.isEmpty { markdown += "[\(inner)](\(href))" }
    }

    return markdown
}

/// One `enumerateAttributes` run, reduced to the inline features the markdown
/// serializer emits.
private struct InlineRun {
    let text: String
    var imageURL: String?
    var imageAlt = ""
    var isAttachment = false
    var href: String?
    var isCode = false
    var isBold = false
    var isItalic = false
    var isStrike = false

    init(text: String, attrs: [NSAttributedString.Key: Any], isHeading: Bool) {
        // A run that is a real `.attachment` is skipped by the caller, but a
        // BARE U+FFFC (a pasted chip) carries no attachment attribute and would
        // otherwise be emitted verbatim — see `expWithoutObjectReplacements`.
        self.text = expWithoutObjectReplacements(text)
        if let imageURL = attrs[.markdownImageURL] as? String {
            self.imageURL = imageURL
            imageAlt = (attrs[.markdownImageAlt] as? String) ?? ""
            return
        }
        if attrs[.attachment] is NSTextAttachment {
            isAttachment = true
            return
        }
        if let url = attrs[.link] as? URL {
            href = url.absoluteString
        } else if let url = attrs[.link] as? String {
            href = url
        }
        isCode = attrs[.markdownInlineCode] as? Bool == true
        let font = attrs[.font] as? PlatformFont
        isBold = expFontHasBold(font) && !isHeading
        isItalic = expFontHasItalic(font)
        isStrike = attrs[.markdownStrikethrough] as? Bool == true
    }

    /// Inline delimiters for this run, link wrapper excluded. Inline code stays
    /// exclusive of the emphasis delimiters — the load path swaps in the
    /// monospace font and loses the bold/italic traits, so there is nothing to
    /// compose (Android matches, keeping the two native clients byte-identical).
    var styled: String {
        if text.isEmpty { return "" }
        if isCode { return "`\(text)`" }
        var out = text
        if isStrike { out = "~~\(out)~~" }
        if isBold && isItalic { out = "***\(out)***" }
        else if isBold { out = "**\(out)**" }
        else if isItalic { out = "*\(out)*" }
        return out
    }
}

/// UTF-16 length of the baked list-item prefix at the start of `string`:
/// bullet `• `, checkbox `☐ `/`☑ `, or ordered `<digits>. `, each with one
/// optional trailing space (the load path always bakes one; a mid-edit
/// paragraph may have lost it). Returns 0 when no prefix is present.
private func bakedListPrefixUTF16Length(of string: String) -> Int {
    let scalars = string.unicodeScalars
    var index = scalars.startIndex
    guard index < scalars.endIndex else { return 0 }
    var length: Int
    let first = scalars[index].value
    if first == 0x2022 || first == 0x2610 || first == 0x2611 { // • ☐ ☑
        length = 1
        index = scalars.index(after: index)
    } else {
        var digits = 0
        while index < scalars.endIndex, (0x30...0x39).contains(scalars[index].value) {
            digits += 1
            index = scalars.index(after: index)
        }
        guard digits > 0, index < scalars.endIndex, scalars[index].value == 0x2E else { return 0 } // "."
        length = digits + 1
        index = scalars.index(after: index)
    }
    if index < scalars.endIndex, scalars[index].value == 0x20 { // " "
        length += 1
    }
    return length
}
