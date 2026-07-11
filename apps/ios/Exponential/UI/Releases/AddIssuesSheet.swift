import ExpUI
import ExpCore
import SwiftUI

/// Multi-select issue picker for a release's "+" toolbar button: searchable
/// list of the workspace's addable issues (status not done/cancelled/duplicate,
/// not already in this release). Rows toggle membership — tapping never
/// dismisses; the bottom prominent "Add N issues" button commits the whole
/// selection at once. Forked from DuplicatePickerSheet's single-select layout.
struct AddIssuesSheet: View {
    /// Candidate issues (same workspace, this release excluded).
    let loadCandidates: () async -> [IssueEntity]
    let onConfirm: ([String]) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var candidates: [IssueEntity]?
    @State private var searchText = ""
    @State private var selected: Set<String> = []

    private var filtered: [IssueEntity] {
        guard let candidates else { return [] }
        let trimmed = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return candidates }
        return candidates.filter {
            $0.title.localizedCaseInsensitiveContains(trimmed)
                || ($0.identifier ?? "").localizedCaseInsensitiveContains(trimmed)
        }
    }

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                // Inline search field. NOT system .searchable — on iOS 26+ it
                // renders as a bottom-edge glass bar (see IssueListView).
                HStack(spacing: 8) {
                    Image(systemName: "magnifyingglass")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    TextField("Search issues", text: $searchText)
                        .textFieldStyle(.plain)
                        .submitLabel(.search)
                    if !searchText.isEmpty {
                        Button {
                            searchText = ""
                        } label: {
                            Image(systemName: "xmark.circle.fill")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        .buttonStyle(.plain)
                    }
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 9)
                .background(Color.white.opacity(0.06), in: RoundedRectangle(cornerRadius: 12))
                .padding(.horizontal, 16)
                .padding(.vertical, 8)

                pickerContent

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

    @ViewBuilder
    private var pickerContent: some View {
        Group {
            if candidates == nil {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if filtered.isEmpty {
                ContentUnavailableView(
                    "No matching issues",
                    systemImage: "shippingbox",
                    description: Text("Open workspace issues you can add will appear here.")
                )
            } else {
                List {
                    ForEach(filtered, id: \.id) { issue in
                        Button {
                            if selected.contains(issue.id) {
                                selected.remove(issue.id)
                            } else {
                                selected.insert(issue.id)
                            }
                        } label: {
                            HStack(spacing: 10) {
                                Image(systemName: IssueStatus.from(issue.status).sfSymbol)
                                    .font(.caption)
                                    .foregroundStyle(IssueStatus.from(issue.status).color)
                                if let identifier = issue.identifier {
                                    Text(identifier)
                                        .font(.caption.monospaced())
                                        .foregroundStyle(.secondary)
                                }
                                Text(issue.title)
                                    .lineLimit(1)
                                Spacer()
                                if selected.contains(issue.id) {
                                    Image(systemName: "checkmark")
                                        .font(.subheadline.weight(.semibold))
                                        .foregroundStyle(.tint)
                                }
                            }
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
        }
    }
}
