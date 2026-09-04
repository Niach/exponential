import ExpUI
import ExpCore
import SwiftUI

/// Issue picker for "Mark as duplicate…" (masterplan §5e): searchable list of
/// the team's other issues; selecting one atomically sets
/// `duplicateOfId` + `status = duplicate` via the caller. The shared sheet
/// chrome with a pinned search field; immediate commit on tap. The rows are
/// `IssueCandidateList`, shared with the relation picker (EXP-736).
struct DuplicatePickerSheet: View {
    /// Candidate canonical issues (same team, self excluded), newest first.
    let loadCandidates: () async -> [IssueEntity]
    let onSelect: (IssueEntity) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var candidates: [IssueEntity]?
    @State private var searchText = ""

    var body: some View {
        GlassSheetChrome(
            title: "Duplicate of",
            pinnedHeader: {
                GlassSheetSearchField(placeholder: "Search issues", text: $searchText)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
            },
            content: {
                IssueCandidateList(
                    candidates: candidates,
                    searchText: searchText,
                    emptyIcon: AppIcons.statusDuplicate,
                    emptyHint: "Pick the canonical issue this one duplicates.",
                    onSelect: { issue in
                        onSelect(issue)
                        dismiss()
                    }
                )
            }
        )
        .task {
            candidates = await loadCandidates()
        }
    }
}
