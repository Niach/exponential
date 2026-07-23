import ExpUI
import os
import UIKit

private let log = Logger(subsystem: "com.exponential", category: "MarkdownToolbar")

final class MarkdownToolbar: UIInputView {
    weak var textView: UITextView?
    var onImagePick: (() -> Void)?
    /// Solo teams hide the @ affordance — there is nobody to mention but
    /// yourself (EXP-246; the typed `@` autocomplete stays functional).
    var showsMentionButton = true {
        didSet { atButton.isHidden = !showsMentionButton }
    }

    private var atButton: UIButton!
    private var bulletListButton: UIButton!
    private var orderedListButton: UIButton!
    private var checklistButton: UIButton!
    private var codeButton: UIButton!
    private var quoteButton: UIButton!

    // Coalesced state-refresh bookkeeping so we don't recompute typing
    // attributes on every caret movement.
    private struct ToolbarState: Equatable {
        var bullet = false, ordered = false, checklist = false, code = false, quote = false
    }
    private var lastState: ToolbarState?
    private var updateScheduled = false

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

        // Linear-style simplified strip (EXP-246): image | @ # | lists, code,
        // quote. Text formatting (heading/bold/italic/strikethrough/link)
        // deliberately has no buttons — inline marks still render and
        // round-trip, only the authoring affordance is gone.
        atButton = makeButton("at", #selector(insertMention))
        let hashButton = makeButton("number", #selector(insertIssueRef))
        bulletListButton = makeButton("list.bullet", #selector(toggleBulletList))
        orderedListButton = makeButton("list.number", #selector(toggleOrderedList))
        checklistButton = makeButton("checklist", #selector(toggleChecklist))
        codeButton = makeButton("chevron.left.forwardslash.chevron.right", #selector(toggleCode))
        quoteButton = makeButton("text.quote", #selector(toggleBlockquote))
        let imageButton = makeButton("photo", #selector(pickImage))
        let dismissButton = makeButton("keyboard.chevron.compact.down", #selector(dismissKeyboard))
        atButton.isHidden = !showsMentionButton

        let scroll = UIScrollView()
        scroll.showsHorizontalScrollIndicator = false
        scroll.translatesAutoresizingMaskIntoConstraints = false

        let stack = UIStackView(arrangedSubviews: [
            imageButton,
            makeSep(),
            atButton, hashButton,
            makeSep(),
            bulletListButton, orderedListButton, checklistButton, codeButton, quoteButton,
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

    /// Coalesced so rapid caret movement does not recompute font traits each
    /// time; runs at most once per runloop and only touches tints when the
    /// derived state actually changes.
    func updateState() {
        guard !updateScheduled else { return }
        updateScheduled = true
        DispatchQueue.main.async { [weak self] in
            self?.updateScheduled = false
            self?.performUpdateState()
        }
    }

    private func performUpdateState() {
        guard let textView else { return }
        let attrs = textView.typingAttributes

        let listType = attrs[.markdownListType] as? String
        let state = ToolbarState(
            bullet: listType == "bullet",
            ordered: listType == "ordered",
            checklist: listType == "checklist",
            code: (attrs[.markdownCodeBlock] as? Bool) == true,
            quote: (attrs[.markdownBlockquote] as? Bool) == true
        )
        guard state != lastState else { return }
        lastState = state

        setActive(bulletListButton, state.bullet)
        setActive(orderedListButton, state.ordered)
        setActive(checklistButton, state.checklist)
        setActive(codeButton, state.code)
        setActive(quoteButton, state.quote)
    }

    private func setActive(_ button: UIButton, _ active: Bool) {
        button.tintColor = active ? MarkdownStyle.linkColor : UIColor.white.withAlphaComponent(0.65)
    }

    // MARK: - Mention / issue-ref triggers

    // insertText is equivalent to typing: it runs the full delegate chain
    // (textViewDidChange → model.updateText → recomputeAutocomplete), so the
    // existing @/# autocomplete bars pop with an empty query.

    @objc private func insertMention() {
        textView?.insertText("@")
    }

    @objc private func insertIssueRef() {
        textView?.insertText("#")
    }

    // MARK: - Lists

    @objc private func toggleBulletList() {
        guard let textView else { return }
        insertOrToggleList(type: "bullet", prefix: "\u{2022} ", initialIndex: 0, in: textView)
    }

    @objc private func toggleOrderedList() {
        guard let textView else { return }
        insertOrToggleList(type: "ordered", prefix: "1. ", initialIndex: 1, in: textView)
    }

    @objc private func toggleChecklist() {
        guard let textView else { return }
        insertOrToggleList(type: "checklist", prefix: "\u{2610} ", initialIndex: 0, in: textView)
    }

    // MARK: - Code block

    @objc private func toggleCode() {
        guard let textView else { return }
        let paraRange = paragraphRange(in: textView)

        let isCodeBlock: Bool
        if let attrs = textView.textStorage.attributesIfInBounds(at: paraRange.location) {
            isCodeBlock = (attrs[.markdownCodeBlock] as? Bool) == true
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
            // No `.backgroundColor`: MarkdownLayoutManager draws the fence as
            // one connected box off `.markdownCodeBlock` (EXP-246).
            if paraRange.length > 0 {
                textView.textStorage.addAttributes([
                    .markdownCodeBlock: true,
                    .font: MarkdownStyle.monospaceFont,
                ], range: paraRange)
            }
            textView.typingAttributes[.markdownCodeBlock] = true
            textView.typingAttributes[.font] = MarkdownStyle.monospaceFont
        }
        refresh(textView)
    }

    // MARK: - Blockquote

    @objc private func toggleBlockquote() {
        guard let textView else { return }
        let paraRange = paragraphRange(in: textView)

        let isQuote: Bool
        if let attrs = textView.textStorage.attributesIfInBounds(at: paraRange.location) {
            isQuote = (attrs[.markdownBlockquote] as? Bool) == true
        } else {
            isQuote = (textView.typingAttributes[.markdownBlockquote] as? Bool) == true
        }

        if isQuote {
            let plainStyle = NSMutableParagraphStyle()
            plainStyle.lineSpacing = 4
            if paraRange.length > 0 {
                textView.textStorage.removeAttribute(.markdownBlockquote, range: paraRange)
                textView.textStorage.addAttributes([
                    .foregroundColor: MarkdownStyle.textColor,
                    .paragraphStyle: plainStyle,
                ], range: paraRange)
            }
            textView.typingAttributes.removeValue(forKey: .markdownBlockquote)
            textView.typingAttributes[.foregroundColor] = MarkdownStyle.textColor
            textView.typingAttributes[.paragraphStyle] = plainStyle
        } else {
            // The indent clears the gutter for the quote bar drawn by
            // MarkdownLayoutManager (EXP-246).
            if paraRange.length > 0 {
                textView.textStorage.addAttributes([
                    .markdownBlockquote: true,
                    .foregroundColor: MarkdownStyle.blockquoteTextColor,
                    .paragraphStyle: MarkdownStyle.blockquoteParagraphStyle,
                ], range: paraRange)
            }
            textView.typingAttributes[.markdownBlockquote] = true
            textView.typingAttributes[.foregroundColor] = MarkdownStyle.blockquoteTextColor
            textView.typingAttributes[.paragraphStyle] = MarkdownStyle.blockquoteParagraphStyle
        }
        refresh(textView)
    }

    // MARK: - Image / Dismiss

    @objc private func pickImage() {
        onImagePick?()
    }

    @objc private func dismissKeyboard() {
        textView?.resignFirstResponder()
    }

    // MARK: - Helpers

    private func insertOrToggleList(type: String, prefix: String, initialIndex: Int, in textView: UITextView) {
        let storage = textView.textStorage
        let paraRange = paragraphRange(in: textView)
        let currentType: String?
        if let attrs = storage.attributesIfInBounds(at: paraRange.location) {
            currentType = attrs[.markdownListType] as? String
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
                .markdownListItemIndex: initialIndex,
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
        refresh(textView)
    }

    private func paragraphRange(in textView: UITextView) -> NSRange {
        let text = textView.textStorage.string as NSString
        return text.safeParagraphRange(at: textView.selectedRange.location)
    }

    private func refresh(_ textView: UITextView) {
        updateState()
        textView.delegate?.textViewDidChange?(textView)
    }
}
