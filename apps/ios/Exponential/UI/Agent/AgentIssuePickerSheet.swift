import ExpCore
import ExpUI
import SwiftUI

/// EXP-825: the composer's issue picker — the `#` tool's searchable checklist
/// over the team's eligible issues (`IssueOption`), the rows checked at OPEN
/// pinned first. The pin order is snapshotted at open and never re-sorts on
/// toggle, so a tapped row visibly checks in place instead of teleporting into
/// a pinned group (EXP-241). Checking an issue swaps out an action chip; the
/// single-repository and batch-cap guards caption the list.
///
/// EXP-1030: it renders through the SHARED `IssuePicker` in multi mode — the
/// one sheet, the one selection language (a checked row reads by its own
/// highlight, never a leading circle), rows `IDENT Title` behind the issue's
/// status glyph. This view is what is left of it: the ranked pool, the pin
/// snapshot and the guard captions, which ride the picker's footer. It is
/// HOST-DRIVEN (`open`): the composer's tool button is the trigger, and it
/// lives in another view tree.
struct AgentIssuePickerSheet: View {
    let model: AgentComposerModel
    /// The composer's `#` button drives it.
    @Binding var isPresented: Bool

    @Environment(\.motion) private var motion
    @State private var searchText = ""
    @State private var pinnedIds: Set<String> = []

    var body: some View {
        IssuePicker(
            issues: rows.map { option in
                let status = IssueStatus.from(option.status)
                return IssuePickerIssue(
                    id: option.id,
                    identifier: option.identifier ?? "",
                    title: option.title,
                    icon: status.iconName,
                    color: status.color
                )
            },
            mode: .multi,
            value: Set(model.effectiveChecked),
            // The picker reports the WHOLE new set; the model checks one issue
            // at a time (a pick also swaps out an action), so the difference is
            // what changed.
            onChange: { picked in
                let current = Set(model.effectiveChecked)
                withAnimation(motion.standard) {
                    for id in picked.symmetricDifference(current) { model.toggleIssue(id) }
                }
            },
            // EXP-892: the rows are RANKED here, so the primitive renders them
            // verbatim and only reports what was typed.
            query: $searchText,
            open: $isPresented,
            hideTrigger: true,
            // Why the list is empty, not just that it is: a team with nothing
            // codeable and a query with no hit are different answers.
            emptyText: model.issues.isEmpty ? "No eligible issues to code." : "No matching issues.",
            footer: showsGuards ? { AnyView(guards) } : nil,
            sheetIdentifier: "agent-composer-issues-picker",
            trigger: { EmptyView() }
        )
        // The pin order is the snapshot taken when the sheet OPENS.
        .onChange(of: isPresented) { _, open in
            if open { pinnedIds = Set(model.effectiveChecked) }
        }
    }

    /// Pinned rows first (the open-time snapshot), then the rest, both ranked
    /// by the shared `IssueSearch` engine (EXP-892) and the tail capped so a
    /// big team stays scrollable.
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
            limit: 50,
            projection: \.searchRow
        )
        return pinned + others
    }

    private var showsGuards: Bool {
        model.multiRepo || model.overCap || model.costWarning
    }

    /// The batch guards, under the rows — what cannot start, said where the
    /// picking happens.
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
        .font(.caption)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, GlassPickerTokens.rowHPadding)
        .padding(.vertical, 10)
    }
}
