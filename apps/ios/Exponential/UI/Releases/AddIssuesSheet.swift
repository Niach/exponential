import ExpUI
import ExpCore
import SwiftUI

/// Multi-select issue picker for a release's "+" toolbar button: the shared
/// IssueMultiSelectPicker over the workspace's addable issues (status not
/// done/cancelled/duplicate, not already in this release). Rows toggle
/// membership — tapping never dismisses; the bottom prominent "Add N issues"
/// button commits the whole selection at once via releases.addIssues.
struct AddIssuesSheet: View {
    /// Candidate issues (same workspace, this release excluded).
    let loadCandidates: () async -> [IssueEntity]
    let onConfirm: ([String]) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var candidates: [IssueEntity]?
    @State private var selected: Set<String> = []

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                IssueMultiSelectPicker(candidates: candidates, selected: $selected)

                Button {
                    onConfirm(Array(selected))
                    dismiss()
                } label: {
                    Text("Add \(selected.count) issue\(selected.count == 1 ? "" : "s")")
                        .font(.subheadline.weight(.medium))
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 12)
                }
                .buttonStyle(.borderedProminent)
                .disabled(selected.isEmpty)
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
            }
            .navigationTitle("Add issues")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
            }
        }
        .presentationDetents([.medium, .large])
        .task {
            candidates = await loadCandidates()
        }
    }
}
