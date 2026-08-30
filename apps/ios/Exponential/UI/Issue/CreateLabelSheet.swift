import ExpUI
import ExpCore
import SwiftUI

// MARK: - Label editor

// Same suggested palette as Android's LabelPickerSheet / the web's label editor.
let suggestedLabelColors = [
    "#ef4444", "#dc2626", "#f97316", "#f59e0b", "#eab308",
    "#84cc16", "#22c55e", "#10b981", "#14b8a6", "#06b6d4",
    "#0ea5e9", "#3b82f6", "#6366f1", "#8b5cf6", "#a855f7",
    "#ec4899", "#f43f5e", "#78716c", "#64748b", "#a3a3a3",
]

/// Minimal name + color form — the iOS analog of Android's LabelEditorDialog
/// (EXP-331: same titles and button wordings on both platforms). Create flows
/// pass "New label"/"Create", the team-settings edit flow passes
/// "Edit label"/"Save" with the label's current name/color prefilled; the
/// caller's `onConfirm` closure decides what happens with the name + color.
struct LabelEditorSheet: View {
    let title: String
    let confirmLabel: String
    let onConfirm: (String, String) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var name: String
    @State private var color: String

    init(
        title: String = "New label",
        confirmLabel: String = "Create",
        initialName: String = "",
        initialColor: String = suggestedLabelColors[0],
        onConfirm: @escaping (String, String) -> Void
    ) {
        self.title = title
        self.confirmLabel = confirmLabel
        self.onConfirm = onConfirm
        _name = State(initialValue: initialName)
        _color = State(initialValue: initialColor)
    }

    private var trimmedName: String {
        name.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    var body: some View {
        GlassSheetChrome(
            title: title,
            content: {
                VStack(alignment: .leading, spacing: 16) {
                    GlassTextField("Label name", text: $name)

                    FlowLayout(spacing: 8) {
                        ForEach(suggestedLabelColors, id: \.self) { swatch in
                            Button {
                                color = swatch
                            } label: {
                                Circle()
                                    .fill(Color(hex: swatch) ?? .gray)
                                    .frame(
                                        width: swatch == color ? 28 : 22,
                                        height: swatch == color ? 28 : 22
                                    )
                                    .overlay {
                                        if swatch == color {
                                            Circle().strokeBorder(.white, lineWidth: 2)
                                        }
                                    }
                            }
                            .buttonStyle(.plain)
                        }
                    }
                }
                .padding(.horizontal, 20)
                .padding(.bottom, 8)
            },
            primaryAction: {
                GlassSubmitButton(confirmLabel, enabled: !trimmedName.isEmpty) {
                    onConfirm(trimmedName, color)
                    dismiss()
                }
            }
        )
    }
}

/// Create-flow alias kept for the create-issue sheet (create the team label,
/// then add it to the local draft selection).
struct CreateLabelSheet: View {
    let onCreate: (String, String) -> Void

    var body: some View {
        LabelEditorSheet(onConfirm: onCreate)
    }
}
