import os
import UIKit

private let log = Logger(subsystem: "com.exponential", category: "MarkdownToolbar")

final class MarkdownToolbar: UIInputView {
    weak var textView: UITextView?
    var onImagePick: (() -> Void)?

    private var headingButton: UIButton!
    private var boldButton: UIButton!
    private var italicButton: UIButton!
    private var underlineButton: UIButton!
    private var strikethroughButton: UIButton!
    private var bulletListButton: UIButton!
    private var checklistButton: UIButton!
    private var codeButton: UIButton!
    private var quoteButton: UIButton!

    init() {
        super.init(frame: CGRect(x: 0, y: 0, width: UIScreen.main.bounds.width, height: 48),
                   inputViewStyle: .keyboard)
        allowsSelfSizing = true
        setupBar()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError() }

    private func setupBar() {
        let pill = UIView()
        pill.backgroundColor = UIColor(white: 0.22, alpha: 1.0)
        pill.layer.cornerRadius = 10
        pill.translatesAutoresizingMaskIntoConstraints = false
        addSubview(pill)

        let iconConfig = UIImage.SymbolConfiguration(pointSize: 16, weight: .medium)
        let tint = UIColor.white.withAlphaComponent(0.65)

        func makeButton(_ symbol: String, _ action: Selector) -> UIButton {
            let b = UIButton(type: .system)
            b.setImage(UIImage(systemName: symbol, withConfiguration: iconConfig), for: .normal)
            b.tintColor = tint
            b.addTarget(self, action: action, for: .touchUpInside)
            b.translatesAutoresizingMaskIntoConstraints = false
            b.widthAnchor.constraint(equalToConstant: 36).isActive = true
            b.heightAnchor.constraint(equalToConstant: 36).isActive = true
            return b
        }

        func makeSep() -> UIView {
            let sep = UIView()
            sep.backgroundColor = UIColor.white.withAlphaComponent(0.12)
            sep.translatesAutoresizingMaskIntoConstraints = false
            sep.widthAnchor.constraint(equalToConstant: 1).isActive = true
            sep.heightAnchor.constraint(equalToConstant: 18).isActive = true
            return sep
        }

        headingButton = makeButton("textformat.size", #selector(toggleHeading))
        boldButton = makeButton("bold", #selector(toggleBold))
        italicButton = makeButton("italic", #selector(toggleItalic))
        underlineButton = makeButton("underline", #selector(tapUnderline))
        strikethroughButton = makeButton("strikethrough", #selector(tapStrikethrough))
        bulletListButton = makeButton("list.bullet", #selector(toggleBulletList))
        checklistButton = makeButton("checklist", #selector(toggleChecklist))
        codeButton = makeButton("chevron.left.forwardslash.chevron.right", #selector(toggleCode))
        quoteButton = makeButton("text.quote", #selector(toggleBlockquote))
        let imageButton = makeButton("photo", #selector(pickImage))
        let dismissButton = makeButton("keyboard.chevron.compact.down", #selector(dismissKeyboard))

        let scroll = UIScrollView()
        scroll.showsHorizontalScrollIndicator = false
        scroll.translatesAutoresizingMaskIntoConstraints = false

        let stack = UIStackView(arrangedSubviews: [
            imageButton,
            makeSep(),
            headingButton, boldButton, italicButton, underlineButton, strikethroughButton,
            makeSep(),
            bulletListButton, checklistButton, codeButton, quoteButton,
        ])
        stack.axis = .horizontal
        stack.alignment = .center
        stack.spacing = 2
        stack.translatesAutoresizingMaskIntoConstraints = false

        scroll.addSubview(stack)
        pill.addSubview(scroll)
        pill.addSubview(dismissButton)

        NSLayoutConstraint.activate([
            pill.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 6),
            pill.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -6),
            pill.topAnchor.constraint(equalTo: topAnchor, constant: 4),
            pill.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -4),
            pill.heightAnchor.constraint(equalToConstant: 40),

            dismissButton.trailingAnchor.constraint(equalTo: pill.trailingAnchor, constant: -2),
            dismissButton.centerYAnchor.constraint(equalTo: pill.centerYAnchor),

            scroll.leadingAnchor.constraint(equalTo: pill.leadingAnchor, constant: 2),
            scroll.trailingAnchor.constraint(equalTo: dismissButton.leadingAnchor, constant: -2),
            scroll.topAnchor.constraint(equalTo: pill.topAnchor),
            scroll.bottomAnchor.constraint(equalTo: pill.bottomAnchor),

            stack.leadingAnchor.constraint(equalTo: scroll.contentLayoutGuide.leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: scroll.contentLayoutGuide.trailingAnchor),
            stack.centerYAnchor.constraint(equalTo: scroll.centerYAnchor),
            stack.heightAnchor.constraint(equalTo: scroll.heightAnchor),
        ])
    }

    func updateState() {
        guard let textView else { return }
        let attrs = textView.typingAttributes

        let font = attrs[.font] as? UIFont
        let isHeading = (attrs[.markdownHeadingLevel] as? Int).map { $0 > 0 } ?? false
        let isBold = font?.fontDescriptor.symbolicTraits.contains(.traitBold) == true && !isHeading
        let isItalic = font?.fontDescriptor.symbolicTraits.contains(.traitItalic) == true
        let isUnderline = (attrs[.underlineStyle] as? Int).map { $0 != 0 } ?? false
        let isStrike = (attrs[.strikethroughStyle] as? Int).map { $0 != 0 } ?? false
        let isBullet = (attrs[.markdownListType] as? String) == "bullet"
        let isChecklist = (attrs[.markdownListType] as? String) == "checklist"
        let isCode = (attrs[.markdownCodeBlock] as? Bool) == true
        let isQuote = (attrs[.markdownBlockquote] as? Bool) == true

        setActive(headingButton, isHeading)
        setActive(boldButton, isBold)
        setActive(italicButton, isItalic)
        setActive(underlineButton, isUnderline)
        setActive(strikethroughButton, isStrike)
        setActive(bulletListButton, isBullet)
        setActive(checklistButton, isChecklist)
        setActive(codeButton, isCode)
        setActive(quoteButton, isQuote)
    }

    private func setActive(_ button: UIButton, _ active: Bool) {
        button.tintColor = active ? MarkdownStyle.linkColor : UIColor.white.withAlphaComponent(0.65)
    }

    // MARK: - Inline formatting

    @objc private func toggleBold() {
        guard let textView else { return }
        toggleFontTrait(.traitBold, in: textView)
    }

    @objc private func toggleItalic() {
        guard let textView else { return }
        toggleFontTrait(.traitItalic, in: textView)
    }

    @objc private func tapUnderline() {
        guard let textView else { return }
        let range = textView.selectedRange
        let current = (textView.typingAttributes[.underlineStyle] as? Int) ?? 0
        let newValue = current == 0 ? NSUnderlineStyle.single.rawValue : 0

        if range.length > 0 {
            if newValue == 0 {
                textView.textStorage.removeAttribute(.underlineStyle, range: range)
            } else {
                textView.textStorage.addAttribute(.underlineStyle, value: newValue, range: range)
            }
        }
        textView.typingAttributes[.underlineStyle] = newValue
        updateState()
        notifyDelegate(textView)
    }

    @objc private func tapStrikethrough() {
        guard let textView else { return }
        let range = textView.selectedRange
        let current = (textView.typingAttributes[.strikethroughStyle] as? Int) ?? 0
        let newValue = current == 0 ? NSUnderlineStyle.single.rawValue : 0

        if range.length > 0 {
            if newValue == 0 {
                textView.textStorage.removeAttribute(.strikethroughStyle, range: range)
                textView.textStorage.removeAttribute(.markdownStrikethrough, range: range)
            } else {
                textView.textStorage.addAttributes([
                    .strikethroughStyle: newValue,
                    .markdownStrikethrough: true,
                ], range: range)
            }
        }
        textView.typingAttributes[.strikethroughStyle] = newValue
        if newValue != 0 {
            textView.typingAttributes[.markdownStrikethrough] = true
        } else {
            textView.typingAttributes.removeValue(forKey: .markdownStrikethrough)
        }
        updateState()
        notifyDelegate(textView)
    }

    // MARK: - Heading

    @objc private func toggleHeading() {
        guard let textView else { return }
        let paraRange = paragraphRange(in: textView)

        let current: Int
        if textView.textStorage.length > 0, paraRange.location < textView.textStorage.length {
            current = (textView.textStorage.attributes(at: paraRange.location, effectiveRange: nil)[.markdownHeadingLevel] as? Int) ?? 0
        } else {
            current = (textView.typingAttributes[.markdownHeadingLevel] as? Int) ?? 0
        }

        let nextLevel: Int
        switch current {
        case 0: nextLevel = 1
        case 1: nextLevel = 2
        case 2: nextLevel = 3
        default: nextLevel = 0
        }

        if nextLevel > 0 {
            let font = MarkdownStyle.headingFont(level: nextLevel)
            if paraRange.length > 0 {
                textView.textStorage.addAttributes([
                    .font: font,
                    .markdownHeadingLevel: nextLevel,
                ], range: paraRange)
            }
            textView.typingAttributes[.font] = font
            textView.typingAttributes[.markdownHeadingLevel] = nextLevel
        } else {
            if paraRange.length > 0 {
                textView.textStorage.addAttribute(.font, value: MarkdownStyle.bodyFont, range: paraRange)
                textView.textStorage.removeAttribute(.markdownHeadingLevel, range: paraRange)
            }
            textView.typingAttributes[.font] = MarkdownStyle.bodyFont
            textView.typingAttributes.removeValue(forKey: .markdownHeadingLevel)
        }
        updateState()
        notifyDelegate(textView)
    }

    // MARK: - Lists

    @objc private func toggleBulletList() {
        guard let textView else { return }
        insertOrToggleList(type: "bullet", prefix: "\u{2022} ", in: textView)
    }

    @objc private func toggleChecklist() {
        guard let textView else { return }
        insertOrToggleList(type: "checklist", prefix: "\u{2610} ", in: textView)
    }

    // MARK: - Code block

    @objc private func toggleCode() {
        guard let textView else { return }
        let paraRange = paragraphRange(in: textView)

        let isCodeBlock: Bool
        if textView.textStorage.length > 0, paraRange.location < textView.textStorage.length {
            isCodeBlock = (textView.textStorage.attributes(at: paraRange.location, effectiveRange: nil)[.markdownCodeBlock] as? Bool) == true
        } else {
            isCodeBlock = (textView.typingAttributes[.markdownCodeBlock] as? Bool) == true
        }

        if isCodeBlock {
            if paraRange.length > 0 {
                textView.textStorage.removeAttribute(.markdownCodeBlock, range: paraRange)
                textView.textStorage.removeAttribute(.markdownCodeBlockLang, range: paraRange)
                textView.textStorage.removeAttribute(.backgroundColor, range: paraRange)
                textView.textStorage.addAttribute(.font, value: MarkdownStyle.bodyFont, range: paraRange)
            }
            textView.typingAttributes.removeValue(forKey: .markdownCodeBlock)
            textView.typingAttributes.removeValue(forKey: .markdownCodeBlockLang)
            textView.typingAttributes.removeValue(forKey: .backgroundColor)
            textView.typingAttributes[.font] = MarkdownStyle.bodyFont
        } else {
            if paraRange.length > 0 {
                textView.textStorage.addAttributes([
                    .markdownCodeBlock: true,
                    .font: MarkdownStyle.monospaceFont,
                    .backgroundColor: MarkdownStyle.codeBlockBackground,
                ], range: paraRange)
            }
            textView.typingAttributes[.markdownCodeBlock] = true
            textView.typingAttributes[.font] = MarkdownStyle.monospaceFont
            textView.typingAttributes[.backgroundColor] = MarkdownStyle.codeBlockBackground
        }
        updateState()
        notifyDelegate(textView)
    }

    // MARK: - Blockquote

    @objc private func toggleBlockquote() {
        guard let textView else { return }
        let paraRange = paragraphRange(in: textView)

        let isQuote: Bool
        if textView.textStorage.length > 0, paraRange.location < textView.textStorage.length {
            isQuote = (textView.textStorage.attributes(at: paraRange.location, effectiveRange: nil)[.markdownBlockquote] as? Bool) == true
        } else {
            isQuote = (textView.typingAttributes[.markdownBlockquote] as? Bool) == true
        }

        if isQuote {
            if paraRange.length > 0 {
                textView.textStorage.removeAttribute(.markdownBlockquote, range: paraRange)
                textView.textStorage.addAttribute(.foregroundColor, value: MarkdownStyle.textColor, range: paraRange)
            }
            textView.typingAttributes.removeValue(forKey: .markdownBlockquote)
            textView.typingAttributes[.foregroundColor] = MarkdownStyle.textColor
        } else {
            if paraRange.length > 0 {
                textView.textStorage.addAttributes([
                    .markdownBlockquote: true,
                    .foregroundColor: MarkdownStyle.blockquoteTextColor,
                ], range: paraRange)
            }
            textView.typingAttributes[.markdownBlockquote] = true
            textView.typingAttributes[.foregroundColor] = MarkdownStyle.blockquoteTextColor
        }
        updateState()
        notifyDelegate(textView)
    }

    // MARK: - Image & Dismiss

    @objc private func pickImage() {
        onImagePick?()
    }

    @objc private func dismissKeyboard() {
        textView?.resignFirstResponder()
    }

    // MARK: - Helpers

    private func toggleFontTrait(_ trait: UIFontDescriptor.SymbolicTraits, in textView: UITextView) {
        let range = textView.selectedRange
        let currentFont = (textView.typingAttributes[.font] as? UIFont) ?? MarkdownStyle.bodyFont
        let hasTrait = currentFont.fontDescriptor.symbolicTraits.contains(trait)

        var traits = currentFont.fontDescriptor.symbolicTraits
        if hasTrait { traits.remove(trait) } else { traits.insert(trait) }
        let desc = currentFont.fontDescriptor.withSymbolicTraits(traits) ?? currentFont.fontDescriptor
        let newFont = UIFont(descriptor: desc, size: currentFont.pointSize)

        if range.length > 0 {
            textView.textStorage.addAttribute(.font, value: newFont, range: range)
        }
        textView.typingAttributes[.font] = newFont
        updateState()
        notifyDelegate(textView)
    }

    private func insertOrToggleList(type: String, prefix: String, in textView: UITextView) {
        let storage = textView.textStorage
        let paraRange = paragraphRange(in: textView)
        let currentType: String?
        if storage.length > 0, paraRange.location < storage.length {
            currentType = storage.attributes(at: paraRange.location, effectiveRange: nil)[.markdownListType] as? String
        } else {
            currentType = textView.typingAttributes[.markdownListType] as? String
        }

        if currentType == type {
            if paraRange.length > 0 {
                storage.removeAttribute(.markdownListType, range: paraRange)
                storage.removeAttribute(.markdownListItemIndex, range: paraRange)
                storage.removeAttribute(.markdownListDepth, range: paraRange)
                let style = NSMutableParagraphStyle()
                style.lineSpacing = 4
                storage.addAttribute(.paragraphStyle, value: style, range: paraRange)
            }
            textView.typingAttributes.removeValue(forKey: .markdownListType)
            textView.typingAttributes.removeValue(forKey: .markdownListItemIndex)
            textView.typingAttributes.removeValue(forKey: .markdownListDepth)
        } else {
            let style = NSMutableParagraphStyle()
            style.lineSpacing = 4
            style.headIndent = 24
            style.firstLineHeadIndent = 0

            let listAttrs: [NSAttributedString.Key: Any] = [
                .markdownListType: type,
                .markdownListItemIndex: 0,
                .markdownListDepth: 0,
                .paragraphStyle: style,
            ]

            let paraText = paraRange.length > 0 ? (storage.string as NSString).substring(with: paraRange) : ""
            if paraText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                var insertAttrs = MarkdownStyle.baseAttributes
                insertAttrs.merge(listAttrs) { _, new in new }
                let prefixStr = NSAttributedString(string: prefix, attributes: insertAttrs)

                if paraRange.length > 0 {
                    storage.replaceCharacters(in: paraRange, with: prefixStr)
                } else {
                    storage.insert(prefixStr, at: textView.selectedRange.location)
                }
                textView.selectedRange = NSRange(location: paraRange.location + prefix.count, length: 0)
            } else {
                if paraRange.length > 0 {
                    storage.addAttributes(listAttrs, range: paraRange)
                }
                var insertAttrs = MarkdownStyle.baseAttributes
                insertAttrs.merge(listAttrs) { _, new in new }
                let prefixStr = NSAttributedString(string: prefix, attributes: insertAttrs)
                storage.insert(prefixStr, at: paraRange.location)
                textView.selectedRange = NSRange(location: textView.selectedRange.location + prefix.count, length: 0)
            }

            textView.typingAttributes.merge(listAttrs) { _, new in new }
        }
        updateState()
        notifyDelegate(textView)
    }

    private func paragraphRange(in textView: UITextView) -> NSRange {
        let text = textView.textStorage.string as NSString
        guard text.length > 0 else { return NSRange(location: 0, length: 0) }
        let cursor = textView.selectedRange.location
        if cursor >= text.length {
            let lastChar = text.character(at: text.length - 1)
            if lastChar == 0x0A || lastChar == 0x0D {
                return NSRange(location: text.length, length: 0)
            }
            return text.paragraphRange(for: NSRange(location: text.length - 1, length: 0))
        }
        return text.paragraphRange(for: NSRange(location: cursor, length: 0))
    }

    private func notifyDelegate(_ textView: UITextView) {
        textView.delegate?.textViewDidChange?(textView)
    }
}
