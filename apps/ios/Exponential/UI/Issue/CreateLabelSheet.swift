import ExpUI
import ExpCore
import SwiftUI

// MARK: - Create label

// Same suggested palette as Android's LabelPickerSheet / the web's label editor.
let suggestedLabelColors = [
    "#ef4444", "#dc2626", "#f97316", "#f59e0b", "#eab308",
    "#84cc16", "#22c55e", "#10b981", "#14b8a6", "#06b6d4",
    "#0ea5e9", "#3b82f6", "#6366f1", "#8b5cf6", "#a855f7",
    "#ec4899", "#f43f5e", "#78716c", "#64748b", "#a3a3a3",
]

/// Minimal name + color form. Shared by the issue detail editor (create +
/// assign to the issue in one step) and the create-issue sheet (create the
/// workspace label, then add it to the local draft selection) — the caller's
/// `onCreate` closure decides what happens with the new name + color.
struct CreateLabelSheet: View {
    let onCreate: (String, String) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var color = suggestedLabelColors[0]

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("New label")
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

            Spacer()

            Button {
                let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
                guard !trimmed.isEmpty else { return }
                onCreate(trimmed, color)
                dismiss()
            } label: {
                Text("Create label")
                    .font(.subheadline.weight(.medium))
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 12)
            }
            .buttonStyle(.borderedProminent)
            .disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }
        .padding(20)
    }
}
