import SwiftUI

/// The host-owned text input (the VAPP-4 verdict): a UIKit / AppKit text
/// view that OWNS its text. SwiftUI never pushes a binding into it, so a
/// re-render mid-burst cannot lose or overwrite a keystroke. Every edit is
/// reported to the model with a client revision; a model write (an echo on
/// the latest revision, a composer clear) lands through `writeGeneration`.
/// Every host field drives it through ONE model path (`Model/Fields.swift`):
/// `Input`/`Textarea` `.field`, the Composer, and the one-line fields inside
/// a native (a NumberField's / ChipInput's `input`, a searchable Select's
/// `search`), whose extra keys (Up / Down step, Backspace on empty removes
/// the last chip) go to `SurfaceModel.fieldKey`. A `focusRequest` naming the
/// field makes it first responder.
struct OwnedTextField {
    let index: Int
    let model: SurfaceModel
    let multiline: Bool
    let placeholder: String
    let font: PlatformFont
    let color: PlatformColor
    let placeholderColor: PlatformColor
    let lineHeight: CGFloat
    let disabled: Bool
    /// Enter submits (a Composer) instead of inserting a newline.
    let submitsOnReturn: Bool
    let accessibilityLabel: String
    /// The owner Input's `type` (catalog enum `inputType`; "" = not an
    /// Input): keyboard, masking, autofill and autocorrection.
    let inputType: String
    /// #EXP-1629: the one-line text's (and placeholder's) alignment; a
    /// `row` Input ends its value (`.natural` = the platform default).
    var textAlignment: NSTextAlignment = .natural

    var secure: Bool { inputType == "password" }

    /// The placeholder's attributes: its colour and font, plus the field's
    /// alignment when it is not the default.
    var placeholderAttributes: [NSAttributedString.Key: Any] {
        var attrs: [NSAttributedString.Key: Any] = [.foregroundColor: placeholderColor, .font: font]
        if textAlignment != .natural {
            let p = NSMutableParagraphStyle()
            p.alignment = textAlignment
            attrs[.paragraphStyle] = p
        }
        return attrs
    }
}

/// What an Input `type` asks of the platform field (Compose `keyboardTypeOf`
/// + autofill hints): the keyboard, the autofill content type, and
/// autocapitalisation / autocorrection OFF for addresses, numbers and secrets.
struct InputTraits: Equatable {
    enum Keyboard: Equatable { case standard, email, url, phone, number, search }
    enum Content: Equatable { case email, url, phone, password }
    var keyboard: Keyboard = .standard
    var content: Content?
    var autocorrect = true
    var secure = false
    /// VAPP-208: focus selects a seeded value, so typing replaces it.
    var selectsOnFocus = false

    init(type: String, kind: FieldKind?) {
        switch type {
        case "email": keyboard = .email; content = .email; autocorrect = false
        case "url": keyboard = .url; content = .url; autocorrect = false
        case "tel": keyboard = .phone; content = .phone; autocorrect = false
        case "password": content = .password; autocorrect = false; secure = true
        case "number": keyboard = .number; autocorrect = false
        case "search": keyboard = .search
        default: break
        }
        switch kind {
        case .number?: keyboard = .number; autocorrect = false; selectsOnFocus = true
        case .search?: keyboard = .search
        default: break
        }
    }
}

/// The driver calls, routed to the field's model state.
@MainActor
extension OwnedTextField {
    var state: (text: String, generation: Int) {
        let f = model.field(index)
        return (f?.text ?? "", f?.writeGeneration ?? 0)
    }

    var kind: FieldKind? { model.node(index).map(FieldKind.init) }

    /// The serial of a focus request naming this field while the model's
    /// focus is still on it.
    var focusSerial: Int? {
        guard let r = model.focusRequest, r.index == index, model.focusedId == r.id else { return nil }
        return r.serial
    }

    func edited(_ text: String) { model.fieldEdited(index, text: text) }

    func focused(_ on: Bool) { model.fieldFocused(index, on) }

    func returned() { model.fieldCommitted(index) }

    /// A key the field does not own (`up`, `down`, `pageup`, `pagedown`,
    /// `backspace` on an empty field): `true` = the model handled it.
    func key(_ name: String, shift: Bool = false) -> Bool { model.fieldKey(index, key: name, shift: shift) }

