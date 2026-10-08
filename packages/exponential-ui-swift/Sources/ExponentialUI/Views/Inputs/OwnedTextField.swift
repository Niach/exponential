import SwiftUI

/// The host-owned text input (the VAPP-4 verdict): a UIKit / AppKit text
/// view that OWNS its text. SwiftUI never pushes a binding into it, so a
/// re-render mid-burst cannot lose or overwrite a keystroke. Every edit is
/// reported to the model with a client revision; a model write (an echo on
/// the latest revision, a composer clear) lands through `writeGeneration`.
///
/// Two drivers: the surface's text fields (`Input`/`Textarea` `.field`, the
/// Composer: `Model/Fields.swift`) and, with `inline`, the one-line fields
/// INSIDE a round-1 native (a NumberField's / ChipInput's `input`, a
/// searchable Select's `search`: `Model/NativesState.swift`), which also
/// take Backspace-on-empty (remove the last chip) and Up / Down (step).
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
    let secure: Bool
    /// A native's inline field (see above).
    var inline: Bool = false
    /// The platform keyboard for an inline field (numbers, search).
    var keyboard: InlineKeyboard = .text
}

/// The keyboard an inline field asks for.
enum InlineKeyboard {
    case text
    case number
    case search
}

/// The driver calls, routed to the field's model state.
@MainActor
extension OwnedTextField {
    var state: (text: String, generation: Int) {
        if inline {
            let f = model.inlineField(index)
            return (f?.text ?? "", f?.writeGeneration ?? 0)
        }
        let f = model.field(index)
        return (f?.text ?? "", f?.writeGeneration ?? 0)
    }

    func edited(_ text: String) {
        if inline { model.inlineFieldEdited(index, text: text) } else { model.fieldEdited(index, text: text) }
    }

    func focused(_ on: Bool) {
        if inline { model.inlineFieldFocused(index, on) } else { model.fieldFocused(index, on) }
    }

    func returned() {
        if inline { model.inlineFieldReturn(index) } else { model.fieldCommitted(index) }
    }

    /// Backspace with the caret at the start of an empty field.
    func backspaceOnEmpty() -> Bool { inline && model.inlineFieldBackspace(index) }

    /// Up / Down (Page Up / Down) in a NumberField field.
    func arrow(up: Bool, page: Bool) -> Bool { inline && model.inlineFieldArrow(index, up: up, page: page) }
}

#if canImport(UIKit)
import UIKit

extension OwnedTextField: UIViewRepresentable {
    final class Coordinator: NSObject, UITextFieldDelegate, UITextViewDelegate {
        var parent: OwnedTextField
        var writeGeneration = -1

        init(_ parent: OwnedTextField) { self.parent = parent }

        @objc func editingChanged(_ field: UITextField) {
            parent.edited(field.text ?? "")
        }

        func textFieldDidBeginEditing(_ textField: UITextField) { parent.focused(true) }
        func textFieldDidEndEditing(_ textField: UITextField) { parent.focused(false) }
        func textFieldShouldReturn(_ textField: UITextField) -> Bool {
            parent.returned()
            // A ChipInput keeps the keyboard up for the next chip.
            if !parent.inline || parent.keyboard != .text { textField.resignFirstResponder() }
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
            field.isSecureTextEntry = secure
            field.accessibilityLabel = accessibilityLabel
            field.attributedPlaceholder = NSAttributedString(string: placeholder, attributes: [.foregroundColor: placeholderColor, .font: font])
            switch keyboard {
            case .number:
                field.keyboardType = .numbersAndPunctuation
                field.returnKeyType = .done
            case .search:
                field.returnKeyType = .search
            case .text:
                break
            }
            let coordinator = context.coordinator
            field.onBackspaceEmpty = { coordinator.parent.backspaceOnEmpty() }
            field.onArrow = { up, page in coordinator.parent.arrow(up: up, page: page) }
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
    }

    func sizeThatFits(_ proposal: ProposedViewSize, uiView: UIView, context: Context) -> CGSize? {
        CGSize(width: proposal.width ?? 200, height: proposal.height ?? lineHeight)
    }

    /// A UITextField that reports Backspace in an empty field and the
    /// hardware arrow keys (a NumberField steps on Up / Down).
    final class KeyedTextField: UITextField {
        var onBackspaceEmpty: (() -> Bool)?
        var onArrow: ((Bool, Bool) -> Bool)?

        override func deleteBackward() {
            if (text ?? "").isEmpty, onBackspaceEmpty?() == true { return }
            super.deleteBackward()
        }

        override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
            for press in presses {
                guard let key = press.key else { continue }
                let handled: Bool
                switch key.keyCode {
                case .keyboardUpArrow: handled = onArrow?(true, key.modifierFlags.contains(.shift)) ?? false
                case .keyboardDownArrow: handled = onArrow?(false, key.modifierFlags.contains(.shift)) ?? false
                case .keyboardPageUp: handled = onArrow?(true, true) ?? false
                case .keyboardPageDown: handled = onArrow?(false, true) ?? false
                default: handled = false
                }
                if handled { return }
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
                return parent.inline
            case #selector(NSResponder.deleteBackward(_:)) where textView.string.isEmpty:
                return parent.backspaceOnEmpty()
            case #selector(NSResponder.moveUp(_:)):
                return parent.arrow(up: true, page: false)
            case #selector(NSResponder.moveDown(_:)):
                return parent.arrow(up: false, page: false)
            case #selector(NSResponder.pageUp(_:)), #selector(NSResponder.scrollPageUp(_:)):
                return parent.arrow(up: true, page: true)
            case #selector(NSResponder.pageDown(_:)), #selector(NSResponder.scrollPageDown(_:)):
                return parent.arrow(up: false, page: true)
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
    }

    func sizeThatFits(_ proposal: ProposedViewSize, nsView: NSView, context: Context) -> CGSize? {
        CGSize(width: proposal.width ?? 200, height: proposal.height ?? lineHeight)
    }
}
#endif
