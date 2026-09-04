import ExpUI
import ExpCore
import SwiftUI

/// "Add relation" (EXP-736), in two stages inside ONE sheet: pick the KIND of
/// link (the six shared picks, in the shared order), then pick the issue on
/// the other end from the same searchable list the duplicate picker uses.
/// Backing out of stage two returns to the picks rather than dismissing.
struct RelationPickerSheet: View {
    /// Candidate issues (same team, self excluded), newest first.
    let loadCandidates: () async -> [IssueEntity]
    let onSelect: (RelationPick, IssueEntity) -> Void

    @Environment(\.dismiss) private var dismiss
    /// nil = stage one (the kind picker).
    @State private var pick: RelationPick?
    @State private var candidates: [IssueEntity]?
    @State private var searchText = ""

    var body: some View {
        GlassSheetChrome(
            title: pick?.title ?? "Add relation",
            pinnedHeader: {
                if pick != nil {
                    GlassSheetSearchField(placeholder: "Search issues", text: $searchText)
                        .padding(.horizontal, 16)
                        .padding(.vertical, 8)
                }
            },
            content: {
                if let pick {
                    VStack(alignment: .leading, spacing: 0) {
                        // The drill-down's own back control lives inside the
                        // content, like the filter sheet's (EXP-687).
                        Button {
                            self.pick = nil
                            searchText = ""
                        } label: {
                            HStack(spacing: 6) {
                                AppIcon(AppIcons.uiBack, size: AppIcon.Size.small)
                                Text("Add relation")
                                    .font(.subheadline)
                            }
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .padding(.horizontal, 20)
                            .frame(height: 36)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)

                        IssueCandidateList(
                            candidates: candidates,
                            searchText: searchText,
                            emptyIcon: pick.iconName,
                            emptyHint: "Pick the issue on the other end of this relation.",
                            onSelect: { issue in
                                onSelect(pick, issue)
                                dismiss()
                            }
                        )
                    }
                } else {
                    kindPicker
                }
            }
        )
    }

    private var kindPicker: some View {
        LazyVStack(spacing: 2) {
            ForEach(RelationPick.all) { entry in
                Button {
                    searchText = ""
                    pick = entry
                    // Loaded on demand: stage one is instant, and a sheet the
                    // user closes again never touches the store.
                    Task { candidates = await loadCandidates() }
                } label: {
                    HStack(spacing: 10) {
                        AppIcon(entry.iconName, size: AppIcon.Size.medium)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .frame(width: 24)
                        Text(entry.title)
                            .font(.subheadline)
                            .foregroundStyle(.white)
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
