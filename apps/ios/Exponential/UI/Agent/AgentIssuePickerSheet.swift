import ExpCore
import ExpUI
import SwiftUI

/// EXP-825: the composer's issue picker, the "+" menu's Implement issue ›
/// page: a searchable checklist over the team's eligible issues
/// (`IssueOption`), the rows checked at OPEN pinned first. The pin order is
/// snapshotted at open and never re-sorts on toggle, so a tapped row visibly
/// checks in place instead of teleporting into a pinned group (EXP-241).
/// Checking an issue swaps out an action chip; the single-repository and
/// batch-cap guards caption the list.
///
/// SLOP-28 (EXP-1259 D5): it is THE search list (`SearchListView`,
/// search-list.json), pushed as a page of the "+" sheet (`ComposerPlusMenu`):
/// the search input on top, the picker's own rows (`IDENT Title` behind the
/// status glyph) under it, ranked by the shared engine at its DEFAULT limit.
/// Once something is picked the footer carries composer-menu.json's
/// `implementButton` (`Implement N issues`), which only closes the menu: the
/// picks already are the run's subject.
struct AgentIssuePickerPage: View {
    let model: AgentComposerModel
    /// Closes the whole "+" menu.
    let onClose: () -> Void

    @Environment(\.motion) private var motion
    @State private var searchText = ""
    /// The open-time snapshot of the checked rows (the pin order).
    @State private var pinnedIds: Set<String>

    init(model: AgentComposerModel, onClose: @escaping () -> Void) {
        self.model = model
        self.onClose = onClose
        _pinnedIds = State(initialValue: Set(model.effectiveChecked))
    }

    var body: some View {
        SearchListView(
            items: IssuePicker<EmptyView>.items(rows.map { option in
                let status = IssueStatus.from(option.status)
                return IssuePickerIssue(
                    id: option.id,
                    identifier: option.identifier ?? "",
                    title: option.title,
                    icon: status.iconName,
                    color: status.color
                )
            }),
            query: $searchText,
            placeholder: IssueSearchCopy.placeholder,
            mode: .multi,
            value: Set(model.effectiveChecked),
            inputIdentifier: ComposerMenu.submenuSearchTestId,
            // Why the list is empty, not just that it is: a team with nothing
            // codeable and a query with no hit are different answers.
            emptyText: model.issues.isEmpty ? "No eligible issues to code." : IssueSearchCopy.noMatches,
            footer: showsFooter ? { AnyView(footer) } : nil,
            onPick: { id in
                withAnimation(motion.standard) { model.toggleIssue(id) }
            }
        )
        .accessibilityIdentifier("agent-composer-issues-picker")
    }

    /// Pinned rows first (the open-time snapshot), then the rest, both ranked
    /// by the shared `IssueSearch` engine (EXP-892) at its default limit
    /// (SLOP-28: no per-surface cap).
    private var rows: [IssueOption] {
        let pool = model.issues
        let pinned = IssueSearch.rank(
            pool.filter { pinnedIds.contains($0.id) },
            query: searchText,
            limit: Int.max,
            projection: \.searchRow
        )
        let others = IssueSearch.rank(
            pool.filter { !pinnedIds.contains($0.id) },
            query: searchText,
            projection: \.searchRow
        )
        return pinned + others
    }

    private var showsGuards: Bool {
        model.multiRepo || model.overCap || model.costWarning
    }

    private var pickedCount: Int { model.effectiveChecked.count }

    private var showsFooter: Bool { showsGuards || pickedCount > 0 }

    /// The guard captions over the `Implement N issues` button.
    private var footer: some View {
        VStack(alignment: .leading, spacing: 0) {
            if showsGuards { guards }
            if pickedCount > 0 {
                GlassSubmitButton(ComposerMenu.implementButtonLabel(pickedCount)) {
                    onClose()
                }
                .accessibilityIdentifier(ComposerMenu.implementSubmitTestId)
                .padding(.horizontal, GlassPickerTokens.rowHPadding)
                .padding(.vertical, 10)
            }
        }
    }

    /// The batch guards, under the rows: what cannot start, said where the
    /// picking happens. Invalid input, so it stays inline (FEED-79).
    private var guards: some View {
        VStack(alignment: .leading, spacing: 4) {
            if model.multiRepo {
                Text("Pick issues from a single repository per run.")
                    .foregroundStyle(DesignTokens.Semantic.red)
            }
            if model.overCap {
                Text("At most \(AgentComposerModel.maxBatchIssues) issues per run. Split the batch.")
                    .foregroundStyle(DesignTokens.Semantic.red)
            } else if model.costWarning {
                Text("Large batches are token-expensive.")
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        }
        .font(.system(size: CGFloat(FeedbackStates.Errors.inlineSize)))
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, GlassPickerTokens.rowHPadding)
        .padding(.vertical, 10)
    }
}
