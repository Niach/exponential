import ExpUI
import os
import UIKit

private let log = Logger(subsystem: "com.exponential", category: "MarkdownToolbar")

/// The keyboard-accessory formatting rail (EXP-568). One strip, three
/// mutually-exclusive content sets — the canonical shape shared with web,
/// desktop and Android:
///
/// - `.main` — insertion affordances (emoji, image, `#`, link) and the block
///   formats (text-format entry, lists, quote, code), plus the pinned
///   keyboard-down chevron.
/// - `.text` — the inline/heading surface: Text · H1 · H2 · H3 | B · I · S ·
///   clear.
/// - `.link` — a URL field with cancel/confirm.
///
/// Every mode keeps the SAME height: `UIInputView` self-sizing fights a
/// changing accessory height, and the keyboard jumps if it wins.
final class MarkdownToolbar: UIInputView {
    enum RailMode { case main, text, link }

    weak var textView: UITextView? {
        didSet {
            // A new host means whatever mode the rail was in belongs to a text
            // view that is gone.
            guard textView !== oldValue else { return }
            setMode(.main, animated: false)
        }
    }

    var onImagePick: (() -> Void)?
    /// EXP-551 — tapped emoji button. The host presents the picker sheet (the
    /// toolbar is UIKit chrome and owns no presentation context).
    var onEmoji: (() -> Void)?
    /// EXP-327: non-nil turns the image button into a "Photo library / Files"
    /// menu — the ONE attach affordance on the screen, which is why the Files
    /// section no longer carries a paperclip of its own. Nil keeps the plain
    /// one-tap image button (the comment composer, whose host has nowhere to
    /// put an attachment). Assign before the button is built, or call
    /// `refreshImageButtonMenu()`.
    var onFilePick: (() -> Void)? {
        didSet { refreshImageButtonMenu() }
    }

    // Main rail
    private var imageButton: UIButton!
    private var linkButton: UIButton!
    private var textFormatButton: UIButton!
    private var listsButton: UIButton!
    private var codeButton: UIButton!
    private var quoteButton: UIButton!
    private var dismissButton: UIButton!
    private var dismissWidthConstraint: NSLayoutConstraint?

    // Text rail
    private var bodyButton: UIButton!
    private var headingButtons: [Int: UIButton] = [:]
    private var boldButton: UIButton!
    private var italicButton: UIButton!
    private var strikeButton: UIButton!

    // Link rail
    private var urlField: UITextField!
    /// The selection the URL applies to, captured on entry: the text view stops
    /// being first responder while the field is up, and UIKit does not promise
    /// to keep `selectedRange` meaningful across that.
    private var linkTargetRange = NSRange(location: 0, length: 0)
    /// Set while confirm/cancel hands the responder back, so the field's own
    /// `didEndEditing` does not race the transition it is part of.
    private var isFinishingLink = false

    private var mainStack: UIStackView!
    private var textStack: UIStackView!
    private var linkStack: UIStackView!
    private var scroll: UIScrollView!
    private(set) var mode: RailMode = .main

    // Coalesced state-refresh bookkeeping so we don't recompute typing
    // attributes on every caret movement.
    private struct ToolbarState: Equatable {
        var bullet = false, ordered = false, checklist = false, code = false, quote = false
        var bold = false, italic = false, strike = false
        var headingLevel = 0
        var hasLink = false
    }
    private var lastState: ToolbarState?
    private var updateScheduled = false

    private let iconPointSize: CGFloat = 18
    private let inactiveTint = UIColor.white.withAlphaComponent(0.65)

    init() {
        super.init(frame: CGRect(x: 0, y: 0, width: UIScreen.main.bounds.width, height: 48),
                   inputViewStyle: .keyboard)
        allowsSelfSizing = true
        setupBar()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError() }

    // MARK: - Construction

