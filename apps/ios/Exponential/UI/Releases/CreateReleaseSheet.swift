import ExpUI
import ExpCore
import SwiftUI

/// Minimal name + optional-description form for a new workspace release
/// (EXP-56). Mirrors CreateLabelSheet's glass styling; the caller's `onCreate`
/// closure runs the tRPC mutation — the new row then arrives via sync.
struct CreateReleaseSheet: View {
    let onCreate: (_ name: String, _ description: String?) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var description = ""

    private var trimmedName: String {
        name.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("New release")
                .font(.headline)
                .foregroundStyle(.white)

            TextField("Release name", text: $name)
                .textFieldStyle(.plain)
                .foregroundStyle(.white)
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
                .background(Color.white.opacity(0.06), in: RoundedRectangle(cornerRadius: 10))

            TextField("Description (optional)", text: $description, axis: .vertical)
                .textFieldStyle(.plain)
                .lineLimit(3...6)
                .foregroundStyle(.white)
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
                .background(Color.white.opacity(0.06), in: RoundedRectangle(cornerRadius: 10))

            Spacer()

            Button {
                guard !trimmedName.isEmpty else { return }
                let trimmedDescription = description.trimmingCharacters(in: .whitespacesAndNewlines)
                onCreate(trimmedName, trimmedDescription.isEmpty ? nil : trimmedDescription)
                dismiss()
            } label: {
                Text("Create release")
                    .font(.subheadline.weight(.medium))
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 12)
            }
            .buttonStyle(.borderedProminent)
            .disabled(trimmedName.isEmpty)
        }
        .padding(20)
    }
}
