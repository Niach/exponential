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
                GlassSheetSearchField(placeholder: "Search issues", text: $searchText)
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
                    ContentUnavailableView {
                        Label {
                            Text("No matching issues")
                        } icon: {
                            AppIcon(AppIcons.statusDuplicate, size: AppIcon.Size.xlarge)
                        }
                    } description: {
                        Text("Pick the canonical issue this one duplicates.")
                    }
                } else {
                    List {
                        ForEach(filtered, id: \.id) { issue in
                            Button {
                                onSelect(issue)
                                dismiss()
                            } label: {
                                HStack(spacing: 10) {
                                    AppIcon(IssueStatus.from(issue.status).iconName, size: AppIcon.Size.small)
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
