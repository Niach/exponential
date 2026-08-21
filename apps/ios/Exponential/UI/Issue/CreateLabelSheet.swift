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
    // Natural form height → a fitted `.height` detent (EXP-577: the medium
    // detent left a third of the sheet empty on both platforms' reference).
    @State private var contentHeight: CGFloat = 0
    // Home-indicator inset — part of a `.height` detent (keyboard values are
    // ignored so a focused field doesn't inflate the sheet).
    @State private var bottomInset: CGFloat = 0

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

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(title)
                .font(.headline)
                .foregroundStyle(.white)

            TextField("Label name", text: $name)
                .textFieldStyle(.plain)
                .foregroundStyle(.white)
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
                .background(Color.white.opacity(0.06), in: RoundedRectangle(cornerRadius: 10))

            FlowLayout(spacing: 8) {
                ForEach(suggestedLabelColors, id: \.self) { swatch in
                    Button {
                        color = swatch
                    } label: {
                        Circle()
                            .fill(Color(hex: swatch) ?? .gray)
                            .frame(width: swatch == color ? 28 : 22, height: swatch == color ? 28 : 22)
                            .overlay {
                                if swatch == color {
                                    Circle().strokeBorder(.white, lineWidth: 2)
                                }
                            }
                    }
                    .buttonStyle(.plain)
                }
            }

            HStack {
                Button("Cancel") {
                    dismiss()
                }
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .buttonStyle(.plain)

                Spacer()

                Button {
                    let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
                    guard !trimmed.isEmpty else { return }
                    onConfirm(trimmed, color)
                    dismiss()
                } label: {
                    Text(confirmLabel)
                        .font(.subheadline.weight(.medium))
                        .padding(.horizontal, 24)
                        .padding(.vertical, 10)
                }
                .buttonStyle(.borderedProminent)
                .disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
        .padding(20)
        .onGeometryChange(for: CGFloat.self, of: { $0.size.height }) { contentHeight = $0 }
        // Top-aligned: a sheet centers shorter content by default, which
        // would float the form once the keyboard shrinks the detent.
        .frame(maxHeight: .infinity, alignment: .top)
        .onGeometryChange(for: CGFloat.self, of: { $0.safeAreaInsets.bottom }) { inset in
            if inset < 60 { bottomInset = inset }
        }
        .presentationDetents(contentHeight > 0 ? [.height(contentHeight + bottomInset)] : [.medium])
        .presentationDragIndicator(.hidden)
        .presentationBackground(.ultraThinMaterial)
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
