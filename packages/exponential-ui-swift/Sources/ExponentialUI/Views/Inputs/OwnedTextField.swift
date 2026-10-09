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

    var secure: Bool { inputType == "password" }
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
        case .number?: keyboard = .number; autocorrect = false
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

        func textFieldDidBeginEditing(_ textField: UITextField) { parent.focused(true) }
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
            field.attributedPlaceholder = NSAttributedString(string: placeholder, attributes: [.foregroundColor: placeholderColor, .font: font])
            Self.apply(InputTraits(type: inputType, kind: kind), to: field)
            let coordinator = context.coordinator
            field.onKey = { name, shift in coordinator.parent.key(name, shift: shift) }
            if wantsWrite, field.text != text { field.text = text }
        } else if let tv = view as? PlaceholderTextView {
            tv.font = font
            tv.textColor = color
            tv.tintColor = color
            tv.isEditable = !disabled
            tv.accessibilityLabel = accessibilityLabel
            tv.placeholderLabel.text = placeholder
            tv.placeholderLabel.textColor = placeholderColor
            tv.placeholderLabel.font = font
            if wantsWrite, tv.text != text { tv.text = text }
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
                let name: String? = switch key.keyCode {
                case .keyboardUpArrow: "up"
                case .keyboardDownArrow: "down"
                case .keyboardPageUp: "pageup"
                case .keyboardPageDown: "pagedown"
                default: nil
                }
                if let name, onKey?(name, shift) == true { return }
            }
            super.pressesBegan(presses, with: event)
        }
    }

    /// A UITextView with a placeholder label.
    final class PlaceholderTextView: UITextView {
        let placeholderLabel = UILabel()

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
            let scroll = NSTextView.scrollableTextView()
            let tv = scroll.documentView as! NSTextView
            tv.delegate = context.coordinator
            tv.drawsBackground = false
            tv.isRichText = false
            tv.textContainerInset = .zero
            tv.textContainer?.lineFragmentPadding = 0
            scroll.drawsBackground = false
            scroll.hasVerticalScroller = false
            return scroll
        }
        let field = secure ? NSSecureTextField() : NSTextField()
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
            field.setAccessibilityLabel(accessibilityLabel)
            field.placeholderAttributedString = NSAttributedString(string: placeholder, attributes: [.foregroundColor: placeholderColor, .font: font])
            if wantsWrite, field.stringValue != text { field.stringValue = text }
        } else if let scroll = view as? NSScrollView, let tv = scroll.documentView as? NSTextView {
            tv.font = font
            tv.textColor = color
            tv.isEditable = !disabled
            if wantsWrite, tv.string != text { tv.string = text }
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
}
#endif
