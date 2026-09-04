import ExpUI
import ExpCore
import SwiftUI

/// The combined Properties sheet (EXP-240): one glass sheet listing every
/// editable property. EXP-698 r5 made it the New-issue page's own rows —
/// `GlassMetaRow`s in one `.glassSection()` group (Status / Priority /
/// Assignee / Due date / Board), each STACKING its per-property picker over
/// this sheet (EXP-687 retired the dismiss-and-re-present hand-off; dismissing
/// the child returns here, exactly like Android) — followed by the shared
/// `IssueLabelsSelector`, whose pills toggle inline and whose add chip opens
/// the searchable Labels sheet, and by `IssueRelationsSection` (EXP-736). The
/// Board row hides when there is nowhere to move.
struct IssuePropertiesSheet<Child: View>: View {
    let issue: IssueEntity
    /// EXP-314: the issue's status resolved against its team's status rows.
    let status: ResolvedIssueStatus
    let assignee: UserEntity?
    /// The issue's team's labels, name-sorted by the caller.
    let labels: [LabelEntity]
    let assignedIds: Set<String>
    /// EXP-736: this issue's relations, read from its own side. Mobile shows
    /// them ONLY here — the detail page keeps chips + timeline.
    let relations: [IssueRelationRow]
    let singleMemberTeam: Bool
    /// The issue's own board — the row draws its glyph + color (EXP-449).
    let board: BoardEntity?
    let hasMoveTargets: Bool
    let onToggleLabel: (String) -> Void
    let onRemoveRelation: (IssueRelationRow) -> Void
    /// The picker stacked OVER this sheet. Host-owned: a picker that hands off
    /// to another one (duplicate status) is promoted by the host on dismiss.
    @Binding var activeChild: IssuePropertyChild?
    /// Fired once a child finished dismissing — the host promotes whatever a
    /// picker parked (a hand-off target, the picked move board).
    let onChildDismiss: () -> Void
    /// The per-property pickers, built by the host (they need the view model).
    @ViewBuilder let child: (IssuePropertyChild) -> Child

    var body: some View {
        let priority = IssuePriority.from(issue.priority)
        GlassSheetChrome(title: "Properties") {
            VStack(alignment: .leading, spacing: 16) {
                // EXP-698 r5: the New-issue page's rows, verbatim — one
                // `.glassSection()` group of `GlassMetaRow`s separated by
                // hairlines, value-led glyphs, no chevrons. The sheet used to
                // draw its own row (leading gutter glyph + trailing chevron),
                // so the same five properties looked like two different lists
                // depending on whether the issue existed yet.
                VStack(spacing: 0) {
                    GlassMetaRow(
                        label: "Status",
                        icon: status.iconName,
                        iconColor: status.color,
                        value: status.name
                    ) { activeChild = .status }

                    GlassDivider()

                    GlassMetaRow(
                        label: "Priority",
                        icon: priority.iconName,
                        iconColor: priority.color,
                        value: priority.label
                    ) { activeChild = .priority }

                    // Solo team: no one else to reassign to (EXP-50).
                    if !singleMemberTeam {
                        GlassDivider()

                        GlassMetaRow(
                            label: "Assignee",
                            icon: issue.assigneeId == nil ? AppIcons.uiUnassigned : AppIcons.uiAssignee,
                            iconColor: .white.opacity(TextOpacity.secondary),
                            value: issue.assigneeId.map { memberDisplayName(assignee, id: $0) } ?? "Unassigned"
                        ) { activeChild = .assignee }
                    }

                    GlassDivider()

                    GlassMetaRow(
                        label: "Due date",
                        icon: AppIcons.uiDueDate,
                        iconColor: .white.opacity(TextOpacity.secondary),
                        value: issue.dueDate.map(dueDateChipLabel) ?? "None"
                    ) { activeChild = .dueDate }

                    // The move picker hides when there is nowhere to move to.
                    if hasMoveTargets {
                        GlassDivider()

                        GlassMetaRow(
                            label: "Board",
                            // The board's own glyph + color, not a generic
                            // boards nav icon (EXP-449).
                            icon: board.map { BoardTypeDisplay.iconName(for: $0) } ?? AppIcons.navBoards,
                            iconColor: board.flatMap { Color(hex: $0.color ?? "#888888") }
                                ?? .white.opacity(TextOpacity.secondary),
                            value: board?.name ?? ""
                        ) { activeChild = .moveBoard }
                    }
                }
                .glassSection()

                // Labels: every team label as a select pill (assigned ones
                // read selected), the add chip hands off to the searchable
                // sheet — the same block the New-issue page renders.
                IssueLabelsSelector(
                    labels: labels,
                    selectedIds: assignedIds,
                    onToggle: onToggleLabel,
                    onAdd: { activeChild = .labels }
                )

                // EXP-736: relations sit under the labels, entry point first.
                IssueRelationsSection(
                    relations: relations,
                    onAdd: { activeChild = .addRelation },
                    onRemove: onRemoveRelation
                )
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 24)
            // The child rides the INNER node — the chrome root carries the
            // move confirm, and no node may own two presentations (EXP-240).
            .sheet(item: $activeChild, onDismiss: onChildDismiss) { target in
                child(target)
            }
        }
    }
}
