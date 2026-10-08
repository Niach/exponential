import SwiftUI

/// The host-owned text input (the VAPP-4 verdict): a UIKit / AppKit text
/// view that OWNS its text. SwiftUI never pushes a binding into it, so a
/// re-render mid-burst cannot lose or overwrite a keystroke. Every edit is
/// reported to the model with a client revision; a model write (an echo on
/// the latest revision, a composer clear) lands through `writeGeneration`.
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
}

#if canImport(UIKit)
import UIKit

extension OwnedTextField: UIViewRepresentable {
    final class Coordinator: NSObject, UITextFieldDelegate, UITextViewDelegate {
        var parent: OwnedTextField
        var writeGeneration = -1

        init(_ parent: OwnedTextField) { self.parent = parent }

        @objc func editingChanged(_ field: UITextField) {
            parent.model.fieldEdited(parent.index, text: field.text ?? "")
        }

        func textFieldDidBeginEditing(_ textField: UITextField) { parent.model.fieldFocused(parent.index, true) }
        func textFieldDidEndEditing(_ textField: UITextField) { parent.model.fieldFocused(parent.index, false) }
        func textFieldShouldReturn(_ textField: UITextField) -> Bool {
            parent.model.fieldCommitted(parent.index)
            textField.resignFirstResponder()
            return true
        }

        func textViewDidChange(_ textView: UITextView) { parent.model.fieldEdited(parent.index, text: textView.text ?? "") }
        func textViewDidBeginEditing(_ textView: UITextView) { parent.model.fieldFocused(parent.index, true) }
        func textViewDidEndEditing(_ textView: UITextView) { parent.model.fieldFocused(parent.index, false) }
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
        let field = UITextField()
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
        let state = model.field(index)
        let text = state?.text ?? ""
        let generation = state?.writeGeneration ?? 0
        let wantsWrite = context.coordinator.writeGeneration != generation
        context.coordinator.writeGeneration = generation
        if let field = view as? UITextField {
            field.font = font
            field.textColor = color
            field.tintColor = color
            field.isEnabled = !disabled
            field.isSecureTextEntry = secure
            field.accessibilityLabel = accessibilityLabel
            field.attributedPlaceholder = NSAttributedString(string: placeholder, attributes: [.foregroundColor: placeholderColor, .font: font])
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
            parent.model.fieldEdited(parent.index, text: field.stringValue)
        }

        func controlTextDidBeginEditing(_ obj: Notification) { parent.model.fieldFocused(parent.index, true) }
        func controlTextDidEndEditing(_ obj: Notification) { parent.model.fieldFocused(parent.index, false) }

        func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
            if selector == #selector(NSResponder.insertNewline(_:)) {
                parent.model.fieldCommitted(parent.index)
                return false
            }
            return false
        }

        func textDidChange(_ notification: Notification) {
            guard let tv = notification.object as? NSTextView else { return }
            parent.model.fieldEdited(parent.index, text: tv.string)
        }

        func textDidBeginEditing(_ notification: Notification) { parent.model.fieldFocused(parent.index, true) }
        func textDidEndEditing(_ notification: Notification) { parent.model.fieldFocused(parent.index, false) }

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
        let state = model.field(index)
        let text = state?.text ?? ""
        let generation = state?.writeGeneration ?? 0
        let wantsWrite = context.coordinator.writeGeneration != generation
        context.coordinator.writeGeneration = generation
        if let field = view as? NSTextField {
            field.font = font
            field.textColor = color
            field.isEnabled = !disabled
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