    /// #EDE-65: a Composer key for the host's menu (`menuKeys`): `true` = taken.
    func menuKey(_ name: String, modified: Bool, composing: Bool) -> Bool { model.composerKey(index, key: name, modified: modified, composing: composing) }
}

#if canImport(UIKit)
import UIKit

extension OwnedTextField: UIViewRepresentable {
    final class Coordinator: NSObject, UITextFieldDelegate, UITextViewDelegate {
        var parent: OwnedTextField
        var writeGeneration = -1
        var focusSerial: Int?

        init(_ parent: OwnedTextField) { self.parent = parent }

        @objc func editingChanged(_ field: UITextField) {
            parent.edited(field.text ?? "")
        }

        func textFieldDidBeginEditing(_ textField: UITextField) {
            parent.focused(true)
            // After this turn: the tap that focused the field places its caret
            // once this returns.
            if InputTraits(type: parent.inputType, kind: parent.kind).selectsOnFocus {
                DispatchQueue.main.async { if textField.isFirstResponder { textField.selectAll(nil) } }
            }
        }
        func textFieldDidEndEditing(_ textField: UITextField) { parent.focused(false) }
        func textFieldShouldReturn(_ textField: UITextField) -> Bool {
            parent.returned()
            // A ChipInput keeps the keyboard up for the next chip.
            if parent.kind != .chips { textField.resignFirstResponder() }
            return true
        }

