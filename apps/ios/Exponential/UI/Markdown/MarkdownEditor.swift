import MarkdownUI
import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

struct MarkdownEditor: View {
    @Binding var text: String
    @Binding var pendingImages: [String: PendingImage]
    var placeholder: String = "Write something..."

    @State private var isEditing = true
    @State private var photoItem: PhotosPickerItem?

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

                MarkdownToolbar(
                    text: $text,
                    photoItem: $photoItem
                )
            } else {
                ScrollView {
                    if text.isEmpty {
                        Text("Nothing to preview")
                            .font(.body)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(8)
                    } else {
                        Markdown(renderedTextForPreview)
                            .markdownTheme(.gitHub)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(8)
                    }
                }
                .frame(minHeight: 120)
            }
        }
        .onChange(of: photoItem) { _, newItem in
            guard let newItem else { return }
            Task { await ingest(newItem) }
        }
    }

    // Draft images can't be resolved by the markdown previewer since their
    // URL scheme isn't `https`. Replace placeholders with a simple alt-text
    // marker for the preview so the user sees "[image: filename.jpg]"
    // instead of a broken image icon.
    private var renderedTextForPreview: String {
        var result = text
        for (placeholder, image) in pendingImages {
            result = result.replacingOccurrences(
                of: "(\(placeholder))",
                with: "(\(placeholder)) _[uploading \(image.filename)]_"
            )
        }
        return result
    }

    private func ingest(_ item: PhotosPickerItem) async {
        defer { photoItem = nil }
        guard let data = try? await item.loadTransferable(type: Data.self) else { return }
        let contentType = item.supportedContentTypes.first?.preferredMIMEType ?? "image/jpeg"
        let ext = item.supportedContentTypes.first?.preferredFilenameExtension ?? "jpg"
        let filename = "image-\(Int(Date().timeIntervalSince1970)).\(ext)"
        let placeholder = MarkdownImageUtils.draftUrl()
        pendingImages[placeholder] = PendingImage(
            data: data,
            filename: filename,
            contentType: contentType
        )
        let insertion = "\n![image](\(placeholder))\n"
        text += insertion
    }
}
