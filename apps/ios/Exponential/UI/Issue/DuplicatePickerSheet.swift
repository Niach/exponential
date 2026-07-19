import ExpUI
import ExpCore
import SwiftUI

/// Issue picker for "Mark as duplicate…" (masterplan §5e): searchable list of
/// the team's other issues; selecting one atomically sets
/// `duplicateOfId` + `status = duplicate` via the caller. Matches the
/// PickerSheet look (medium/large detent, immediate commit on tap).
struct DuplicatePickerSheet: View {
    /// Candidate canonical issues (same team, self excluded), newest first.
    let loadCandidates: () async -> [IssueEntity]
    let onSelect: (IssueEntity) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var candidates: [IssueEntity]?
    @State private var searchText = ""

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
            }
            .navigationTitle("Duplicate of")
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
                        systemImage: "doc.on.doc",
                        description: Text("Pick the canonical issue this one duplicates.")
                    )
                } else {
                    List {
                        ForEach(filtered, id: \.id) { issue in
                            Button {
                                onSelect(issue)
                                dismiss()
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