        func textViewDidChange(_ textView: UITextView) { parent.edited(textView.text ?? "") }
        func textViewDidBeginEditing(_ textView: UITextView) { parent.focused(true) }
        func textViewDidEndEditing(_ textView: UITextView) { parent.focused(false) }
        func textView(_ textView: UITextView, shouldChangeTextIn range: NSRange, replacementText text: String) -> Bool {
            if parent.submitsOnReturn, text == "\n" {
                // #EDE-65: with menuKeys on, Return goes to the host's menu.
                if parent.menuKey("Enter", modified: false, composing: textView.markedTextRange != nil) { return false }
                parent.model.composerSubmit(parent.index)
                return false
            }
            return true
        }
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeUIView(context: Context) -> UIView {
        if multiline {
            let view = PlaceholderTextView()
            view.delegate = context.coordinator
            view.backgroundColor = .clear
            view.textContainerInset = .zero
            view.textContainer.lineFragmentPadding = 0
            view.isScrollEnabled = true
            view.alwaysBounceVertical = false
            view.autocorrectionType = .default
            view.placeholderLabel.font = font
            return view
        }
        let field = KeyedTextField()
        field.delegate = context.coordinator
        field.addTarget(context.coordinator, action: #selector(Coordinator.editingChanged(_:)), for: .editingChanged)
        field.backgroundColor = .clear
        field.borderStyle = .none
        field.setContentHuggingPriority(.defaultLow, for: .horizontal)
        field.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        return field
    }

    func updateUIView(_ view: UIView, context: Context) {
        context.coordinator.parent = self
        let (text, generation) = state
        let wantsWrite = context.coordinator.writeGeneration != generation
        context.coordinator.writeGeneration = generation
        if let field = view as? KeyedTextField {
            field.font = font
            field.textColor = color
            field.tintColor = color
            field.isEnabled = !disabled
            field.accessibilityLabel = accessibilityLabel
            field.textAlignment = textAlignment
            field.attributedPlaceholder = NSAttributedString(string: placeholder, attributes: placeholderAttributes)
            Self.apply(InputTraits(type: inputType, kind: kind), to: field)
            let coordinator = context.coordinator
            field.onKey = { name, shift in coordinator.parent.key(name, shift: shift) }
            if wantsWrite, field.text != text {
                field.text = text
                // A model write (an echo, an action's `set`) leaves the cursor at the END.
                field.selectedTextRange = field.textRange(from: field.endOfDocument, to: field.endOfDocument)
            }
        } else if let tv = view as? PlaceholderTextView {
            let coordinator = context.coordinator
            tv.onMenuKey = submitsOnReturn ? { name, modified, composing in coordinator.parent.menuKey(name, modified: modified, composing: composing) } : nil
            tv.font = font
            tv.textColor = color
            tv.tintColor = color
            tv.isEditable = !disabled
            tv.accessibilityLabel = accessibilityLabel
            tv.placeholderLabel.text = placeholder
            tv.placeholderLabel.textColor = placeholderColor
            tv.placeholderLabel.font = font
            if wantsWrite, tv.text != text {
                tv.text = text
                tv.selectedRange = NSRange(location: (text as NSString).length, length: 0)
            }
            tv.placeholderLabel.isHidden = !(tv.text ?? "").isEmpty
            tv.placeholderLabel.frame = CGRect(x: 0, y: 0, width: tv.bounds.width, height: lineHeight)
        }
        if let serial = focusSerial, serial != context.coordinator.focusSerial {
            context.coordinator.focusSerial = serial
            DispatchQueue.main.async { if !view.isFirstResponder { view.becomeFirstResponder() } }
        }
    }

    /// The UIKit side of `InputTraits` (tests read it back).
    static func apply(_ t: InputTraits, to field: UITextField) {
        switch t.keyboard {
        case .standard: field.keyboardType = .default
        case .email: field.keyboardType = .emailAddress
        case .url: field.keyboardType = .URL
        case .phone: field.keyboardType = .phonePad
        case .number: field.keyboardType = .numbersAndPunctuation
        case .search: field.keyboardType = .default
        }
        switch t.content {
        case .email?: field.textContentType = .emailAddress
        case .url?: field.textContentType = .URL
        case .phone?: field.textContentType = .telephoneNumber
        case .password?: field.textContentType = .password
        case nil: field.textContentType = nil
        }
        field.returnKeyType = t.keyboard == .search ? .search : (t.keyboard == .number ? .done : .default)
        field.autocorrectionType = t.autocorrect ? .default : .no
        field.spellCheckingType = t.autocorrect ? .default : .no
        field.autocapitalizationType = t.autocorrect ? .sentences : .none
        field.isSecureTextEntry = t.secure
    }

    func sizeThatFits(_ proposal: ProposedViewSize, uiView: UIView, context: Context) -> CGSize? {
        CGSize(width: proposal.width ?? 200, height: proposal.height ?? lineHeight)
    }

    /// A UITextField that hands the model Backspace in an empty field and
    /// the hardware Up / Down / Page keys (a NumberField steps).
    final class KeyedTextField: UITextField {
        var onKey: ((String, Bool) -> Bool)?

        override func deleteBackward() {
            if (text ?? "").isEmpty, onKey?("backspace", false) == true { return }
            super.deleteBackward()
        }

        override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
            for press in presses {
                guard let key = press.key else { continue }
                let shift = key.modifierFlags.contains(.shift)
                // EXP-1259: Tab and ← / → with the caret at the start reach a
                // search list's field (the model ignores them elsewhere).
                let atStart = selectedTextRange.map { $0.isEmpty && compare($0.start, to: beginningOfDocument) == .orderedSame } ?? false
                let name: String? = switch key.keyCode {
                case .keyboardUpArrow: "ArrowUp"
                case .keyboardDownArrow: "ArrowDown"
                case .keyboardPageUp: "pageup"
                case .keyboardPageDown: "pagedown"
                case .keyboardTab: "Tab"
                case .keyboardLeftArrow where atStart: "leftAtStart"
                case .keyboardRightArrow where atStart: "rightAtStart"
                default: nil
                }
                if let name, onKey?(name, shift) == true { return }
            }
            super.pressesBegan(presses, with: event)
        }
    }

    /// A UITextView with a placeholder label. A Composer's hands the
    /// hardware Up / Down / Return / Tab / Escape to `onMenuKey` first
    /// (#EDE-65 `menuKeys`).
    final class PlaceholderTextView: UITextView {
        let placeholderLabel = UILabel()
        var onMenuKey: ((String, Bool, Bool) -> Bool)?

        override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
            if let onMenuKey {
                for press in presses {
                    guard let key = press.key else { continue }
                    let name: String? = switch key.keyCode {
                    case .keyboardUpArrow: "ArrowUp"
                    case .keyboardDownArrow: "ArrowDown"
                    case .keyboardReturnOrEnter, .keypadEnter: "Enter"
                    case .keyboardTab: "Tab"
                    case .keyboardEscape: "Escape"
                    default: nil
                    }
                    let modified = !key.modifierFlags.intersection([.shift, .control, .alternate, .command]).isEmpty
                    if let name, onMenuKey(name, modified, markedTextRange != nil) { return }
                }
            }
            super.pressesBegan(presses, with: event)
        }

