import ExpUI
import ExpCore
import SwiftUI

/// "Add relation" (EXP-736), in two stages: pick the KIND of link (the six
/// shared picks, in the shared order), then pick the issue on the other end.
///
/// EXP-1021 moved stage two onto the SHARED `IssuePicker`
/// (`IssueCandidatePicker`, the same rows the duplicate picker shows), so it
/// stacks OVER this sheet instead of replacing its content. Backing out of it
/// is a swipe down, which lands back on the picks exactly as the in-content
/// back button used to — and there is one fewer bespoke list in the app.
struct RelationPickerSheet: View {
    /// Candidate issues (same team, self excluded), newest first.
    let loadCandidates: () async -> [IssueEntity]
    /// EXP-892 — the picker's server augmentation: a debounced
    /// `issues.search` so a query that only matches a COMMENT still finds
    /// its issue. Hits outside `loadCandidates`' pool are dropped.
    var serverSearch: ((String) async -> [SearchIssueHit])?
    let onSelect: (RelationPick, IssueEntity) -> Void

    @Environment(\.dismiss) private var dismiss
    /// nil = stage one (the kind picker) is the only thing on screen.
    @State private var pick: RelationPick?
    @State private var candidates: [IssueEntity]?

    var body: some View {
        GlassSheetChrome(title: "Add relation") {
            kindPicker
                // Stage two is its own presentation, on its own node.
                .background {
                    IssueCandidatePicker(
                        candidates: candidates,
                        open: Binding(
                            get: { pick != nil },
                            set: { isOpen in if !isOpen { pick = nil } }
                        ),
                        serverSearch: serverSearch,
                        onSelect: { issue in
                            guard let pick else { return }
                            onSelect(pick, issue)
                            self.pick = nil
                            dismiss()
                        }
                    )
                }
        }
    }

    private var kindPicker: some View {
        LazyVStack(spacing: GlassPickerTokens.rowSpacing) {
            ForEach(RelationPick.all) { entry in
                Button {
                    pick = entry
                    // Loaded on demand: stage one is instant, and a sheet the
                    // user closes again never touches the store.
                    if candidates == nil {
                        Task { candidates = await loadCandidates() }
                    }
                } label: {
                    HStack(spacing: 10) {
                        AppIcon(entry.iconName, size: AppIcon.Size.medium)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .frame(width: GlassPickerTokens.markWidth)
                        Text(entry.title)
                            .font(.subheadline)
                            .foregroundStyle(.white)
                        Spacer(minLength: 0)
                    }
                    .padding(.horizontal, GlassPickerTokens.rowHPadding)
                    .frame(minHeight: GlassPickerTokens.rowMinHeight)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, GlassPickerTokens.listHPadding)
        .padding(.bottom, GlassPickerTokens.listBottomPadding)
    }
}