    private func setupBar() {
        // Glass, not a flat fill: the rail sits on the keyboard's own material
        // and an opaque pill read as a foreign slab against it (EXP-568).
        let blur = UIVisualEffectView(effect: UIBlurEffect(style: .systemThinMaterialDark))
        blur.layer.cornerRadius = 10
        blur.clipsToBounds = true
        blur.translatesAutoresizingMaskIntoConstraints = false
        addSubview(blur)
        let pill = blur.contentView

        // Lucide has no keyboard glyph — the chevron reads as "collapse".
        dismissButton = makeButton(AppIcons.uiChevronDown, #selector(dismissKeyboard))
        dismissButton.accessibilityLabel = "Hide keyboard"
        // Kept as a stored constraint so the button can collapse out of the way
        // in the text/link rails instead of leaving 36pt of dead space.
        dismissWidthConstraint = dismissButton.constraints.first { $0.firstAttribute == .width }

        scroll = UIScrollView()
        scroll.showsHorizontalScrollIndicator = false
        scroll.translatesAutoresizingMaskIntoConstraints = false

        mainStack = makeMainStack()
        textStack = makeTextStack()
        linkStack = makeLinkStack()

        // ONE outer stack: a hidden arranged subview collapses to zero width,
        // which is what makes the mode swap a plain `isHidden` animation.
        let rail = UIStackView(arrangedSubviews: [mainStack, textStack, linkStack])
        rail.axis = .horizontal
        rail.alignment = .center
        rail.spacing = 0
        rail.translatesAutoresizingMaskIntoConstraints = false

        scroll.addSubview(rail)
        pill.addSubview(scroll)
        pill.addSubview(dismissButton)

        // The link rail fills the visible width rather than hugging its
        // content — a URL field the width of its placeholder is useless.
        // Below the stack's own hidden-collapse priority so it yields in the
        // other two modes.
        let linkWidth = linkStack.widthAnchor.constraint(equalTo: scroll.frameLayoutGuide.widthAnchor)
        linkWidth.priority = .defaultHigh

        NSLayoutConstraint.activate([
            blur.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 6),
            blur.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -6),
            blur.topAnchor.constraint(equalTo: topAnchor, constant: 4),
            blur.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -4),
            blur.heightAnchor.constraint(equalToConstant: 40),

            dismissButton.trailingAnchor.constraint(equalTo: pill.trailingAnchor, constant: -2),
            dismissButton.centerYAnchor.constraint(equalTo: pill.centerYAnchor),

            scroll.leadingAnchor.constraint(equalTo: pill.leadingAnchor, constant: 2),
            scroll.trailingAnchor.constraint(equalTo: dismissButton.leadingAnchor, constant: -2),
            scroll.topAnchor.constraint(equalTo: pill.topAnchor),
            scroll.bottomAnchor.constraint(equalTo: pill.bottomAnchor),

            rail.leadingAnchor.constraint(equalTo: scroll.contentLayoutGuide.leadingAnchor),
            rail.trailingAnchor.constraint(equalTo: scroll.contentLayoutGuide.trailingAnchor),
            rail.centerYAnchor.constraint(equalTo: scroll.centerYAnchor),
            rail.heightAnchor.constraint(equalTo: scroll.heightAnchor),
            linkWidth,
        ])

        applyModeVisibility()
    }

    private func makeMainStack() -> UIStackView {
        // EXP-551 — opens the emoji picker sheet; the typed `:shortcode`
        // typeahead stays available independently.
        let emojiButton = makeButton(AppIcons.editorEmoji, #selector(pickEmoji))
        emojiButton.accessibilityLabel = "Insert emoji"
        imageButton = makeButton(AppIcons.editorImage, #selector(pickImage))
        imageButton.accessibilityLabel = "Insert image"
        let hashButton = makeButton(AppIcons.editorIssueRef, #selector(insertIssueRef))
        hashButton.accessibilityLabel = "Link an issue"
        linkButton = makeButton(AppIcons.editorLink, #selector(enterLinkMode))
        linkButton.accessibilityLabel = "Link"
        textFormatButton = makeButton(AppIcons.editorTextFormat, #selector(enterTextMode))
        textFormatButton.accessibilityLabel = "Text formatting"
        listsButton = makeButton(AppIcons.editorList, #selector(toggleBulletList))
        listsButton.accessibilityLabel = "Lists"
        quoteButton = makeButton(AppIcons.editorQuote, #selector(toggleBlockquote))
        quoteButton.accessibilityLabel = "Quote"
        codeButton = makeButton(AppIcons.editorCode, #selector(toggleCode))
        codeButton.accessibilityLabel = "Code block"

        refreshImageButtonMenu()
        refreshListsButtonMenu()

        return makeStack([
            emojiButton, imageButton, hashButton, linkButton,
            makeSep(),
            textFormatButton, listsButton, quoteButton, codeButton,
        ])
    }

    private func makeTextStack() -> UIStackView {
        let back = makeButton(AppIcons.uiBack, #selector(enterMainMode))
        back.accessibilityLabel = "Back"
        bodyButton = makeButton(AppIcons.editorText, #selector(applyBody))
        bodyButton.accessibilityLabel = "Body text"
        let h1 = makeButton(AppIcons.editorHeading1, #selector(applyHeading1))
        h1.accessibilityLabel = "Heading 1"
        let h2 = makeButton(AppIcons.editorHeading2, #selector(applyHeading2))
        h2.accessibilityLabel = "Heading 2"
        let h3 = makeButton(AppIcons.editorHeading3, #selector(applyHeading3))
        h3.accessibilityLabel = "Heading 3"
        headingButtons = [1: h1, 2: h2, 3: h3]
        boldButton = makeButton(AppIcons.editorBold, #selector(toggleBold))
        boldButton.accessibilityLabel = "Bold"
        italicButton = makeButton(AppIcons.editorItalic, #selector(toggleItalic))
        italicButton.accessibilityLabel = "Italic"
        strikeButton = makeButton(AppIcons.editorStrikethrough, #selector(toggleStrikethrough))
        strikeButton.accessibilityLabel = "Strikethrough"
        let clear = makeButton(AppIcons.editorClearFormatting, #selector(clearFormatting))
        clear.accessibilityLabel = "Clear formatting"

        return makeStack([
            back, bodyButton, h1, h2, h3,
            makeSep(),
            boldButton, italicButton, strikeButton, clear,
        ])
    }

    private func makeLinkStack() -> UIStackView {
        let cancel = makeButton(AppIcons.uiClose, #selector(cancelLink))
        cancel.accessibilityLabel = "Cancel link"
        let confirm = makeButton(AppIcons.uiCheck, #selector(confirmLink))
        confirm.accessibilityLabel = "Apply link"

        let field = UITextField()
        field.borderStyle = .none
        field.backgroundColor = UIColor.white.withAlphaComponent(0.08)
        field.layer.cornerRadius = 8
        field.textColor = .white
        field.tintColor = MarkdownStyle.linkColor
        field.font = .systemFont(ofSize: 15)
        field.attributedPlaceholder = NSAttributedString(
            string: "https://",
            attributes: [.foregroundColor: UIColor.white.withAlphaComponent(0.35)]
        )
        field.keyboardType = .URL
        field.keyboardAppearance = .dark
        field.autocapitalizationType = .none
        field.autocorrectionType = .no
        field.spellCheckingType = .no
        field.returnKeyType = .done
        field.clearButtonMode = .whileEditing
        field.delegate = self
        field.accessibilityLabel = "Link URL"
        // `borderStyle = .none` leaves the text flush against the fill.
        field.leftView = UIView(frame: CGRect(x: 0, y: 0, width: 8, height: 1))
        field.leftViewMode = .always
        field.translatesAutoresizingMaskIntoConstraints = false
        field.heightAnchor.constraint(equalToConstant: 30).isActive = true
        field.setContentHuggingPriority(.defaultLow, for: .horizontal)
        field.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        urlField = field

        let stack = makeStack([cancel, field, confirm])
        stack.spacing = 4
        return stack
    }

    private func makeStack(_ views: [UIView]) -> UIStackView {
        let stack = UIStackView(arrangedSubviews: views)
        stack.axis = .horizontal
        stack.alignment = .center
        stack.spacing = 2
        stack.translatesAutoresizingMaskIntoConstraints = false
        return stack
    }

    // Registry icons are vector imagesets, not symbols — the point size is
    // baked in by AppIcons.uiImage instead of a SymbolConfiguration.
    private func makeButton(_ icon: String, _ action: Selector) -> UIButton {
        let b = UIButton(type: .system)
        b.setImage(AppIcons.uiImage(icon, pointSize: iconPointSize), for: .normal)
        b.tintColor = inactiveTint
        b.addTarget(self, action: action, for: .touchUpInside)
        b.translatesAutoresizingMaskIntoConstraints = false
        b.widthAnchor.constraint(equalToConstant: 36).isActive = true
        b.heightAnchor.constraint(equalToConstant: 36).isActive = true
        return b
    }

    private func makeSep() -> UIView {
        let sep = UIView()
        sep.backgroundColor = UIColor.white.withAlphaComponent(0.12)
        sep.translatesAutoresizingMaskIntoConstraints = false
        sep.widthAnchor.constraint(equalToConstant: 1).isActive = true
        sep.heightAnchor.constraint(equalToConstant: 18).isActive = true
        return sep
    }

    // MARK: - Modes

    private func setMode(_ newMode: RailMode, animated: Bool = true) {
        guard mode != newMode else { return }
        mode = newMode
        let apply = { [weak self] in
            self?.applyModeVisibility()
            self?.layoutIfNeeded()
        }
        if animated, !UIAccessibility.isReduceMotionEnabled {
            UIView.animate(withDuration: 0.25, animations: apply)
        } else {
            apply()
        }
        // A rail scrolled halfway along would otherwise open the next mode
        // mid-strip.
        scroll.setContentOffset(.zero, animated: false)
    }

    private func applyModeVisibility() {
        setHidden(mainStack, mode != .main)
        setHidden(textStack, mode != .text)
        setHidden(linkStack, mode != .link)
        let hideDismiss = mode != .main
        setHidden(dismissButton, hideDismiss)
        dismissWidthConstraint?.constant = hideDismiss ? 0 : 36
    }

    /// Guarded: re-setting `isHidden` on a stack's arranged subview double-fires
    /// its internal hide constraint and the animation stutters.
    private func setHidden(_ view: UIView, _ hidden: Bool) {
        if view.isHidden != hidden { view.isHidden = hidden }
        view.alpha = hidden ? 0 : 1
    }

    @objc private func enterTextMode() {
        setMode(.text)
        lastState = nil
        updateState()
    }

    @objc private func enterMainMode() {
        setMode(.main)
    }

    /// The host text view stopped editing (keyboard dismissed, view popped).
    /// Link mode is exempt: there the text view resigned precisely BECAUSE the
    /// URL field took over.
    func handleHostEndedEditing() {
        guard mode != .link else { return }
        setMode(.main, animated: false)
    }

    // MARK: - State

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
        let storage = textView.textStorage
        let selection = textView.selectedRange

        let listType = attrs[.markdownListType] as? String
        let paragraph = (storage.string as NSString).safeParagraphRange(at: selection.location)
        let headingLevel: Int
        if let paraAttrs = storage.attributesIfInBounds(at: paragraph.location) {
            headingLevel = (paraAttrs[.markdownHeadingLevel] as? Int) ?? 0
        } else {
            headingLevel = (attrs[.markdownHeadingLevel] as? Int) ?? 0
        }

        var state = ToolbarState(
            bullet: listType == "bullet",
            ordered: listType == "ordered",
            checklist: listType == "checklist",
            code: (attrs[.markdownCodeBlock] as? Bool) == true,
            quote: (attrs[.markdownBlockquote] as? Bool) == true
        )
        state.headingLevel = headingLevel
        if selection.length > 0 {
            state.bold = MarkdownFormatOps.isBold(storage, range: selection)
            state.italic = MarkdownFormatOps.isItalic(storage, range: selection)
            state.strike = MarkdownFormatOps.isStrikethrough(storage, range: selection)
        } else {
            let font = attrs[.font] as? UIFont
            // Bold inside a heading is dropped by the serializer, so the button
            // must not claim it is on.
            state.bold = expFontHasBold(font) && headingLevel == 0
            state.italic = expFontHasItalic(font)
            state.strike = (attrs[.markdownStrikethrough] as? Bool) == true
        }
        state.hasLink = MarkdownFormatOps.hasLink(storage, range: selection)

        guard state != lastState else { return }
        lastState = state

        setActive(listsButton, state.bullet || state.ordered || state.checklist)
        setActive(codeButton, state.code)
        setActive(quoteButton, state.quote)
        setActive(linkButton, state.hasLink)
        setActive(bodyButton, state.headingLevel == 0)
        for (level, button) in headingButtons {
            setActive(button, state.headingLevel == level)
        }
        setActive(boldButton, state.bold)
        setActive(italicButton, state.italic)
        setActive(strikeButton, state.strike)
    }

    private func setActive(_ button: UIButton, _ active: Bool) {
        button.tintColor = active ? MarkdownStyle.linkColor : inactiveTint
    }

    // MARK: - Issue refs / emoji

    // insertText is equivalent to typing: it runs the full delegate chain
    // (textViewDidChange → model.updateText → recomputeAutocomplete), so the
    // existing # autocomplete bar pops with an empty query.

    @objc private func insertIssueRef() {
        textView?.insertText("#")
    }

    // Unlike #, emoji has no useful "type the trigger" affordance — a bare `:`
    // needs two more characters before it means anything — so the button opens
    // the full picker instead (EXP-551).
    @objc private func pickEmoji() {
        onEmoji?()
    }

    // MARK: - Inline marks

    @objc private func toggleBold() {
        applyInlineMark(
            selection: { MarkdownFormatOps.toggleBold(in: $0, range: $1) },
            typing: MarkdownFormatOps.togglingBold
        )
    }

    @objc private func toggleItalic() {
        applyInlineMark(
            selection: { MarkdownFormatOps.toggleItalic(in: $0, range: $1) },
            typing: MarkdownFormatOps.togglingItalic
        )
    }

    @objc private func toggleStrikethrough() {
        applyInlineMark(
            selection: { MarkdownFormatOps.toggleStrikethrough(in: $0, range: $1) },
            typing: MarkdownFormatOps.togglingStrikethrough
        )
    }

    /// A selection is marked in place; a collapsed caret arms the mark on the
    /// typing attributes instead, exactly like the keyboard's own bold.
    private func applyInlineMark(
        selection: (NSMutableAttributedString, NSRange) -> Void,
        typing: ([NSAttributedString.Key: Any]) -> [NSAttributedString.Key: Any]
    ) {
        guard let textView else { return }
        let range = textView.selectedRange
        if range.length > 0 {
            let storage = textView.textStorage
            storage.beginEditing()
            selection(storage, range)
            storage.endEditing()
            textView.selectedRange = range
        } else {
            textView.typingAttributes = typing(textView.typingAttributes)
        }
        // Typing attributes can change without a selection change, which the
        // coalesced comparison would otherwise swallow.
        lastState = nil
        refresh(textView)
    }

    @objc private func clearFormatting() {
        guard let textView else { return }
        let storage = textView.textStorage
        let range = textView.selectedRange
        storage.beginEditing()
        let restored: NSRange
        if range.length > 0 {
            restored = MarkdownFormatOps.clearFormatting(in: storage, range: range)
        } else {
            // Nothing selected: the marks live on the typing attributes, so the
            // only thing left to clear in the document is the line format —
            // the paragraph's inline marks stay put (EXP-571).
            restored = MarkdownFormatOps.setHeading(
                level: 0, in: storage, range: range, selection: range)
        }
        storage.endEditing()
        restoreSelection(restored, in: textView)
        textView.typingAttributes = MarkdownFormatOps.clearedTypingAttributes()
        lastState = nil
        refresh(textView)
    }

    // MARK: - Headings

    @objc private func applyBody() { applyHeading(0) }
    @objc private func applyHeading1() { applyHeading(1) }
    @objc private func applyHeading2() { applyHeading(2) }
    @objc private func applyHeading3() { applyHeading(3) }

    private func applyHeading(_ level: Int) {
        guard let textView else { return }
        let storage = textView.textStorage
        let paragraph = paragraphRange(in: textView)
        let current = MarkdownFormatOps.headingLevel(storage, at: paragraph.location)
        // Tapping the active level reverts to body, like every other client.
        let target = current == level ? 0 : level
        let selection = textView.selectedRange

        storage.beginEditing()
        // EXP-571: every paragraph the selection touches, like web/Android;
        // the returned selection is already shifted past any dropped baked
        // list prefix, so the caret does not drift.
        let restored = MarkdownFormatOps.setHeading(
            level: target, in: storage, range: selection, selection: selection)
        storage.endEditing()
        restoreSelection(restored, in: textView)
        textView.typingAttributes = MarkdownFormatOps.headingTypingAttributes(
            level: target, from: textView.typingAttributes)
        lastState = nil
        refresh(textView)
    }

    // MARK: - Links

    @objc private func enterLinkMode() {
        guard let textView else { return }
        var range = textView.selectedRange
        // A caret parked inside an existing link retargets the WHOLE link
        // rather than splitting it in two.
        if range.length == 0,
           let existing = MarkdownFormatOps.linkRange(textView.textStorage, at: max(0, range.location - 1)) {
            range = existing
        }
        linkTargetRange = range
        urlField.text = MarkdownFormatOps.linkURL(textView.textStorage, at: range) ?? ""
        setMode(.link)
        // Assigned BEFORE becoming first responder: UIKit reads the incoming
        // responder's accessory as it swaps, and this rail IS that accessory —
        // without it the strip is torn down and rebuilt empty.
        urlField.inputAccessoryView = self
        urlField.becomeFirstResponder()
    }

    @objc private func cancelLink() {
        finishLinkMode(restoringSelection: linkTargetRange)
    }

    @objc private func confirmLink() {
        guard let textView else {
            finishLinkMode(restoringSelection: nil)
            return
        }
        let raw = (urlField.text ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        let storage = textView.textStorage
        var applied = linkTargetRange
        storage.beginEditing()
        if raw.isEmpty {
            // An emptied field is how you REMOVE a link.
            MarkdownFormatOps.removeLink(in: storage, range: linkTargetRange)
        } else {
            applied = MarkdownFormatOps.applyLink(
                normalizedURL(raw), in: storage, range: linkTargetRange)
        }
        storage.endEditing()
        finishLinkMode(restoringSelection: NSRange(location: NSMaxRange(applied), length: 0))
        refresh(textView)
    }

    private func finishLinkMode(restoringSelection range: NSRange?) {
        isFinishingLink = true
        defer { isFinishingLink = false }
        setMode(.main)
        if let textView, textView.becomeFirstResponder() {
            if let range { restoreSelection(range, in: textView) }
        } else {
            urlField.resignFirstResponder()
        }
        urlField.inputAccessoryView = nil
        lastState = nil
        updateState()
    }

    /// Bare hosts and `example.com` are what people actually type; a link
    /// without a scheme is not a link on any client.
    private func normalizedURL(_ raw: String) -> String {
        if raw.contains("://") { return raw }
        if raw.hasPrefix("mailto:") || raw.hasPrefix("tel:") { return raw }
        // Relative attachment URLs (`/api/attachments/…`) are the interchange
        // form and must stay relative.
        if raw.hasPrefix("/") || raw.hasPrefix("#") { return raw }
        return "https://\(raw)"
    }

    // MARK: - Lists

    /// The three list kinds share ONE button: a menu keeps the main rail short
    /// enough to hold the block formats too (same pattern as the image button).
    private func refreshListsButtonMenu() {
        guard let listsButton else { return }
        listsButton.menu = UIMenu(children: [
            UIAction(
                title: "List",
                image: AppIcons.uiImage(AppIcons.editorList, pointSize: 16)
            ) { [weak self] _ in self?.toggleBulletList() },
            UIAction(
                title: "Numbered list",
                image: AppIcons.uiImage(AppIcons.editorListOrdered, pointSize: 16)
            ) { [weak self] _ in self?.toggleOrderedList() },
            UIAction(
                title: "Checklist",
                image: AppIcons.uiImage(AppIcons.editorListTodo, pointSize: 16)
            ) { [weak self] _ in self?.toggleChecklist() },
        ])
        listsButton.showsMenuAsPrimaryAction = true
    }

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

    /// With a file handler installed the image button presents a menu instead of
    /// firing the photo picker directly (EXP-327). `showsMenuAsPrimaryAction`
    /// keeps it a single tap — no long press to discover.
    private func refreshImageButtonMenu() {
        guard let imageButton else { return }
        guard onFilePick != nil else {
            imageButton.menu = nil
            imageButton.showsMenuAsPrimaryAction = false
            return
        }
        imageButton.menu = UIMenu(children: [
            UIAction(
                title: "Files",
                image: AppIcons.uiImage(AppIcons.uiAttach, pointSize: 16)
            ) { [weak self] _ in self?.onFilePick?() },
            UIAction(
                title: "Photo library",
                image: AppIcons.uiImage(AppIcons.editorImage, pointSize: 16)
            ) { [weak self] _ in self?.onImagePick?() },
        ])
        imageButton.showsMenuAsPrimaryAction = true
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

    /// Re-applies a selection captured before an edit, clamped to whatever the
    /// document length is now.
    private func restoreSelection(_ range: NSRange, in textView: UITextView) {
        let length = textView.textStorage.length
        let location = min(max(0, range.location), length)
        textView.selectedRange = NSRange(
            location: location, length: min(range.length, length - location))
    }

    private func refresh(_ textView: UITextView) {
        updateState()
        textView.delegate?.textViewDidChange?(textView)
    }
}

// MARK: - URL field

extension MarkdownToolbar: UITextFieldDelegate {
    func textFieldShouldReturn(_ textField: UITextField) -> Bool {
        confirmLink()
        return false
    }

    func textFieldDidEndEditing(_ textField: UITextField) {
        // The keyboard went away under the field (or focus moved elsewhere)
        // without confirm/cancel: leave the URL unapplied, but do not strand
        // the rail in link mode.
        guard !isFinishingLink, mode == .link else { return }
        setMode(.main)
        urlField.inputAccessoryView = nil
    }
}
