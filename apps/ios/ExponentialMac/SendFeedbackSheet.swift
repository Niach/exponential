import AppKit
import ExpCore
import SwiftUI

/// The "file feedback" sheet shown after the user finishes annotating. Collects
/// a title + description and a thumbnail of the flattened screenshot, then files
/// via MacFeedbackReporter (authenticated-direct, as the logged-in developer).
struct SendFeedbackSheet: View {
    @Environment(\.dismiss) private var dismiss

    let accountId: String
    let projectId: String
    let targetName: String
    // The pre-flattened screenshot (annotations baked in) — nil if flatten failed.
    let screenshot: MacFeedbackReporter.Screenshot?
    // A small preview image for the sheet (the flattened frame).
    let thumbnail: NSImage?
    let reporter: MacFeedbackReporter
    let toasts: MacToastCenter
    let onFiled: () -> Void

    @State private var title = ""
    @State private var description = ""
    @State private var submitting = false
    @State private var error: String?

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Send feedback").font(.headline)
                Spacer()
                Text(targetName).font(.caption).foregroundStyle(.secondary)
            }
            .padding()
            Divider()

            Form {
                Section {
                    TextField("Title", text: $title)
                        .textFieldStyle(.roundedBorder)
                    TextField("What's wrong? (optional)", text: $description, axis: .vertical)
                        .lineLimit(3...8)
                        .textFieldStyle(.roundedBorder)
                }
                if let thumbnail {
                    Section("Screenshot") {
                        Image(nsImage: thumbnail)
                            .resizable()
                            .scaledToFit()
                            .frame(maxHeight: 200)
                            .clipShape(RoundedRectangle(cornerRadius: 8))
                            .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Color.white.opacity(0.08)))
                    }
                } else if screenshot == nil {
                    Section {
                        Label("Screenshot couldn't be captured — filing text only.", systemImage: "exclamationmark.triangle")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }
                if let error {
                    Text(error).foregroundStyle(.red).font(.callout)
                }
            }
            .formStyle(.grouped)

            Divider()
            HStack {
                Button("Cancel", role: .cancel) { dismiss() }
                Spacer()
                Button(submitting ? "Filing…" : "File issue") { submit() }
                    .buttonStyle(.borderedProminent)
                    .disabled(submitting || title.trimmingCharacters(in: .whitespaces).isEmpty)
            }
            .padding()
        }
        .frame(width: 460, height: 540)
    }

    private func submit() {
        submitting = true
        error = nil
        Task {
            do {
                let filed = try await reporter.file(
                    accountId: accountId,
                    projectId: projectId,
                    title: title,
                    description: description,
                    screenshot: screenshot
                )
                if filed.screenshotAttached {
                    toasts.show("Feedback filed.", style: .success)
                } else if screenshot != nil {
                    toasts.show("Feedback filed — screenshot upload failed.", style: .error)
                } else {
                    toasts.show("Feedback filed.", style: .success)
                }
                onFiled()
                dismiss()
            } catch {
                self.error = error.localizedDescription
                submitting = false
            }
        }
    }
}
