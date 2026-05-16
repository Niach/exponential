import MarkdownUI
import SwiftUI

struct MarkdownEditor: View {
    @Binding var text: String
    var placeholder: String = "Write something..."
    @State private var isEditing = true

    var body: some View {
        VStack(spacing: 0) {
            // Toggle bar
            HStack {
                Button {
                    isEditing = true
                } label: {
                    Text("Edit")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.white.opacity(isEditing ? 1.0 : TextOpacity.tertiary))
                        .padding(.horizontal, 12)
                        .padding(.vertical, 6)
                }
                .glassButton(isActive: isEditing)

                Button {
                    isEditing = false
                } label: {
                    Text("Preview")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.white.opacity(!isEditing ? 1.0 : TextOpacity.tertiary))
                        .padding(.horizontal, 12)
                        .padding(.vertical, 6)
                }
                .glassButton(isActive: !isEditing)

                Spacer()
            }
            .padding(.horizontal, 4)
            .padding(.vertical, 6)

            if isEditing {
                // Edit mode - plain text
                TextEditor(text: $text)
                    .font(.body)
                    .foregroundStyle(.white)
                    .scrollContentBackground(.hidden)
                    .frame(minHeight: 120)
                    .padding(8)
                    .background(Color.white.opacity(0.04))
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                    .overlay(
                        RoundedRectangle(cornerRadius: 8)
                            .stroke(Color.white.opacity(0.08), lineWidth: 0.5)
                    )

                // Toolbar
                MarkdownToolbar(text: $text)
            } else {
                // Preview mode
                ScrollView {
                    if text.isEmpty {
                        Text("Nothing to preview")
                            .font(.body)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(8)
                    } else {
                        Markdown(text)
                            .markdownTheme(.gitHub)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(8)
                    }
                }
                .frame(minHeight: 120)
            }
        }
    }
}
