import ExpUI
import SwiftUI

/// The issue's big editable title — ONE field for the issue face and the New
/// issue page (EXP-1170): the face saves on blur/submit, the draft page
/// flushes its autosave there. Always reports its bottom edge
/// (`IssueTitleRowBottomKey`, EXP-1162) so the host's header collapses its
/// title once the row has scrolled under the band.
struct IssueTitleField: View {
    @Binding var text: String
    let placeholder: String
    let focused: FocusState<Bool>.Binding
    var accessibilityIdentifier: String = "issue-title"
    /// The field lost focus.
    var onBlur: () -> Void = {}
    /// Return was pressed.
    var onSubmit: () -> Void = {}

    var body: some View {
        TextField(placeholder, text: $text)
            .font(.title2.weight(.semibold))
            .textFieldStyle(.plain)
            .foregroundStyle(.white)
            .focused(focused)
            .onSubmit(onSubmit)
            .onChange(of: focused.wrappedValue) { _, isFocused in
                if !isFocused { onBlur() }
            }
            .accessibilityIdentifier(accessibilityIdentifier)
            .background {
                GeometryReader { row in
                    Color.clear.preference(
                        key: IssueTitleRowBottomKey.self,
                        value: row.frame(in: .global).maxY
                    )
                }
            }
    }
}