        override init(frame: CGRect, textContainer: NSTextContainer?) {
            super.init(frame: frame, textContainer: textContainer)
            addSubview(placeholderLabel)
            placeholderLabel.isUserInteractionEnabled = false
        }

        required init?(coder: NSCoder) { nil }

        override var text: String! {
            didSet { placeholderLabel.isHidden = !(text ?? "").isEmpty }
        }

        override func layoutSubviews() {
            super.layoutSubviews()
            placeholderLabel.frame = CGRect(x: 0, y: 0, width: bounds.width, height: placeholderLabel.font.lineHeight + 4)
            placeholderLabel.isHidden = !(text ?? "").isEmpty
        }
    }
}

#elseif canImport(AppKit)
import AppKit

extension OwnedTextField: NSViewRepresentable {
    final class Coordinator: NSObject, NSTextFieldDelegate, NSTextViewDelegate {
        var parent: OwnedTextField
        var writeGeneration = -1
        var focusSerial: Int?

        init(_ parent: OwnedTextField) { self.parent = parent }

        func controlTextDidChange(_ obj: Notification) {
            guard let field = obj.object as? NSTextField else { return }
            parent.edited(field.stringValue)
        }

        func controlTextDidBeginEditing(_ obj: Notification) { parent.focused(true) }
        func controlTextDidEndEditing(_ obj: Notification) { parent.focused(false) }

        func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
            switch selector {
            case #selector(NSResponder.insertNewline(_:)):
                parent.returned()
                return true
            case #selector(NSResponder.deleteBackward(_:)) where textView.string.isEmpty:
                return parent.key("backspace")
            case #selector(NSResponder.moveUp(_:)):
                return parent.key("up")
            case #selector(NSResponder.moveDown(_:)):
                return parent.key("down")
            case #selector(NSResponder.moveUpAndModifySelection(_:)):
                return parent.key("up", shift: true)
            case #selector(NSResponder.moveDownAndModifySelection(_:)):
                return parent.key("down", shift: true)
            case #selector(NSResponder.pageUp(_:)), #selector(NSResponder.scrollPageUp(_:)):
                return parent.key("pageup")
            case #selector(NSResponder.pageDown(_:)), #selector(NSResponder.scrollPageDown(_:)):
                return parent.key("pagedown")
            // EXP-1259: a search list's field picks on Tab and leaves a
            // searchable submenu on ← at the caret's start.
            case #selector(NSResponder.insertTab(_:)):
                return parent.key("tab")
            case #selector(NSResponder.moveLeft(_:)) where textView.selectedRange() == NSRange(location: 0, length: 0):
                return parent.key("leftAtStart")
            case #selector(NSResponder.moveRight(_:)) where textView.selectedRange() == NSRange(location: 0, length: 0):
                return parent.key("rightAtStart")
            default:
                return false
            }
        }

        func textDidChange(_ notification: Notification) {
            guard let tv = notification.object as? NSTextView else { return }
            parent.edited(tv.string)
        }

        func textDidBeginEditing(_ notification: Notification) { parent.focused(true) }
        func textDidEndEditing(_ notification: Notification) { parent.focused(false) }

