import ExpUI
import SwiftUI
import UIKit

/// A host WRITE into a client-owned field: applied only when the host really
/// changed the value AND it answers the client's latest revision.
struct VappHostWrite: Equatable {
    let value: String
    let revision: Int
}

/// VAPP-4 typing fix (candidate 2, FINDINGS-ios.md): a UITextField that OWNS
/// its text. SwiftUI never pushes a `text` binding into it, so a re-render
/// mid-burst cannot lose or overwrite a keystroke. Every edit is reported with
/// a client revision; `updateUIView` applies a host write only under the rule
/// above, and an echo of the value the field already holds is never written.
struct VappOwnedTextField: UIViewRepresentable {
    let placeholder: String
    let accessibilityIdentifier: String
    /// The host's pending write (nil = none). Read here so SwiftUI calls
    /// `updateUIView` when it changes.
    let hostWrite: VappHostWrite?
    /// (text, client revision) after every UIKit edit.
    let onEdit: (String, Int) -> Void
    @Binding var focused: Bool

    final class Coordinator: NSObject, UITextFieldDelegate {
        var parent: VappOwnedTextField
        var revision = 0

        init(_ parent: VappOwnedTextField) {
            self.parent = parent
        }

        @objc func editingChanged(_ field: UITextField) {
            revision += 1
            parent.onEdit(field.text ?? "", revision)
        }

        func textFieldDidBeginEditing(_ textField: UITextField) {
            if !parent.focused { parent.focused = true }
        }

        func textFieldDidEndEditing(_ textField: UITextField) {
            if parent.focused { parent.focused = false }
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

    func makeUIView(context: Context) -> UITextField {
        let field = UITextField()
        field.delegate = context.coordinator
        field.addTarget(context.coordinator, action: #selector(Coordinator.editingChanged(_:)), for: .editingChanged)
        field.font = UIFont.preferredFont(forTextStyle: .subheadline)
        field.adjustsFontForContentSizeCategory = true
        field.textColor = .white
        field.tintColor = .white
        field.attributedPlaceholder = NSAttributedString(
            string: placeholder,
            attributes: [.foregroundColor: UIColor(DesignTokens.Palette.mutedForeground)]
        )
        field.autocapitalizationType = .none
        field.autocorrectionType = .no
        field.spellCheckingType = .no
        field.smartQuotesType = .no
        field.smartDashesType = .no
        field.smartInsertDeleteType = .no
        field.accessibilityIdentifier = accessibilityIdentifier
        field.accessibilityLabel = placeholder
        field.setContentHuggingPriority(.defaultLow, for: .horizontal)
        field.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        return field
    }

    func updateUIView(_ field: UITextField, context: Context) {
        context.coordinator.parent = self
        guard let write = hostWrite,
              write.revision == context.coordinator.revision,
              write.value != (field.text ?? "")
        else { return }
        // A real host change on the latest revision: keep the caret at the end
        // only if it was there.
        let atEnd = field.selectedTextRange.map { field.compare($0.end, to: field.endOfDocument) == .orderedSame } ?? true
        let offset = field.selectedTextRange.map { field.offset(from: field.beginningOfDocument, to: $0.end) } ?? 0
        field.text = write.value
        let position = atEnd
            ? field.endOfDocument
            : field.position(from: field.beginningOfDocument, offset: min(offset, write.value.count)) ?? field.endOfDocument
        field.selectedTextRange = field.textRange(from: position, to: position)
    }

    func sizeThatFits(_ proposal: ProposedViewSize, uiView: UITextField, context: Context) -> CGSize? {
        let height = ceil(uiView.font?.lineHeight ?? 20) + 2
        return CGSize(width: proposal.width ?? 200, height: height)
    }
}
