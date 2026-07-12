import ExpUI
import ExpCore
import SwiftUI

/// Release creation sheet (EXP-62): name (optional — blank lets the server
/// auto-name "Release N") + the shared IssueMultiSelectPicker so the issues
/// are picked BEFORE the release exists. Create stays disabled until at
/// least one issue is selected — an empty release is useless. Mirrors the
/// web/Android/IDE creation dialogs; one releases.create call attaches the
/// bundle in the same server transaction.
struct CreateReleaseSheet: View {
    /// Candidate issues (the workspace's still-actionable issues).
    let loadCandidates: () async -> [IssueEntity]
    /// Runs the create (API call + wait-for-sync + navigation). Returns an
    /// error message on failure, nil on success — success dismisses the
    /// sheet; failure renders inline and keeps the selection.
    let onCreate: (_ name: String?, _ issueIds: [String]) async -> String?

    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var candidates: [IssueEntity]?
    @State private var selected: Set<String> = []
    @State private var creating = false
    @State private var error: String?

    private var createLabel: String {
        if creating { return "Creating…" }
        switch selected.count {
        case 0: return "Create release"
        case 1: return "Create with 1 issue"
        default: return "Create with \(selected.count) issues"
        }
    }

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                TextField("Release name (optional)", text: $name)
                    .textFieldStyle(.plain)
                    .submitLabel(.done)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 9)
                    .background(Color.white.opacity(0.06), in: RoundedRectangle(cornerRadius: 12))
                    .padding(.horizontal, 16)
                    .padding(.top, 8)

                IssueMultiSelectPicker(candidates: candidates, selected: $selected)

                if let error {
                    Text(error)
                        .font(.caption)
                        .foregroundStyle(.red)
                        .padding(.horizontal, 16)
                        .padding(.top, 8)
                }

                Button {
                    Task { await create() }
                } label: {
                    Text(createLabel)
                        .font(.subheadline.weight(.medium))
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 12)
                }
                .buttonStyle(.borderedProminent)
                .disabled(selected.isEmpty || creating)
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
            }
            .navigationTitle("New release")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                        .disabled(creating)
                }
            }
        }
        .presentationDetents([.medium, .large])
        .interactiveDismissDisabled(creating)
        .task {
            candidates = await loadCandidates()
        }
    }

    private func create() async {
        guard !creating, !selected.isEmpty else { return }
        creating = true
        defer { creating = false }
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        if let message = await onCreate(trimmed.isEmpty ? nil : trimmed, Array(selected)) {
            error = message
        } else {
            error = nil
            dismiss()
        }
    }
}