        func textView(_ textView: NSTextView, doCommandBy commandSelector: Selector) -> Bool {
            // #EDE-65: a Composer with menuKeys on hands Up, Down, Return, Tab
            // and Escape (no modifier; never during a composition: AppKit
            // routes those keys to the input method) to the host's menu.
            if parent.submitsOnReturn {
                let name: String? = switch commandSelector {
                case #selector(NSResponder.moveUp(_:)): "ArrowUp"
                case #selector(NSResponder.moveDown(_:)): "ArrowDown"
                case #selector(NSResponder.insertNewline(_:)): "Enter"
                case #selector(NSResponder.insertTab(_:)): "Tab"
                case #selector(NSResponder.cancelOperation(_:)): "Escape"
                default: nil
                }
                let flags = NSApp.currentEvent?.modifierFlags.intersection([.shift, .control, .option, .command]) ?? []
                if let name, parent.menuKey(name, modified: !flags.isEmpty, composing: textView.hasMarkedText()) { return true }
            }
            if parent.submitsOnReturn, commandSelector == #selector(NSResponder.insertNewline(_:)) {
                parent.model.composerSubmit(parent.index)
                return true
            }
            return false
        }
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeNSView(context: Context) -> NSView {
        if multiline {
            // `NSTextView.scrollableTextView()`, with the placeholder-drawing
            // text view (AppKit's NSTextView has no public placeholder; the
            // iOS twin is `PlaceholderTextView`'s label).
            let scroll = NSScrollView()
            let tv = PlaceholderTextView(frame: .zero)
            tv.minSize = .zero
            tv.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
            tv.isVerticallyResizable = true
            tv.isHorizontallyResizable = false
            tv.autoresizingMask = [.width]
            tv.textContainer?.widthTracksTextView = true
            scroll.documentView = tv
            tv.delegate = context.coordinator
            tv.drawsBackground = false
            tv.isRichText = false
            tv.textContainerInset = .zero
            tv.textContainer?.lineFragmentPadding = 0
            scroll.drawsBackground = false
            scroll.hasVerticalScroller = false
            return scroll
        }
        let field = secure ? CenteredSecureTextField() : CenteredTextField()
        if secure { field.contentType = .password }
        field.delegate = context.coordinator
        field.isBordered = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.cell?.wraps = false
        field.cell?.isScrollable = true
        return field
    }

    func updateNSView(_ view: NSView, context: Context) {
        context.coordinator.parent = self
        let (text, generation) = state
        let wantsWrite = context.coordinator.writeGeneration != generation
        context.coordinator.writeGeneration = generation
        if let field = view as? NSTextField {
            field.font = font
            field.textColor = color
            field.isEnabled = !disabled
            (field as? CenteredTextField)?.selectsOnPress = InputTraits(type: inputType, kind: kind).selectsOnFocus
            field.setAccessibilityLabel(accessibilityLabel)
            field.alignment = textAlignment
            field.placeholderAttributedString = NSAttributedString(string: placeholder, attributes: placeholderAttributes)
            if wantsWrite, field.stringValue != text {
                field.stringValue = text
                // A model write (an echo, an action's `set`) leaves the cursor at the END.
                field.currentEditor()?.selectedRange = NSRange(location: (text as NSString).length, length: 0)
            }
        } else if let scroll = view as? NSScrollView, let tv = scroll.documentView as? NSTextView {
            tv.font = font
            tv.textColor = color
            tv.isEditable = !disabled
            if wantsWrite, tv.string != text {
                tv.string = text
                tv.setSelectedRange(NSRange(location: (text as NSString).length, length: 0))
            }
            if let ptv = tv as? PlaceholderTextView {
                ptv.placeholder = placeholder.isEmpty ? nil : NSAttributedString(string: placeholder, attributes: [.foregroundColor: placeholderColor, .font: font])
                ptv.placeholderLineHeight = lineHeight
            }
        }
        if let serial = focusSerial, serial != context.coordinator.focusSerial {
            context.coordinator.focusSerial = serial
            let target: NSView = (view as? NSScrollView)?.documentView ?? view
            DispatchQueue.main.async {
                if let window = target.window, window.firstResponder !== target, (window.firstResponder as? NSText)?.delegate as? NSView !== target {
                    window.makeFirstResponder(target)
                }
            }
        }
    }

    func sizeThatFits(_ proposal: ProposedViewSize, nsView: NSView, context: Context) -> CGSize? {
        CGSize(width: proposal.width ?? 200, height: proposal.height ?? lineHeight)
    }

    /// An NSTextView that draws its placeholder while empty, at the top of
    /// the text, centred in one `placeholderLineHeight` line (the iOS
    /// `PlaceholderTextView` label's frame).
    final class PlaceholderTextView: NSTextView {
        var placeholder: NSAttributedString? { didSet { if placeholder != oldValue { needsDisplay = true } } }
        var placeholderLineHeight: CGFloat = 0 { didSet { if placeholderLineHeight != oldValue { needsDisplay = true } } }

        override var string: String { didSet { needsDisplay = true } }

        override func didChangeText() {
            super.didChangeText()
            needsDisplay = true
        }

        /// The rect the placeholder draws in (nil = none shown).
        var placeholderRect: CGRect? {
            guard string.isEmpty, let placeholder, placeholder.length > 0 else { return nil }
            let natural = ceil(placeholder.size().height)
            let line = max(placeholderLineHeight, natural)
            let inset = textContainerInset
            let pad = textContainer?.lineFragmentPadding ?? 0
            return CGRect(x: inset.width + pad, y: inset.height + (line - natural) / 2, width: max(0, bounds.width - 2 * (inset.width + pad)), height: natural)
        }

        override func draw(_ dirtyRect: NSRect) {
            super.draw(dirtyRect)
            if let rect = placeholderRect, let placeholder {
                placeholder.draw(with: rect, options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
            }
        }
    }

    /// A one-line NSTextField whose text (and placeholder, and field
    /// editor) sit vertically CENTRED in the field box, as UITextField and
    /// the web's input do; AppKit's cell draws at the top.
    final class CenteredTextField: NSTextField {
        /// VAPP-208: the press that focuses the field selects its value (Tab
        /// already does on AppKit).
        var selectsOnPress = false

        override class var cellClass: AnyClass? {
            get { CenteredTextFieldCell.self }
            set {}
        }

        // The field editor's press tracking runs inside `super` until release.
        override func mouseDown(with event: NSEvent) {
            let focusing = currentEditor() == nil
            super.mouseDown(with: event)
            if selectsOnPress, focusing, let editor = currentEditor(), editor.selectedRange.length == 0 { editor.selectAll(nil) }
        }
    }

    final class CenteredSecureTextField: NSSecureTextField {
        override class var cellClass: AnyClass? {
            get { CenteredSecureTextFieldCell.self }
            set {}
        }
    }

    final class CenteredTextFieldCell: NSTextFieldCell {
        override func drawingRect(forBounds rect: NSRect) -> NSRect {
            OwnedTextField.centered(super.drawingRect(forBounds: rect), font: font)
        }

        // The field editor follows the centred line (else the text jumps
        // to the top while editing).
        override func edit(withFrame rect: NSRect, in controlView: NSView, editor textObj: NSText, delegate: Any?, event: NSEvent?) {
            super.edit(withFrame: OwnedTextField.centered(rect, font: font), in: controlView, editor: textObj, delegate: delegate, event: event)
        }

        override func select(withFrame rect: NSRect, in controlView: NSView, editor textObj: NSText, delegate: Any?, start selStart: Int, length selLength: Int) {
            super.select(withFrame: OwnedTextField.centered(rect, font: font), in: controlView, editor: textObj, delegate: delegate, start: selStart, length: selLength)
        }
    }

    final class CenteredSecureTextFieldCell: NSSecureTextFieldCell {
        override func drawingRect(forBounds rect: NSRect) -> NSRect {
            OwnedTextField.centered(super.drawingRect(forBounds: rect), font: font)
        }

        // The field editor follows the centred line (else the text jumps
        // to the top while editing).
        override func edit(withFrame rect: NSRect, in controlView: NSView, editor textObj: NSText, delegate: Any?, event: NSEvent?) {
            super.edit(withFrame: OwnedTextField.centered(rect, font: font), in: controlView, editor: textObj, delegate: delegate, event: event)
        }

        override func select(withFrame rect: NSRect, in controlView: NSView, editor textObj: NSText, delegate: Any?, start selStart: Int, length selLength: Int) {
            super.select(withFrame: OwnedTextField.centered(rect, font: font), in: controlView, editor: textObj, delegate: delegate, start: selStart, length: selLength)
        }
    }

    /// `rect` narrowed to one line of `font`, centred vertically in it.
    static func centered(_ rect: NSRect, font: NSFont?) -> NSRect {
        guard let font else { return rect }
        let line = ceil(NSLayoutManager().defaultLineHeight(for: font))
        guard rect.height > line else { return rect }
        var r = rect
        r.origin.y += floor((rect.height - line) / 2)
        r.size.height = line
        return r
    }
}
#endif
