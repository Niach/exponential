import ExpUI
import ExpCore
import SwiftUI

/// Issue picker for "Mark as duplicate…" (masterplan §5e): searchable list of
/// the team's other issues; selecting one atomically sets
/// `duplicateOfId` + `status = duplicate` via the caller. The shared sheet
/// chrome with a pinned search field; immediate commit on tap.
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
        GlassSheetChrome(
            title: "Duplicate of",
            pinnedHeader: {
                GlassSheetSearchField(placeholder: "Search issues", text: $searchText)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
            },
            content: {
                pickerContent
            }
        )
        .task {
            candidates = await loadCandidates()
        }
    }

    @ViewBuilder
    private var pickerContent: some View {
        if candidates == nil {
            ProgressView()
                .frame(maxWidth: .infinity)
                .padding(.vertical, 40)
        } else if filtered.isEmpty {
            VStack(spacing: 8) {
                AppIcon(AppIcons.statusDuplicate, size: AppIcon.Size.xlarge)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                Text("No matching issues")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Text("Pick the canonical issue this one duplicates.")
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .multilineTextAlignment(.center)
            }
            .frame(maxWidth: .infinity)
            .padding(.horizontal, 32)
            .padding(.vertical, 32)
        } else {
            // A `ScrollView` of rows, not a `List`: the chrome measures its
            // content, and a List reports an unbounded height (EXP-687).
            LazyVStack(spacing: 2) {
                ForEach(filtered, id: \.id) { issue in
                    Button {
                        onSelect(issue)
                        dismiss()
                    } label: {
                        HStack(spacing: 10) {
                            AppIcon(IssueStatus.from(issue.status).iconName, size: AppIcon.Size.small)
                                .foregroundStyle(IssueStatus.from(issue.status).color)
                                .frame(width: 24)
                            if let identifier = issue.identifier {
                                Text(identifier)
                                    .font(.caption.monospaced())
                                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            }
                            Text(issue.title)
                                .font(.subheadline)
                                .foregroundStyle(.white)
                                .lineLimit(1)
                            Spacer(minLength: 0)
                        }
                        .padding(.horizontal, 14)
                        .frame(minHeight: 44)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.horizontal, 6)
            .padding(.bottom, 16)
        }
    }
}
